//! Audacity install/process discovery, mirroring `dawmcp-reaper`'s
//! `discovery.rs` but simpler: Audacity needs no bridge file installed (no
//! Lua-equivalent script to copy) - `mod-script-pipe` is a built-in module
//! the user enables once in Edit > Preferences > Modules. This module only
//! reports what's found; it never toggles that preference itself (that's a
//! security-relevant setting - Audacity's own docs warn that any program
//! can then fully control it with no notification - so enabling it isn't
//! something to do silently on the user's behalf).

use std::path::PathBuf;

use serde::Serialize;
use sysinfo::System;

#[derive(Debug, Clone, Serialize)]
pub struct AudacityProcess {
    pub pid: u32,
    pub exe: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudacityInstall {
    pub path: String,
    /// How this install was found: "known_path" or "env".
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudacityDiscoveryReport {
    pub os: String,
    pub running_processes: Vec<AudacityProcess>,
    pub installs: Vec<AudacityInstall>,
    /// True if Audacity's mod-script-pipe pipes/FIFOs could be opened -
    /// i.e. Audacity is running AND the module is enabled, not just that
    /// Audacity is installed.
    pub pipe_reachable: bool,
}

fn is_audacity_process_name(name: &str) -> bool {
    name.to_lowercase() == "audacity.exe" || name.to_lowercase() == "audacity"
}

pub fn find_running_audacity() -> Vec<AudacityProcess> {
    let mut system = System::new_all();
    system.refresh_all();

    system
        .processes()
        .values()
        .filter_map(|proc| {
            let name = proc.name().to_string_lossy().to_string();
            if !is_audacity_process_name(&name) {
                return None;
            }
            Some(AudacityProcess {
                pid: proc.pid().as_u32(),
                exe: proc.exe().map(|p| p.to_string_lossy().to_string()),
                name,
            })
        })
        .collect()
}

fn candidate_install_paths() -> Vec<(PathBuf, &'static str)> {
    let mut candidates: Vec<(PathBuf, &'static str)> = Vec::new();

    if cfg!(target_os = "windows") {
        for env_var in ["PROGRAMFILES", "PROGRAMFILES(X86)"] {
            if let Ok(base) = std::env::var(env_var) {
                candidates.push((PathBuf::from(&base).join("Audacity").join("Audacity.exe"), "known_path"));
            }
        }
    } else if cfg!(target_os = "macos") {
        candidates.push((PathBuf::from("/Applications/Audacity.app"), "known_path"));
    } else {
        for path in ["/usr/bin/audacity", "/usr/local/bin/audacity", "/snap/bin/audacity"] {
            candidates.push((PathBuf::from(path), "known_path"));
        }
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            candidates.push((
                home.join(".var/app/org.audacityteam.Audacity/current/active/files/bin/audacity"),
                "known_path",
            ));
        }
    }

    if let Ok(env_override) = std::env::var("AUDACITY_PATH") {
        candidates.insert(0, (PathBuf::from(env_override), "env"));
    }

    candidates
}

pub fn find_audacity_installs() -> Vec<AudacityInstall> {
    let mut installs = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (path, source) in candidate_install_paths() {
        if !path.exists() {
            continue;
        }
        let key = path.to_string_lossy().to_string();
        if !seen.insert(key.clone()) {
            continue;
        }
        installs.push(AudacityInstall { path: key, source: source.to_string() });
    }

    installs
}

pub async fn run_discovery() -> AudacityDiscoveryReport {
    let pipe_reachable = crate::pipe_client::AudacityPipeClient::connect().await.is_ok();
    AudacityDiscoveryReport {
        os: std::env::consts::OS.to_string(),
        running_processes: find_running_audacity(),
        installs: find_audacity_installs(),
        pipe_reachable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_audacity_process_name_matches_common_forms() {
        assert!(is_audacity_process_name("audacity.exe"));
        assert!(is_audacity_process_name("Audacity"));
        assert!(!is_audacity_process_name("chrome.exe"));
    }
}
