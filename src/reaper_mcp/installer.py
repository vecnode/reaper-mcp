"""Installs reaper_bridge.lua into the detected REAPER Scripts directory,
and wires it into REAPER's native __startup.lua so it auto-runs on launch.
"""

from __future__ import annotations

import filecmp
import shutil
from pathlib import Path

from .discovery import ReaperInstall, find_reaper_installs

BRIDGE_FILENAME = "reaper_bridge.lua"
STARTUP_FILENAME = "__startup.lua"
STARTUP_START_MARKER = "-- reaper-mcp:start"
STARTUP_END_MARKER = "-- reaper-mcp:end"


def bridge_source_path() -> Path:
    # lua/reaper_bridge.lua lives at repo_root/lua/, two levels up from this file
    # (src/reaper_mcp/installer.py -> repo_root/lua/reaper_bridge.lua)
    repo_root = Path(__file__).resolve().parents[2]
    path = repo_root / "lua" / BRIDGE_FILENAME
    if not path.exists():
        raise FileNotFoundError(f"bundled bridge script not found at {path}")
    return path


def install_bridge(install: ReaperInstall | None = None) -> list[str]:
    """Copy reaper_bridge.lua into one or all detected REAPER Scripts dirs,
    and wire it into __startup.lua so it auto-runs on the next REAPER launch.

    Returns a list of human-readable status lines for each install acted on.
    """
    src = bridge_source_path()
    targets = [install] if install else find_reaper_installs()
    if not targets:
        return ["No REAPER installation found. Set REAPER_RESOURCE_PATH or install REAPER first."]

    results = []
    for inst in targets:
        scripts_dir = Path(inst.scripts_dir)
        dest = scripts_dir / BRIDGE_FILENAME
        try:
            scripts_dir.mkdir(parents=True, exist_ok=True)

            if dest.exists() and filecmp.cmp(src, dest, shallow=False):
                results.append(f"up to date: {dest}")
            else:
                shutil.copy2(src, dest)
                results.append(f"installed: {dest}")
        except PermissionError:
            results.append(f"skipped (no write permission): {dest}")

    results.extend(install_startup_hook(install))

    results.append(
        "The bridge is wired into REAPER's __startup.lua and will auto-run the next "
        "time REAPER launches (fully quit and reopen REAPER if it's already running "
        "for this to take effect). No manual Actions-list step or extensions required."
    )
    return results


def _startup_block() -> str:
    return (
        f"{STARTUP_START_MARKER}\n"
        'pcall(dofile, reaper.GetResourcePath() .. "/Scripts/reaper_bridge.lua")\n'
        f"{STARTUP_END_MARKER}\n"
    )


def _merge_startup_content(existing: str) -> str:
    """Insert/replace our marker-delimited block, leaving any of the user's
    own __startup.lua content untouched."""
    block = _startup_block()
    start_idx = existing.find(STARTUP_START_MARKER)
    end_idx = existing.find(STARTUP_END_MARKER)
    if start_idx != -1 and end_idx != -1 and end_idx > start_idx:
        end_idx_full = end_idx + len(STARTUP_END_MARKER)
        return existing[:start_idx] + block.rstrip("\n") + existing[end_idx_full:]

    if existing and not existing.endswith("\n"):
        existing += "\n"
    return existing + block


def install_startup_hook(install: ReaperInstall | None = None) -> list[str]:
    """Idempotently wire reaper_bridge.lua into REAPER's native __startup.lua
    (a file REAPER auto-runs at launch; no extension required), without
    disturbing any of the user's own startup script content."""
    targets = [install] if install else find_reaper_installs()
    if not targets:
        return ["No REAPER installation found; skipped startup hook."]

    results = []
    for inst in targets:
        scripts_dir = Path(inst.scripts_dir)
        dest = scripts_dir / STARTUP_FILENAME
        try:
            scripts_dir.mkdir(parents=True, exist_ok=True)
            existing = dest.read_text(encoding="utf-8") if dest.exists() else ""
            merged = _merge_startup_content(existing)
            if merged == existing:
                results.append(f"startup hook up to date: {dest}")
                continue
            dest.write_text(merged, encoding="utf-8")
            results.append(f"startup hook installed: {dest}")
        except PermissionError:
            results.append(f"skipped (no write permission): {dest}")
    return results
