//! DAW-agnostic traits and types shared by every `dawmcp` adapter.
//!
//! An adapter crate (`dawmcp-reaper`, `dawmcp-audacity`, ...) implements
//! [`DawBackend`] for its DAW; `dawmcp-server` exposes the same MCP tool
//! surface over whichever backend is active, and adds DAW-specific escape
//! hatches (`run_reascript`, ...) only where the underlying DAW actually
//! supports them.

mod error;
mod traits;
mod types;

pub use error::{DawError, DawResult};
pub use traits::{DawBackend, Fx, Project, Render, Status, Transport, Tracks};
pub use types::{FxInfo, RenderRequest, TrackIndex, TrackInfo, TransportState};
