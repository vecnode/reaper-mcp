//! Installs `reaper_bridge.lua` and the default project into the detected
//! REAPER install(s), and wires both into REAPER's native `__startup.lua` so
//! they auto-run/auto-open on launch. Ported from `installer.py`; behavior
//! (idempotent copy-if-different, marker-delimited startup block that
//! leaves the rest of the user's `__startup.lua` untouched) is unchanged.

use std::path::{Path, PathBuf};

use crate::discovery::{find_reaper_installs, ReaperInstall};

const BRIDGE_FILENAME: &str = "reaper_bridge.lua";
const DEFAULT_PROJECT_SOURCE_FILENAME: &str = "default.RPP";
const DEFAULT_PROJECT_INSTALLED_FILENAME: &str = "reaper-mcp-default.RPP";
const STARTUP_FILENAME: &str = "__startup.lua";
const STARTUP_START_MARKER: &str = "-- reaper-mcp:start";
const STARTUP_END_MARKER: &str = "-- reaper-mcp:end";

/// Repo root, resolved at compile time from this crate's manifest location
/// (`crates/dawmcp-reaper/Cargo.toml` is always two levels under repo root
/// in this workspace) - stable regardless of where the built binary is run
/// from, unlike resolving relative to `current_exe()`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("dawmcp-reaper is two levels under repo root")
        .to_path_buf()
}

fn bridge_source_path() -> anyhow::Result<PathBuf> {
    let path = repo_root().join("lua").join(BRIDGE_FILENAME);
    if !path.exists() {
        anyhow::bail!("bundled bridge script not found at {}", path.display());
    }
    Ok(path)
}

fn default_project_source_path() -> anyhow::Result<PathBuf> {
    let path = repo_root().join("reaper_project").join(DEFAULT_PROJECT_SOURCE_FILENAME);
    if !path.exists() {
        anyhow::bail!("bundled default project not found at {}", path.display());
    }
    Ok(path)
}

