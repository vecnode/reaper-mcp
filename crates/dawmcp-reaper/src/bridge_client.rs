//! File-based IPC client for `lua/reaper_bridge.lua`, ported from the
//! Python `bridge_client.py` it replaces. Protocol is unchanged: one JSON
//! request file per call, poll for the matching response file, clean both up.
//! See `lua/reaper_bridge.lua` for the Lua side and `docs/ARCHITECTURE.md`
//! for why this is file IPC and not a socket.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use tokio::sync::Mutex;

const HEARTBEAT_STALE_AFTER: Duration = Duration::from_millis(2000);
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub struct BridgeClient {
    bridge_dir: PathBuf,
    client_id: String,
    counter: Mutex<u64>,
    request_timeout: Duration,
}

impl BridgeClient {
    pub fn new(bridge_dir: PathBuf) -> Self {
        Self {
            bridge_dir,
            client_id: uuid::Uuid::new_v4().simple().to_string()[..8].to_string(),
            counter: Mutex::new(0),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }

    fn requests_dir(&self) -> PathBuf {
        self.bridge_dir.join("requests")
    }

    fn responses_dir(&self) -> PathBuf {
        self.bridge_dir.join("responses")
    }

    fn heartbeat_file(&self) -> PathBuf {
        self.bridge_dir.join("heartbeat.txt")
    }

    #[cfg(test)]
    pub(crate) fn client_id(&self) -> &str {
        &self.client_id
    }

    /// True if `reaper_bridge.lua` has touched its heartbeat file recently.
    pub fn is_alive(&self) -> bool {
        let Ok(meta) = std::fs::metadata(self.heartbeat_file()) else {
            return false;
        };
        let Ok(modified) = meta.modified() else {
            return false;
        };
        SystemTime::now()
            .duration_since(modified)
            .map(|age| age < HEARTBEAT_STALE_AFTER)
            .unwrap_or(false)
    }

    pub async fn call(&self, op: &str, args: Value) -> Result<Value> {
        self.call_with_timeout(op, args, self.request_timeout).await
    }

    pub async fn call_with_timeout(&self, op: &str, args: Value, timeout: Duration) -> Result<Value> {
        if !self.is_alive() {
            bail!(
                "REAPER bridge heartbeat not found or stale at {}. Is REAPER running with \
                 reaper_bridge.lua loaded (Actions -> Show action list -> run reaper_bridge.lua)?",
                self.heartbeat_file().display()
            );
        }

        std::fs::create_dir_all(self.requests_dir()).context("creating bridge requests dir")?;

        let req_id = {
            let mut counter = self.counter.lock().await;
            *counter += 1;
            format!("{}-{}", self.client_id, *counter)
        };

        let payload = serde_json::json!({ "id": req_id, "op": op, "args": args }).to_string();
        write_atomic(&self.requests_dir().join(format!("req_{req_id}.json")), &payload)
            .context("writing bridge request file")?;

        let response_path = self.responses_dir().join(format!("resp_{req_id}.json"));
        let deadline = Instant::now() + timeout;
        loop {
            if response_path.exists() {
                let content = match std::fs::read_to_string(&response_path) {
                    Ok(c) => c,
                    Err(_) => {
                        tokio::time::sleep(POLL_INTERVAL).await;
                        continue;
                    }
                };
                let _ = std::fs::remove_file(&response_path);
                let msg: Value = serde_json::from_str(&content)
                    .with_context(|| format!("malformed response from bridge: {content:?}"))?;
                let ok = msg.get("ok").and_then(Value::as_bool).unwrap_or(false);
                if !ok {
                    let err = msg.get("error").cloned().unwrap_or(Value::Null);
                    return Err(anyhow!("REAPER bridge error for op '{op}': {err}"));
                }
                return Ok(msg.get("result").cloned().unwrap_or(Value::Object(Default::default())));
            }
            if Instant::now() >= deadline {
                bail!("timed out waiting for bridge response to op '{op}' (id={req_id})");
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
}

fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Resolves the bridge IPC directory, matching `default_bridge_dir()` in the
/// Python `bridge_client.py`: `REAPER_MCP_BRIDGE_DIR` env override first,
/// else the first discovered REAPER install's `Scripts/reaper_mcp_bridge`,
/// else a last-resort fallback so callers still get a stable path to report
/// in errors.
pub fn default_bridge_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("REAPER_MCP_BRIDGE_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(install) = crate::discovery::find_reaper_installs().into_iter().next() {
        return PathBuf::from(install.scripts_dir).join("reaper_mcp_bridge");
    }
    dirs_fallback().join(".reaper_mcp_bridge")
}

fn dirs_fallback() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    //! Ported from `tests/test_bridge_client.py`: a background task stands
    //! in for `reaper_bridge.lua`'s defer-loop pump, answering known ops and
    //! keeping the heartbeat fresh, so these exercise the real file-IPC
    //! protocol without a running REAPER.

    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    struct MockBridgeWorker {
        stop: Arc<AtomicBool>,
        handle: Option<tokio::task::JoinHandle<()>>,
    }

    impl MockBridgeWorker {
        fn start(bridge_dir: PathBuf) -> Self {
            let requests_dir = bridge_dir.join("requests");
            let responses_dir = bridge_dir.join("responses");
            let heartbeat_file = bridge_dir.join("heartbeat.txt");
            std::fs::create_dir_all(&requests_dir).unwrap();
            std::fs::create_dir_all(&responses_dir).unwrap();

            let stop = Arc::new(AtomicBool::new(false));
            let stop_clone = stop.clone();
            let handle = tokio::spawn(async move {
                while !stop_clone.load(Ordering::Relaxed) {
                    let _ = std::fs::write(&heartbeat_file, "tick");
                    if let Ok(entries) = std::fs::read_dir(&requests_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
                            if !name.starts_with("req_") {
                                continue;
                            }
                            let Ok(content) = std::fs::read_to_string(&path) else { continue };
                            let _ = std::fs::remove_file(&path);
                            let Ok(req): Result<Value, _> = serde_json::from_str(&content) else { continue };
                            let id = req.get("id").and_then(|v| v.as_str()).unwrap_or_default();
                            let op = req.get("op").and_then(|v| v.as_str()).unwrap_or_default();
                            let resp = if op == "ping" {
                                serde_json::json!({ "id": id, "ok": true, "result": { "pong": true } })
                            } else {
                                serde_json::json!({ "id": id, "ok": false, "error": format!("unknown op {op}") })
                            };
                            let resp_path = responses_dir.join(format!("resp_{id}.json"));
                            let _ = std::fs::write(&resp_path, resp.to_string());
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            });
            Self { stop, handle: Some(handle) }
        }
    }

    impl Drop for MockBridgeWorker {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                handle.abort();
            }
        }
    }

    #[tokio::test]
    async fn call_success() {
        let dir = tempfile::tempdir().unwrap();
        let _worker = MockBridgeWorker::start(dir.path().to_path_buf());
        tokio::time::sleep(Duration::from_millis(20)).await;

        let client = BridgeClient::new(dir.path().to_path_buf());
        let result = client.call("ping", serde_json::json!({})).await.unwrap();
        assert_eq!(result, serde_json::json!({ "pong": true }));
    }

    #[tokio::test]
    async fn call_unknown_op_raises() {
        let dir = tempfile::tempdir().unwrap();
        let _worker = MockBridgeWorker::start(dir.path().to_path_buf());
        tokio::time::sleep(Duration::from_millis(20)).await;

        let client = BridgeClient::new(dir.path().to_path_buf());
        let err = client.call("nonexistent_op", serde_json::json!({})).await.unwrap_err();
        assert!(err.to_string().contains("unknown op"));
    }

    #[tokio::test]
    async fn no_heartbeat_raises() {
        let dir = tempfile::tempdir().unwrap();
        let client = BridgeClient::new(dir.path().join("no_bridge_here"));
        let err = client.call("ping", serde_json::json!({})).await.unwrap_err();
        assert!(err.to_string().contains("heartbeat"));
    }

    #[tokio::test]
    async fn is_alive_reflects_heartbeat_freshness() {
        let dir = tempfile::tempdir().unwrap();
        let worker = MockBridgeWorker::start(dir.path().to_path_buf());
        tokio::time::sleep(Duration::from_millis(20)).await;

        let client = BridgeClient::new(dir.path().to_path_buf());
        assert!(client.is_alive());

        drop(worker);
        let dead_client = BridgeClient::new(dir.path().join("dead"));
        assert!(!dead_client.is_alive());
    }

    #[tokio::test]
    async fn concurrent_clients_do_not_collide_on_request_ids() {
        let dir = tempfile::tempdir().unwrap();
        let _worker = MockBridgeWorker::start(dir.path().to_path_buf());
        tokio::time::sleep(Duration::from_millis(20)).await;

        let client_a = BridgeClient::new(dir.path().to_path_buf());
        let client_b = BridgeClient::new(dir.path().to_path_buf());
        assert_ne!(client_a.client_id(), client_b.client_id());

        let result_a = client_a.call("ping", serde_json::json!({})).await.unwrap();
        let result_b = client_b.call("ping", serde_json::json!({})).await.unwrap();
        assert_eq!(result_a, serde_json::json!({ "pong": true }));
        assert_eq!(result_b, serde_json::json!({ "pong": true }));
    }
}
