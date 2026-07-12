//! `dawmcp-core` trait impl for Audacity, over `mod-script-pipe`.
//!
//! Scaffold status: only commands whose name AND full effect are verified
//! against Audacity's official scripting reference / source are wired up
//! (currently: Play/Stop/Pause/Record2ndChoice, zero parameters, no reply
//! parsing needed). Everything else returns `DawError::Unsupported` with a
//! comment on exactly what's unverified, rather than guessing at parameter
//! key spelling (e.g. `SetTrackAudio`'s exact `Mute=`/`Solo=`/`Gain=` keys,
//! `GetInfo: Type=Tracks` response format for parsing a track list,
//! `SelectTracks`'s exact index parameter name) or fabricating a return
//! value (e.g. the new track index after `NewMonoTrack`) that this crate
//! can't actually confirm is correct. Filling these in is next-steps work
//! in AGENTS.md, each needing its own doc/source verification pass - same
//! discipline as the REAPER adapter's `js_ReaScriptAPI`/`RENDER_FORMAT`
//! lessons.

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
        client.command(command).await.map_err(|e| DawError::Other(format!("{command}: {e}")))
    }
}

impl Default for AudacityBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn unsupported(what: &str) -> DawError {
    DawError::Unsupported(format!(
        "{what} - not yet implemented for Audacity, needs verification against Audacity's \
         scripting reference before it can be added (see AGENTS.md next steps)"
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
        // "Record2ndChoice" (records onto a new track) - confirmed to exist
        // in Audacity's scripting reference; "Record1stChoice" (records at
        // end of selected track) is the other option, not used here since
        // `Transport::record` has no track-selection concept to match it to.
        self.send("Record2ndChoice").await.map(|_| ())
    }

    async fn seek(&self, _position_seconds: f64) -> DawResult<()> {
        Err(unsupported("transport_seek"))
    }

    async fn set_tempo(&self, _bpm: f64) -> DawResult<()> {
        Err(unsupported("transport_set_tempo"))
    }

    async fn get_state(&self) -> DawResult<TransportState> {
        Err(unsupported("transport_get_state"))
    }
}

#[async_trait]
impl Tracks for AudacityBackend {
    async fn list(&self) -> DawResult<Vec<TrackInfo>> {
        Err(unsupported("track_list"))
    }

    async fn add(&self, _name: Option<&str>) -> DawResult<TrackIndex> {
        Err(unsupported("track_add"))
    }

    async fn remove(&self, _track: TrackIndex) -> DawResult<()> {
        Err(unsupported("track_remove"))
    }

    async fn rename(&self, _track: TrackIndex, _name: &str) -> DawResult<()> {
        Err(unsupported("track_rename"))
    }

    async fn set_volume_db(&self, _track: TrackIndex, _db: f64) -> DawResult<()> {
        Err(unsupported("track_set_volume_db"))
    }

    async fn set_pan(&self, _track: TrackIndex, _pan: f64) -> DawResult<()> {
        Err(unsupported("track_set_pan"))
    }

    async fn set_mute(&self, _track: TrackIndex, _muted: bool) -> DawResult<()> {
        Err(unsupported("track_set_mute"))
    }

    async fn set_solo(&self, _track: TrackIndex, _soloed: bool) -> DawResult<()> {
        Err(unsupported("track_set_solo"))
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
    async fn render(&self, _request: RenderRequest) -> DawResult<String> {
        Err(unsupported("render_project"))
    }
}

#[async_trait]
impl Project for AudacityBackend {
    async fn save(&self) -> DawResult<()> {
        Err(unsupported("project_save"))
    }

    async fn undo(&self) -> DawResult<()> {
        Err(unsupported("project_undo"))
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