fn files_identical(a: &Path, b: &Path) -> bool {
    match (std::fs::read(a), std::fs::read(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Copy `reaper_bridge.lua` and the default project into one or all
/// detected REAPER installs, and wire both into `__startup.lua`. Returns
/// human-readable status lines for each install acted on.
pub fn install_bridge() -> Vec<String> {
    let src = match bridge_source_path() {
        Ok(p) => p,
        Err(e) => return vec![e.to_string()],
    };
    let targets = find_reaper_installs();
    if targets.is_empty() {
        return vec![
            "No REAPER installation found. Set REAPER_RESOURCE_PATH or install REAPER first."
                .to_string(),
        ];
    }

    let mut results = Vec::new();
    for install in &targets {
        let scripts_dir = PathBuf::from(&install.scripts_dir);
        let dest = scripts_dir.join(BRIDGE_FILENAME);
        match std::fs::create_dir_all(&scripts_dir) {
            Ok(()) => {
                if dest.exists() && files_identical(&src, &dest) {
                    results.push(format!("up to date: {}", dest.display()));
                } else if let Err(e) = std::fs::copy(&src, &dest) {
                    results.push(format!("skipped ({e}): {}", dest.display()));
                } else {
                    results.push(format!("installed: {}", dest.display()));
                }
            }
            Err(e) => results.push(format!("skipped ({e}): {}", dest.display())),
        }
    }

    results.extend(install_default_project(&targets));
    results.extend(install_startup_hook(&targets));

    results.push(
        "The bridge is wired into REAPER's __startup.lua and will auto-run the next time \
         REAPER launches (fully quit and reopen REAPER if it's already running for this to \
         take effect). No manual Actions-list step or extensions required."
            .to_string(),
    );
    results
}

/// Copy the bundled blank default project into REAPER's resource path under
/// a dedicated filename so it never collides with the user's own projects.
/// A missing default project is a soft skip, not a hard failure - it's a
/// convenience feature, unlike the bridge script itself.
pub fn install_default_project(targets: &[ReaperInstall]) -> Vec<String> {
    let src = match default_project_source_path() {
        Ok(p) => p,
        Err(_) => return vec!["default project not bundled; skipped (bridge still installs normally)".to_string()],
    };
    if targets.is_empty() {
        return vec!["No REAPER installation found; skipped default project.".to_string()];
    }

    let mut results = Vec::new();
    for install in targets {
        let resource_path = PathBuf::from(&install.resource_path);
        let dest = resource_path.join(DEFAULT_PROJECT_INSTALLED_FILENAME);
        match std::fs::create_dir_all(&resource_path) {
            Ok(()) => {
                if dest.exists() && files_identical(&src, &dest) {
                    results.push(format!("default project up to date: {}", dest.display()));
                } else if let Err(e) = std::fs::copy(&src, &dest) {
                    results.push(format!("skipped ({e}): {}", dest.display()));
                } else {
                    results.push(format!("default project installed: {}", dest.display()));
                }
            }
            Err(e) => results.push(format!("skipped ({e}): {}", dest.display())),
        }
    }
    results
}

fn startup_block() -> String {
    format!(
        "{STARTUP_START_MARKER}\n\
         pcall(dofile, reaper.GetResourcePath() .. \"/Scripts/reaper_bridge.lua\")\n\
         pcall(reaper.Main_openProject, reaper.GetResourcePath() .. \"/{DEFAULT_PROJECT_INSTALLED_FILENAME}\")\n\
         {STARTUP_END_MARKER}\n"
    )
}

/// Insert/replace our marker-delimited block, leaving the rest of the
/// user's own `__startup.lua` content untouched.
fn merge_startup_content(existing: &str) -> String {
    let block = startup_block();
    if let (Some(start_idx), Some(end_idx)) =
        (existing.find(STARTUP_START_MARKER), existing.find(STARTUP_END_MARKER))
    {
        if end_idx > start_idx {
            let end_idx_full = end_idx + STARTUP_END_MARKER.len();
            return format!("{}{}{}", &existing[..start_idx], block.trim_end_matches('\n'), &existing[end_idx_full..]);
        }
    }

    if existing.is_empty() || existing.ends_with('\n') {
        format!("{existing}{block}")
    } else {
        format!("{existing}\n{block}")
    }
}

/// Idempotently wire `reaper_bridge.lua` and the default project into
/// REAPER's native `__startup.lua` (auto-run at launch, no extension
/// required) without disturbing the user's own startup script content.
pub fn install_startup_hook(targets: &[ReaperInstall]) -> Vec<String> {
    if targets.is_empty() {
        return vec!["No REAPER installation found; skipped startup hook.".to_string()];
    }

    let mut results = Vec::new();
    for install in targets {
        let scripts_dir = PathBuf::from(&install.scripts_dir);
        let dest = scripts_dir.join(STARTUP_FILENAME);
        if let Err(e) = std::fs::create_dir_all(&scripts_dir) {
            results.push(format!("skipped ({e}): {}", dest.display()));
            continue;
        }
        let existing = std::fs::read_to_string(&dest).unwrap_or_default();
        let merged = merge_startup_content(&existing);
        if merged == existing {
            results.push(format!("startup hook up to date: {}", dest.display()));
            continue;
        }
        match std::fs::write(&dest, &merged) {
            Ok(()) => results.push(format!("startup hook installed: {}", dest.display())),
            Err(e) => results.push(format!("skipped ({e}): {}", dest.display())),
        }
    }
    results
}

#[cfg(test)]
mod tests {
    //! Ported from `tests/test_installer.py`. `install_bridge`/
    //! `install_default_project`'s source files resolve to this repo's real
    //! `lua/reaper_bridge.lua`/`reaper_project/default.RPP` at compile time
    //! (not mockable like the Python `patch()`-based tests were) - these
    //! exercise the real bundled files instead, which is at least as
    //! faithful to actual installed behavior.

    use super::*;

    fn fake_install(dir: &Path) -> ReaperInstall {
        let resource_path = dir.join("REAPER");
        let scripts_dir = resource_path.join("Scripts");
        std::fs::create_dir_all(&scripts_dir).unwrap();
        ReaperInstall {
            resource_path: resource_path.to_string_lossy().to_string(),
            scripts_dir: scripts_dir.to_string_lossy().to_string(),
            source: "test".to_string(),
        }
    }

    #[test]
    fn startup_hook_creates_fresh_file() {
        let dir = tempfile::tempdir().unwrap();
        let install = fake_install(dir.path());
        let results = install_startup_hook(&[install]);

        let dest = dir.path().join("REAPER").join("Scripts").join(STARTUP_FILENAME);
        let content = std::fs::read_to_string(&dest).unwrap();
        assert!(content.contains(STARTUP_START_MARKER));
        assert!(content.contains(STARTUP_END_MARKER));
        assert!(content.contains("reaper_bridge.lua"));
        assert!(content.contains("Main_openProject"));
        assert!(content.contains(DEFAULT_PROJECT_INSTALLED_FILENAME));
        assert!(results.iter().any(|r| r.contains("installed")));
    }

    #[test]
    fn startup_hook_preserves_foreign_content() {
        let dir = tempfile::tempdir().unwrap();
        let install = fake_install(dir.path());
        let dest = dir.path().join("REAPER").join("Scripts").join(STARTUP_FILENAME);
        std::fs::write(&dest, "-- my own startup logic\nreaper.ShowConsoleMsg('hi')\n").unwrap();

        install_startup_hook(&[install]);

        let content = std::fs::read_to_string(&dest).unwrap();
        assert!(content.contains("my own startup logic"));
        assert!(content.contains("ShowConsoleMsg"));
        assert!(content.contains(STARTUP_START_MARKER));
    }

    #[test]
    fn startup_hook_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let install = fake_install(dir.path());
        let dest = dir.path().join("REAPER").join("Scripts").join(STARTUP_FILENAME);

        install_startup_hook(&[install.clone()]);
        let first = std::fs::read_to_string(&dest).unwrap();
        let results = install_startup_hook(&[install]);
        let second = std::fs::read_to_string(&dest).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.matches(STARTUP_START_MARKER).count(), 1);
        assert!(results.iter().any(|r| r.contains("up to date")));
    }

    #[test]
    fn startup_hook_replaces_existing_block_not_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let install = fake_install(dir.path());
        let dest = dir.path().join("REAPER").join("Scripts").join(STARTUP_FILENAME);
        std::fs::write(
            &dest,
            format!(
                "-- before\n{STARTUP_START_MARKER}\n-- stale content that should be replaced\n{STARTUP_END_MARKER}\n-- after\n"
            ),
        )
        .unwrap();

        install_startup_hook(&[install]);

        let content = std::fs::read_to_string(&dest).unwrap();
        assert_eq!(content.matches(STARTUP_START_MARKER).count(), 1);
        assert_eq!(content.matches(STARTUP_END_MARKER).count(), 1);
        assert!(content.contains("-- before"));
        assert!(content.contains("-- after"));
        assert!(!content.contains("stale content"));
        assert!(content.contains("reaper_bridge.lua"));
    }

    #[test]
    fn default_project_copies_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let install = fake_install(dir.path());

        let first = install_default_project(&[install.clone()]);
        assert!(first.iter().any(|r| r.contains("installed")));

        let dest = dir.path().join("REAPER").join(DEFAULT_PROJECT_INSTALLED_FILENAME);
        assert!(dest.exists());

        let second = install_default_project(&[install]);
        assert!(second.iter().any(|r| r.contains("up to date")));
    }

    #[test]
    fn install_bridge_also_installs_startup_hook_and_default_project() {
        let dir = tempfile::tempdir().unwrap();
        let install = fake_install(dir.path());

        let results = install_bridge_into(&[install.clone()]);

        let startup_file = dir.path().join("REAPER").join("Scripts").join(STARTUP_FILENAME);
        assert!(startup_file.exists());
        assert!(results.iter().any(|r| r.contains("startup hook")));
        assert!(results.iter().any(|r| r.contains("default project")));
    }

    /// Test-only seam mirroring `install_bridge()` but against an explicit
    /// target list instead of live discovery, so tests don't depend on
    /// what's actually installed on the machine running them.
    fn install_bridge_into(targets: &[ReaperInstall]) -> Vec<String> {
        let src = bridge_source_path().unwrap();
        let mut results = Vec::new();
        for install in targets {
            let scripts_dir = PathBuf::from(&install.scripts_dir);
            let dest = scripts_dir.join(BRIDGE_FILENAME);
            std::fs::create_dir_all(&scripts_dir).unwrap();
            if dest.exists() && files_identical(&src, &dest) {
                results.push(format!("up to date: {}", dest.display()));
            } else {
                std::fs::copy(&src, &dest).unwrap();
                results.push(format!("installed: {}", dest.display()));
            }
        }
        results.extend(install_default_project(targets));
        results.extend(install_startup_hook(targets));
        results
    }
}
