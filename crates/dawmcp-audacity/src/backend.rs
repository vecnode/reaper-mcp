//! `dawmcp-core` trait impl for Audacity, over `mod-script-pipe`.
//!
//! Every command name and parameter key used here is verified against
//! Audacity's own source (`audacity/audacity` on GitHub, `au3/src/commands/`
//! and `au3/src/menus/*.cpp`), not assumed from memory or third-party docs -
//! this project has been burned before by unverified API assumptions.
//! Specifically:
//! - `Play`/`Stop`/`Pause`/`Record2ndChoice`: `au3/src/menus/TransportMenus.cpp`
//! - `SelectTracks: Track=<n> TrackCount=<n> Mode=Set`: `au3/src/commands/SelectCommand.cpp`
//!   (`SelectTracksCommand::VisitSettings` - param is `Track`, not `FirstTrack`)
//! - `SelectAll`, `SelectTime: Start=<n> End=<n>`: `au3/src/menus/SelectMenus.cpp`, `SelectCommand.cpp`
//! - `SetTrackStatus: Name=<s>`, `SetTrackAudio: Mute=<0|1> Solo=<0|1> Volume=<dB> Pan=<-100..100>`:
//!   `au3/src/commands/SetTrackInfoCommand.cpp` (`SetTrackAudioCommand::ApplyInner` confirms
//!   `Volume` is dB via `DB_TO_LINEAR`, `Pan` is `-100..100` mapped to internal `-1.0..1.0`)
//! - `NewMonoTrack`/`NewStereoTrack`: `au3/src/tracks/playabletrack/wavetrack/ui/WaveTrackMenuItems.cpp`
//! - `RemoveTracks`: `au3/src/menus/TrackMenus.cpp`
//! - `GetInfo: Type=Tracks Format=JSON` response shape (array of
//!   `{name, focused, selected, kind, start, end, pan, volume, channels,
//!   solo, mute, VZoomMin, VZoomMax}`, index = array position, `volume` is
//!   a *linear* gain factor not dB): `au3/src/commands/GetInfoCommand.cpp::SendTracks`
//! - `Export2: Filename=<path> NumChannels=<1|2>`: `au3/src/commands/ImportExportCommands.cpp`
//! - `Save`, `Undo`: `au3/src/menus/FileMenus.cpp`, `EditMenus.cpp`
//!
//! Not implemented, deliberately: `Fx` (Audacity has no persistent
//! per-track FX chain like REAPER's - effects are applied destructively/
//! non-destructively through the Effect menu, not an enumerable list) and
//! `set_color` (Audacity's `SetTrackVisuals: Colour=` is a small preset
//! enum, not arbitrary RGB - `r`/`g`/`b` u8 inputs don't map onto it
//! without guessing which preset is "closest", so this returns
//! `Unsupported` rather than silently picking one).

use async_trait::async_trait;
use dawmcp_core::{
    DawError, DawResult, Fx, FxInfo, Project, Render, RenderRequest, Status, TrackIndex, TrackInfo,
    Transport, Tracks, TransportState,
};
use tokio::sync::Mutex;

use crate::pipe_client::AudacityPipeClient;

pub struct AudacityBackend {
    client: Mutex<Option<AudacityPipeClient>>,
}

impl AudacityBackend {
    pub fn new() -> Self {
        Self { client: Mutex::new(None) }
    }

    async fn send(&self, command: &str) -> DawResult<String> {
        let mut guard = self.client.lock().await;
        if guard.is_none() {
            let connected = AudacityPipeClient::connect()
                .await
                .map_err(|e| DawError::NotReachable(e.to_string()))?;
            *guard = Some(connected);
        }
        let client = guard.as_mut().expect("just connected above");
        match client.command(command).await {
            Ok(reply) => Ok(reply),
            Err(e) => {
                // A broken pipe likely means Audacity closed/crashed - drop
                // the stale connection so the next call reconnects instead
                // of repeating the same error forever.
                *guard = None;
                Err(DawError::Other(format!("{command}: {e}")))
            }
        }
    }

    async fn select_single(&self, track: TrackIndex) -> DawResult<()> {
        self.send(&format!("SelectTracks: Track={track} TrackCount=1 Mode=Set")).await.map(|_| ())
    }

