use serde::{Deserialize, Serialize};

/// `-1` conventionally means the master track, mirroring the REAPER adapter's
/// existing convention so tool callers don't need to special-case it per DAW.
pub type TrackIndex = i32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    pub index: TrackIndex,
    pub name: String,
    pub volume_db: f64,
    pub pan: f64,
    pub muted: bool,
    pub soloed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportState {
    pub playing: bool,
    pub recording: bool,
    pub paused: bool,
    pub position_seconds: f64,
    pub tempo_bpm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FxInfo {
    pub index: i32,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderRequest {
    pub output_path: String,
    pub start_seconds: Option<f64>,
    pub end_seconds: Option<f64>,
    pub overwrite: bool,
}
