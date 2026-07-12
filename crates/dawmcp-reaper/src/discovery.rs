//! REAPER install/process discovery, ported from `discovery.py`: scans known
//! install locations per-OS, inspects the running process list for REAPER
//! executables, and checks the bridge heartbeat - so callers get actionable
//! diagnostics instead of a bare "not reachable".

use std::path::PathBuf;

use serde::Serialize;
use sysinfo::System;

use crate::bridge_client::BridgeClient;

#[derive(Debug, Clone, Serialize)]
pub struct ReaperProcess {
    pub pid: u32,
    pub exe: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReaperInstall {
    pub resource_path: String,
    pub scripts_dir: String,
    /// How this install was found: "known_path" or "env".
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveryReport {
    pub os: String,
    pub running_processes: Vec<ReaperProcess>,
    pub installs: Vec<ReaperInstall>,
    pub bridge_reachable: bool,
    pub bridge_dir: String,
}

fn is_reaper_process_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower == "reaper.exe" || lower == "reaper"
}

pub fn find_running_reaper() -> Vec<ReaperProcess> {
    let mut system = System::new_all();
    system.refresh_all();

    system
        .processes()
        .values()
        .filter_map(|proc| {
            let name = proc.name().to_string_lossy().to_string();
            if !is_reaper_process_name(&name) {
                return None;
            }
            Some(ReaperProcess {
                pid: proc.pid().as_u32(),
                exe: proc.exe().map(|p| p.to_string_lossy().to_string()),
                name,
            })
        })
        .collect()
}

fn candidate_resource_paths() -> Vec<(PathBuf, &'static str)> {
    let mut candidates: Vec<(PathBuf, &'static str)> = Vec::new();

    if cfg!(target_os = "windows") {
        if let Ok(appdata) = std::env::var("APPDATA") {
            candidates.push((PathBuf::from(appdata).join("REAPER"), "known_path"));
        }
        for env_var in ["PROGRAMFILES", "PROGRAMFILES(X86)"] {
            if let Ok(base) = std::env::var(env_var) {
                candidates.push((PathBuf::from(&base).join("REAPER (x64)"), "known_path"));
                candidates.push((PathBuf::from(&base).join("REAPER"), "known_path"));
            }
        }
    } else if cfg!(target_os = "macos") {
        if let Some(home) = dirs_home() {
            candidates.push((
                home.join("Library").join("Application Support").join("REAPER"),
                "known_path",
            ));
        }
        candidates.push((PathBuf::from("/Applications/REAPER64.app"), "known_path"));
    } else {
        if let Some(home) = dirs_home() {
            candidates.push((home.join(".config").join("REAPER"), "known_path"));
        }
    }

    if let Ok(env_override) = std::env::var("REAPER_RESOURCE_PATH") {
        candidates.insert(0, (PathBuf::from(env_override), "env"));
    }

    candidates
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

pub fn find_reaper_installs() -> Vec<ReaperInstall> {
    let mut installs = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (path, source) in candidate_resource_paths() {
        if !path.exists() {
            continue;
        }
        let key = path.to_string_lossy().to_string();
        if !seen.insert(key) {
            continue;
        }
        let scripts_dir = path.join("Scripts");
        installs.push(ReaperInstall {
            resource_path: path.to_string_lossy().to_string(),
            scripts_dir: scripts_dir.to_string_lossy().to_string(),
            source: source.to_string(),
        });
    }

    installs
}

pub fn run_discovery() -> DiscoveryReport {
    let bridge = BridgeClient::new(crate::bridge_client::default_bridge_dir());
    DiscoveryReport {
        os: std::env::consts::OS.to_string(),
        running_processes: find_running_reaper(),
        installs: find_reaper_installs(),
        bridge_reachable: bridge.is_alive(),
        bridge_dir: crate::bridge_client::default_bridge_dir().to_string_lossy().to_string(),
    }
}

pub fn which_reaper() -> Option<PathBuf> {
    which_in_path("reaper").or_else(|| which_in_path("reaper.exe"))
}

fn which_in_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var).find_map(|dir| {
        let candidate = dir.join(name);
        if candidate.is_file() {
            Some(candidate)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    //! Ported from `tests/test_discovery.py`. Env-var mutation isn't
    //! thread-safe across parallel test runs, so these are `#[serial]`.

    use super::*;
    use serial_test::serial;

    #[test]
    fn is_reaper_process_name_matches_common_forms() {
        assert!(is_reaper_process_name("reaper.exe"));
        assert!(is_reaper_process_name("REAPER"));
        assert!(!is_reaper_process_name("chrome.exe"));
    }

    #[test]
    #[serial]
    fn find_reaper_installs_respects_env_override() {
        let dir = tempfile::tempdir().unwrap();
        let fake_resource_dir = dir.path().join("REAPER");
        std::fs::create_dir_all(&fake_resource_dir).unwrap();

        std::env::set_var("REAPER_RESOURCE_PATH", &fake_resource_dir);
        let installs = find_reaper_installs();
        std::env::remove_var("REAPER_RESOURCE_PATH");

        assert!(installs.iter().any(|i| i.resource_path == fake_resource_dir.to_string_lossy()));
    }

    #[test]
    #[serial]
    fn run_discovery_bridge_reachable_when_heartbeat_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let bridge_dir = dir.path().join("bridge");
        std::fs::create_dir_all(&bridge_dir).unwrap();
        std::fs::write(bridge_dir.join("heartbeat.txt"), "123.456").unwrap();

        std::env::set_var("REAPER_MCP_BRIDGE_DIR", &bridge_dir);
        let report = run_discovery();
        std::env::remove_var("REAPER_MCP_BRIDGE_DIR");

        assert!(report.bridge_reachable);
        assert_eq!(report.bridge_dir, bridge_dir.to_string_lossy());
    }

    #[test]
    #[serial]
    fn run_discovery_bridge_unreachable_without_heartbeat() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("REAPER_MCP_BRIDGE_DIR", dir.path().join("no_such_bridge_dir"));
        let report = run_discovery();
        std::env::remove_var("REAPER_MCP_BRIDGE_DIR");

        assert!(!report.bridge_reachable);
    }
}