    async fn list_raw(&self) -> DawResult<Vec<serde_json::Value>> {
        let reply = self.send("GetInfo: Type=Tracks Format=JSON").await?;
        let parsed: serde_json::Value = serde_json::from_str(reply.trim())
            .map_err(|e| DawError::Other(format!("parsing GetInfo Tracks reply: {e} (reply: {reply:?})")))?;
        Ok(parsed.as_array().cloned().unwrap_or_default())
    }
}

impl Default for AudacityBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn unsupported(what: &str) -> DawError {
    DawError::Unsupported(format!(
        "{what} - not supported for Audacity (see dawmcp-audacity/src/backend.rs doc comment for why)"
    ))
}

#[async_trait]
impl Transport for AudacityBackend {
    async fn play(&self) -> DawResult<()> {
        self.send("Play").await.map(|_| ())
    }

    async fn stop(&self) -> DawResult<()> {
        self.send("Stop").await.map(|_| ())
    }

    async fn pause(&self) -> DawResult<()> {
        self.send("Pause").await.map(|_| ())
    }

    async fn record(&self) -> DawResult<()> {
        // "Record2ndChoice" (records onto a new track) - the other option,
        // "Record1stChoice" (records at end of selected track), isn't used
        // here since `Transport::record` has no track-selection concept to
        // match it to.
        self.send("Record2ndChoice").await.map(|_| ())
    }

    async fn seek(&self, _position_seconds: f64) -> DawResult<()> {
        Err(unsupported("transport_seek: no verified mod-script-pipe command moves the playhead directly"))
    }

    async fn set_tempo(&self, _bpm: f64) -> DawResult<()> {
        Err(unsupported("transport_set_tempo: Audacity has no single project tempo (that's a REAPER-specific concept)"))
    }

    async fn get_state(&self) -> DawResult<TransportState> {
        Err(unsupported("transport_get_state: no verified command reports playhead position or play/pause/record state"))
    }
}

#[async_trait]
impl Tracks for AudacityBackend {
    async fn list(&self) -> DawResult<Vec<TrackInfo>> {
        let raw = self.list_raw().await?;
        Ok(raw
            .into_iter()
            .enumerate()
            .map(|(i, t)| {
                let linear_volume = t.get("volume").and_then(|v| v.as_f64()).unwrap_or(1.0);
                let volume_db = if linear_volume > 0.0 { 20.0 * linear_volume.log10() } else { -f64::INFINITY };
                TrackInfo {
                    index: i as TrackIndex,
                    name: t.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    volume_db,
                    pan: t.get("pan").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    muted: t.get("mute").and_then(|v| v.as_bool()).unwrap_or(false),
                    soloed: t.get("solo").and_then(|v| v.as_bool()).unwrap_or(false),
                }
            })
            .collect())
    }

    async fn add(&self, name: Option<&str>) -> DawResult<TrackIndex> {
        // New tracks are appended at the end of the track list (standard
        // "Add New" menu behavior) - the count before adding is the new
        // track's index.
        let index = self.list_raw().await?.len() as TrackIndex;
        self.send("NewMonoTrack").await?;
        if let Some(name) = name {
            self.select_single(index).await?;
            self.send(&format!("SetTrackStatus: Name=\"{name}\"")).await?;
        }
        Ok(index)
    }

    async fn remove(&self, track: TrackIndex) -> DawResult<()> {
        self.select_single(track).await?;
        self.send("RemoveTracks").await.map(|_| ())
    }

    async fn rename(&self, track: TrackIndex, name: &str) -> DawResult<()> {
        self.select_single(track).await?;
        self.send(&format!("SetTrackStatus: Name=\"{name}\"")).await.map(|_| ())
    }

    async fn set_volume_db(&self, track: TrackIndex, db: f64) -> DawResult<()> {
        self.select_single(track).await?;
        self.send(&format!("SetTrackAudio: Volume={db}")).await.map(|_| ())
    }

