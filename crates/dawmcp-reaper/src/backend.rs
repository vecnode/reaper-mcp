//! `dawmcp-core` trait impls backed by the REAPER file-IPC bridge. Each
//! method maps 1:1 to an `ops.<name>` handler in `lua/reaper_bridge.lua` -
//! see that file for the REAPER-side implementation and field names.

use async_trait::async_trait;
use dawmcp_core::{
    DawError, DawResult, Fx, FxInfo, Project, RenderRequest, Render, Status, TrackIndex, TrackInfo,
    Transport, Tracks, TransportState,
};
use serde_json::json;

use crate::bridge_client::BridgeClient;

pub struct ReaperBackend {
    bridge: BridgeClient,
}

impl ReaperBackend {
    pub fn new(bridge: BridgeClient) -> Self {
        Self { bridge }
    }

    pub(crate) fn bridge(&self) -> &BridgeClient {
        &self.bridge
    }

    pub(crate) fn map_err(op: &str, err: anyhow::Error) -> DawError {
        DawError::Other(format!("{op}: {err}"))
    }
}

#[async_trait]
impl Transport for ReaperBackend {
    async fn play(&self) -> DawResult<()> {
        self.bridge.call("transport_play", json!({})).await.map(|_| ()).map_err(|e| Self::map_err("transport_play", e))
    }

    async fn stop(&self) -> DawResult<()> {
        self.bridge.call("transport_stop", json!({})).await.map(|_| ()).map_err(|e| Self::map_err("transport_stop", e))
    }

    async fn pause(&self) -> DawResult<()> {
        self.bridge.call("transport_pause", json!({})).await.map(|_| ()).map_err(|e| Self::map_err("transport_pause", e))
    }

    async fn record(&self) -> DawResult<()> {
        self.bridge.call("transport_record", json!({})).await.map(|_| ()).map_err(|e| Self::map_err("transport_record", e))
    }

    async fn seek(&self, position_seconds: f64) -> DawResult<()> {
        self.bridge
            .call("transport_seek", json!({ "position_sec": position_seconds }))
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("transport_seek", e))
    }

    async fn set_tempo(&self, bpm: f64) -> DawResult<()> {
        self.bridge
            .call("transport_set_tempo", json!({ "bpm": bpm }))
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("transport_set_tempo", e))
    }

    async fn get_state(&self) -> DawResult<TransportState> {
        let result = self
            .bridge
            .call("transport_get_state", json!({}))
            .await
            .map_err(|e| Self::map_err("transport_get_state", e))?;
        // REAPER's play_state is a bitfield: 1=playing, 2=paused, 4=recording.
        let play_state = result.get("play_state").and_then(|v| v.as_i64()).unwrap_or(0);
        Ok(TransportState {
            playing: play_state & 1 != 0,
            paused: play_state & 2 != 0,
            recording: play_state & 4 != 0,
            position_seconds: result.get("position_sec").and_then(|v| v.as_f64()).unwrap_or(0.0),
            tempo_bpm: result.get("tempo").and_then(|v| v.as_f64()).unwrap_or(0.0),
        })
    }
}

#[async_trait]
impl Tracks for ReaperBackend {
    async fn list(&self) -> DawResult<Vec<TrackInfo>> {
        let result = self
            .bridge
            .call("track_list", json!({}))
            .await
            .map_err(|e| Self::map_err("track_list", e))?;
        let tracks = result.get("tracks").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        Ok(tracks
            .into_iter()
            .map(|t| TrackInfo {
                index: t.get("index").and_then(|v| v.as_i64()).unwrap_or(0) as TrackIndex,
                name: t.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                volume_db: t.get("volume_db").and_then(|v| v.as_f64()).unwrap_or(0.0),
                pan: t.get("pan").and_then(|v| v.as_f64()).unwrap_or(0.0),
                muted: t.get("mute").and_then(|v| v.as_bool()).unwrap_or(false),
                soloed: t.get("solo").and_then(|v| v.as_bool()).unwrap_or(false),
            })
            .collect())
    }

    async fn add(&self, name: Option<&str>) -> DawResult<TrackIndex> {
        let result = self
            .bridge
            .call("track_add", json!({ "name": name }))
            .await
            .map_err(|e| Self::map_err("track_add", e))?;
        Ok(result.get("index").and_then(|v| v.as_i64()).unwrap_or(0) as TrackIndex)
    }

