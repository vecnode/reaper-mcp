//! Audacity adapter (scaffold): implements `dawmcp_core::DawBackend` over
//! Audacity's `mod-script-pipe`. See `pipe_client.rs` for the verified wire
//! protocol and `backend.rs` for which commands are actually wired up vs.
//! left `Unsupported` pending further verification.

mod backend;
mod pipe_client;

pub use backend::AudacityBackend;
pub use pipe_client::AudacityPipeClient;