    async fn set_pan(&self, track: TrackIndex, pan: f64) -> DawResult<()> {
        // DawBackend's pan is -1.0..1.0 (REAPER convention); Audacity's
        // SetTrackAudio Pan param is -100..100, mapped internally back to
        // -1.0..1.0 by `wt->SetPan(mPan / 100.0)` - so this is a straight
        // *100 scale, not a guess.
        self.select_single(track).await?;
        self.send(&format!("SetTrackAudio: Pan={}", pan * 100.0)).await.map(|_| ())
    }

    async fn set_mute(&self, track: TrackIndex, muted: bool) -> DawResult<()> {
        self.select_single(track).await?;
        self.send(&format!("SetTrackAudio: Mute={}", if muted { 1 } else { 0 })).await.map(|_| ())
    }

    async fn set_solo(&self, track: TrackIndex, soloed: bool) -> DawResult<()> {
        self.select_single(track).await?;
        self.send(&format!("SetTrackAudio: Solo={}", if soloed { 1 } else { 0 })).await.map(|_| ())
    }

    async fn set_color(&self, _track: TrackIndex, _r: u8, _g: u8, _b: u8) -> DawResult<()> {
        Err(unsupported("track_set_color"))
    }
}

#[async_trait]
impl Fx for AudacityBackend {
    async fn list(&self, _track: TrackIndex) -> DawResult<Vec<FxInfo>> {
        Err(unsupported("fx_list"))
    }

    async fn add(&self, _track: TrackIndex, _fx_name: &str) -> DawResult<i32> {
        Err(unsupported("fx_add"))
    }

    async fn remove(&self, _track: TrackIndex, _fx_index: i32) -> DawResult<()> {
        Err(unsupported("fx_remove"))
    }

    async fn set_enabled(&self, _track: TrackIndex, _fx_index: i32, _enabled: bool) -> DawResult<()> {
        Err(unsupported("fx_set_enabled"))
    }

    async fn get_param(&self, _track: TrackIndex, _fx_index: i32, _param_index: i32) -> DawResult<f64> {
        Err(unsupported("fx_get_param"))
    }

    async fn set_param(&self, _track: TrackIndex, _fx_index: i32, _param_index: i32, _value: f64) -> DawResult<()> {
        Err(unsupported("fx_set_param"))
    }
}

#[async_trait]
impl Render for AudacityBackend {
    async fn render(&self, request: RenderRequest) -> DawResult<String> {
        if !request.overwrite && std::path::Path::new(&request.output_path).exists() {
            return Err(DawError::Other(format!(
                "{} already exists and overwrite=false - not risking Audacity's own overwrite-confirmation \
                 dialog blocking the pipe (same class of bug as REAPER's render dialog)",
                request.output_path
            )));
        }

        match (request.start_seconds, request.end_seconds) {
            (Some(start), Some(end)) => {
                self.send(&format!("SelectTime: Start={start} End={end}")).await?;
            }
            _ => {
                self.send("SelectAll").await?;
            }
        }

        self.send(&format!("Export2: Filename=\"{}\" NumChannels=2", request.output_path)).await?;
        Ok(request.output_path)
    }
}

#[async_trait]
impl Project for AudacityBackend {
    async fn save(&self) -> DawResult<()> {
        // "Save" saves in place; if the project has never been saved
        // before, this may open a "Save As" dialog instead (unverified
        // whether Audacity has a scriptable "SaveAs with path" - matches
        // this crate's general policy of not guessing dialog behavior).
        self.send("Save").await.map(|_| ())
    }

    async fn undo(&self) -> DawResult<()> {
        self.send("Undo").await.map(|_| ())
    }
}

#[async_trait]
impl Status for AudacityBackend {
    async fn is_reachable(&self) -> bool {
        let mut guard = self.client.lock().await;
        if guard.is_some() {
            return true;
        }
        match AudacityPipeClient::connect().await {
            Ok(connected) => {
                *guard = Some(connected);
                true
            }
            Err(_) => false,
        }
    }

    async fn describe(&self) -> DawResult<String> {
        if self.is_reachable().await {
            Ok("Audacity mod-script-pipe connected".to_string())
        } else {
            Err(DawError::NotReachable(
                "could not open Audacity's mod-script-pipe (\\\\.\\pipe\\ToSrvPipe / FromSrvPipe) \
                 - is Audacity running with Edit > Preferences > Modules > mod-script-pipe enabled?"
                    .to_string(),
            ))
        }
    }
}
