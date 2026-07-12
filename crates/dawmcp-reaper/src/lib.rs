//! REAPER adapter: implements `dawmcp_core::DawBackend` over the existing
//! file-IPC bridge (`lua/reaper_bridge.lua`, unchanged by this port).

mod backend;
mod bridge_client;
mod discovery;
mod extra;
mod installer;

pub use backend::ReaperBackend;
pub use bridge_client::{default_bridge_dir, BridgeClient};
pub use discovery::{run_discovery, which_reaper, DiscoveryReport};
pub use extra::{ComposeNote, ComposeResult};
pub use installer::install_bridge;