    async fn remove(&self, track: TrackIndex) -> DawResult<()> {
        self.bridge
            .call("track_remove", json!({ "track_index": track }))
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("track_remove", e))
    }

    async fn rename(&self, track: TrackIndex, name: &str) -> DawResult<()> {
        self.bridge
            .call("track_rename", json!({ "track_index": track, "name": name }))
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("track_rename", e))
    }

    async fn set_volume_db(&self, track: TrackIndex, db: f64) -> DawResult<()> {
        self.bridge
            .call("track_set_volume_db", json!({ "track_index": track, "db": db }))
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("track_set_volume_db", e))
    }

    async fn set_pan(&self, track: TrackIndex, pan: f64) -> DawResult<()> {
        self.bridge
            .call("track_set_pan", json!({ "track_index": track, "pan": pan }))
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("track_set_pan", e))
    }

    async fn set_mute(&self, track: TrackIndex, muted: bool) -> DawResult<()> {
        self.bridge
            .call("track_set_mute", json!({ "track_index": track, "mute": muted }))
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("track_set_mute", e))
    }

    async fn set_solo(&self, track: TrackIndex, soloed: bool) -> DawResult<()> {
        self.bridge
            .call("track_set_solo", json!({ "track_index": track, "solo": soloed }))
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("track_set_solo", e))
    }

    async fn set_color(&self, track: TrackIndex, r: u8, g: u8, b: u8) -> DawResult<()> {
        self.bridge
            .call("track_set_color", json!({ "track_index": track, "r": r, "g": g, "b": b }))
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("track_set_color", e))
    }
}

#[async_trait]
impl Fx for ReaperBackend {
    async fn list(&self, track: TrackIndex) -> DawResult<Vec<FxInfo>> {
        let result = self
            .bridge
            .call("fx_list", json!({ "track_index": track }))
            .await
            .map_err(|e| Self::map_err("fx_list", e))?;
        let fx = result.get("fx").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        Ok(fx
            .into_iter()
            .map(|f| FxInfo {
                index: f.get("index").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                name: f.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                enabled: f.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
            })
            .collect())
    }

    async fn add(&self, track: TrackIndex, fx_name: &str) -> DawResult<i32> {
        let result = self
            .bridge
            .call("fx_add", json!({ "track_index": track, "fx_name": fx_name }))
            .await
            .map_err(|e| Self::map_err("fx_add", e))?;
        Ok(result.get("fx_index").and_then(|v| v.as_i64()).unwrap_or(0) as i32)
    }

    async fn remove(&self, track: TrackIndex, fx_index: i32) -> DawResult<()> {
        self.bridge
            .call("fx_remove", json!({ "track_index": track, "fx_index": fx_index }))
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("fx_remove", e))
    }

    async fn set_enabled(&self, track: TrackIndex, fx_index: i32, enabled: bool) -> DawResult<()> {
        self.bridge
            .call(
                "fx_set_enabled",
                json!({ "track_index": track, "fx_index": fx_index, "enabled": enabled }),
            )
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("fx_set_enabled", e))
    }

    async fn get_param(&self, track: TrackIndex, fx_index: i32, param_index: i32) -> DawResult<f64> {
        let result = self
            .bridge
            .call(
                "fx_get_param",
                json!({ "track_index": track, "fx_index": fx_index, "param_index": param_index }),
            )
            .await
            .map_err(|e| Self::map_err("fx_get_param", e))?;
        Ok(result.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0))
    }

    async fn set_param(&self, track: TrackIndex, fx_index: i32, param_index: i32, value: f64) -> DawResult<()> {
        self.bridge
            .call(
                "fx_set_param",
                json!({ "track_index": track, "fx_index": fx_index, "param_index": param_index, "value": value }),
            )
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("fx_set_param", e))
    }
}

#[async_trait]
impl Render for ReaperBackend {
    async fn render(&self, request: RenderRequest) -> DawResult<String> {
        let result = self
            .bridge
            .call(
                "render_project",
                json!({
                    "output_path": request.output_path,
                    "start_sec": request.start_seconds,
                    "end_sec": request.end_seconds,
                    "overwrite": request.overwrite,
                }),
            )
            .await
            .map_err(|e| Self::map_err("render_project", e))?;
        Ok(result
            .get("output_path")
            .and_then(|v| v.as_str())
            .unwrap_or(&request.output_path)
            .to_string())
    }
}

#[async_trait]
impl Project for ReaperBackend {
    async fn save(&self) -> DawResult<()> {
        self.bridge.call("project_save", json!({})).await.map(|_| ()).map_err(|e| Self::map_err("project_save", e))
    }

    async fn undo(&self) -> DawResult<()> {
        self.bridge.call("project_undo", json!({})).await.map(|_| ()).map_err(|e| Self::map_err("project_undo", e))
    }
}

#[async_trait]
impl Status for ReaperBackend {
    async fn is_reachable(&self) -> bool {
        self.bridge.is_alive()
    }

    async fn describe(&self) -> DawResult<String> {
        if !self.bridge.is_alive() {
            return Err(DawError::NotReachable(
                "REAPER bridge heartbeat missing or stale".to_string(),
            ));
        }
        let result = self
            .bridge
            .call("get_reaper_info", json!({}))
            .await
            .map_err(|e| Self::map_err("get_reaper_info", e))?;
        Ok(result.to_string())
    }
}
