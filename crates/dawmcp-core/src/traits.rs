use async_trait::async_trait;

use crate::error::DawResult;
use crate::types::{FxInfo, RenderRequest, TrackIndex, TrackInfo, TransportState};

/// Play/stop/record/seek/tempo - concepts every DAW shares.
#[async_trait]
pub trait Transport {
    async fn play(&self) -> DawResult<()>;
    async fn stop(&self) -> DawResult<()>;
    async fn pause(&self) -> DawResult<()>;
    async fn record(&self) -> DawResult<()>;
    async fn seek(&self, position_seconds: f64) -> DawResult<()>;
    async fn set_tempo(&self, bpm: f64) -> DawResult<()>;
    async fn get_state(&self) -> DawResult<TransportState>;
}

/// Track add/remove/rename and per-track properties.
#[async_trait]
pub trait Tracks {
    async fn list(&self) -> DawResult<Vec<TrackInfo>>;
    async fn add(&self, name: Option<&str>) -> DawResult<TrackIndex>;
    async fn remove(&self, track: TrackIndex) -> DawResult<()>;
    async fn rename(&self, track: TrackIndex, name: &str) -> DawResult<()>;
    async fn set_volume_db(&self, track: TrackIndex, db: f64) -> DawResult<()>;
    async fn set_pan(&self, track: TrackIndex, pan: f64) -> DawResult<()>;
    async fn set_mute(&self, track: TrackIndex, muted: bool) -> DawResult<()>;
    async fn set_solo(&self, track: TrackIndex, soloed: bool) -> DawResult<()>;
    async fn set_color(&self, track: TrackIndex, r: u8, g: u8, b: u8) -> DawResult<()>;
}

/// FX/plugin chains on a track. Not every DAW has a chain concept as rich as
/// REAPER's (Audacity's built-in effects are closer to a fixed list) - the
/// adapter returns `DawError::Unsupported` for anything it can't back.
#[async_trait]
pub trait Fx {
    async fn list(&self, track: TrackIndex) -> DawResult<Vec<FxInfo>>;
    async fn add(&self, track: TrackIndex, fx_name: &str) -> DawResult<i32>;
    async fn remove(&self, track: TrackIndex, fx_index: i32) -> DawResult<()>;
    async fn set_enabled(&self, track: TrackIndex, fx_index: i32, enabled: bool) -> DawResult<()>;
    async fn get_param(&self, track: TrackIndex, fx_index: i32, param_index: i32) -> DawResult<f64>;
    async fn set_param(
        &self,
        track: TrackIndex,
        fx_index: i32,
        param_index: i32,
        value: f64,
    ) -> DawResult<()>;
}

/// Render/export to an audio file.
#[async_trait]
pub trait Render {
    async fn render(&self, request: RenderRequest) -> DawResult<String>;
}

/// Project-level persistence.
#[async_trait]
pub trait Project {
    async fn save(&self) -> DawResult<()>;
    async fn undo(&self) -> DawResult<()>;
}

/// Whether the DAW and its bridge/adapter are currently reachable.
#[async_trait]
pub trait Status {
    async fn is_reachable(&self) -> bool;
    async fn describe(&self) -> DawResult<String>;
}

/// A full DAW backend: every adapter (REAPER, Audacity, ...) implements this
/// once, and `dawmcp-server` exposes the same MCP tool surface over whichever
/// backend is active. DAW-specific concepts that don't fit these shared
/// traits (REAPER's `run_reascript`, Audacity label tracks, etc.) live as
/// extra inherent methods on the adapter, not on this trait.
pub trait DawBackend: Transport + Tracks + Fx + Render + Project + Status + Send + Sync {}

impl<T> DawBackend for T where T: Transport + Tracks + Fx + Render + Project + Status + Send + Sync {}
