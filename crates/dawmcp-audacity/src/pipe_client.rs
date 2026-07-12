//! Client for Audacity's `mod-script-pipe` named-pipe protocol, verified
//! against the official reference client (`audacity/au3/scripts/piped-work/
//! pipeclient.py` in the Audacity source, not assumed from memory - this
//! project has been burned before by unverified API assumptions).
//!
//! Protocol, confirmed from that source:
//! - Windows: two named pipes, `\\.\pipe\ToSrvPipe` (write) and
//!   `\\.\pipe\FromSrvPipe` (read). Each command is terminated with
//!   `"\r\n\0"`.
//! - Linux/Mac: two FIFOs at `/tmp/audacity_script_pipe.to.<uid>` and
//!   `.from.<uid>`, terminated with `"\n"`. Audacity must be run with
//!   mod-script-pipe enabled (Edit > Preferences > Modules) for these to
//!   exist - this client does not create them.
//! - Plain text, not JSON: commands are strings like `"Play"` or
//!   `"NewMonoTrack"`, one per write. The reply is read line-by-line until
//!   a blank line (`"\n"` alone) is seen, which terminates the response -
//!   there is no explicit length prefix or JSON framing.
//!
//! Command *names and parameters* beyond what's used in `backend.rs` are
//! NOT verified here - see that file for which commands are implemented
//! vs. deliberately left `Unsupported` pending verification against
//! Audacity's scripting reference.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

#[cfg(windows)]
mod platform {
    pub const WRITE_PATH: &str = r"\\.\pipe\ToSrvPipe";
    pub const READ_PATH: &str = r"\\.\pipe\FromSrvPipe";
    pub const EOL: &str = "\r\n\0";
}

#[cfg(not(windows))]
mod platform {
    pub fn write_path() -> String {
        format!("/tmp/audacity_script_pipe.to.{}", unsafe { libc_getuid() })
    }
    pub fn read_path() -> String {
        format!("/tmp/audacity_script_pipe.from.{}", unsafe { libc_getuid() })
    }
    pub const EOL: &str = "\n";

    extern "C" {
        fn getuid() -> u32;
    }
    unsafe fn libc_getuid() -> u32 {
        getuid()
    }
}

pub struct AudacityPipeClient {
    #[cfg(windows)]
    write_pipe: tokio::net::windows::named_pipe::NamedPipeClient,
    #[cfg(not(windows))]
    write_pipe: tokio::fs::File,
    #[cfg(windows)]
    read_pipe: BufReader<tokio::net::windows::named_pipe::NamedPipeClient>,
    #[cfg(not(windows))]
    read_pipe: BufReader<tokio::fs::File>,
}

impl AudacityPipeClient {
    /// Connects to Audacity's already-running mod-script-pipe. Fails if
    /// Audacity isn't running with the module enabled, matching
    /// `pipeclient.py`'s behavior of erroring rather than waiting forever.
    #[cfg(windows)]
    pub async fn connect() -> Result<Self> {
        use tokio::net::windows::named_pipe::ClientOptions;

        let write_pipe = ClientOptions::new()
            .open(platform::WRITE_PATH)
            .with_context(|| format!("opening {} - is Audacity running with mod-script-pipe enabled?", platform::WRITE_PATH))?;
        let read_pipe = ClientOptions::new()
            .open(platform::READ_PATH)
            .with_context(|| format!("opening {}", platform::READ_PATH))?;
        Ok(Self { write_pipe, read_pipe: BufReader::new(read_pipe) })
    }

    #[cfg(not(windows))]
    pub async fn connect() -> Result<Self> {
        let write_path = platform::write_path();
        let read_path = platform::read_path();
        let write_pipe = tokio::fs::OpenOptions::new()
            .write(true)
            .open(&write_path)
            .await
            .with_context(|| format!("opening {write_path} - is Audacity running with mod-script-pipe enabled?"))?;
        let read_pipe = tokio::fs::OpenOptions::new()
            .read(true)
            .open(&read_path)
            .await
            .with_context(|| format!("opening {read_path}"))?;
        Ok(Self { write_pipe, read_pipe: BufReader::new(read_pipe) })
    }

    /// Sends a command and waits for its reply, terminated by a blank line
    /// (confirmed framing from `pipeclient.py`'s `_reader`).
    pub async fn command(&mut self, command: &str) -> Result<String> {
        self.command_with_timeout(command, DEFAULT_TIMEOUT).await
    }

    pub async fn command_with_timeout(&mut self, command: &str, timeout: Duration) -> Result<String> {
        let payload = format!("{command}{}", platform::EOL);
        self.write_pipe.write_all(payload.as_bytes()).await.context("writing to Audacity pipe")?;
        self.write_pipe.flush().await.context("flushing Audacity pipe")?;

        tokio::time::timeout(timeout, self.read_reply())
            .await
            .with_context(|| format!("timed out waiting for Audacity's reply to '{command}'"))?
    }

    async fn read_reply(&mut self) -> Result<String> {
        let mut message = String::new();
        loop {
            let mut line = String::new();
            let n = self.read_pipe.read_line(&mut line).await.context("reading Audacity pipe")?;
            if n == 0 {
                bail!("Audacity pipe closed (it may have crashed)");
            }
            if line == "\n" {
                break;
            }
            message.push_str(&line);
        }
        Ok(message)
    }
}
