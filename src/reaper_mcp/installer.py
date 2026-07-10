"""Installs reaper_bridge.lua into the detected REAPER Scripts directory."""

from __future__ import annotations

import filecmp
import shutil
from pathlib import Path

from .discovery import ReaperInstall, find_reaper_installs

BRIDGE_FILENAME = "reaper_bridge.lua"


def bridge_source_path() -> Path:
    # lua/reaper_bridge.lua lives at repo_root/lua/, two levels up from this file
    # (src/reaper_mcp/installer.py -> repo_root/lua/reaper_bridge.lua)
    repo_root = Path(__file__).resolve().parents[2]
    path = repo_root / "lua" / BRIDGE_FILENAME
    if not path.exists():
        raise FileNotFoundError(f"bundled bridge script not found at {path}")
    return path


def install_bridge(install: ReaperInstall | None = None) -> list[str]:
    """Copy reaper_bridge.lua into one or all detected REAPER Scripts dirs.

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
                continue

            shutil.copy2(src, dest)
            results.append(f"installed: {dest}")
        except PermissionError:
            results.append(f"skipped (no write permission): {dest}")

    results.append(
        "Next step (one-time, per REAPER instance): in REAPER, open Actions -> Show action "
        "list -> New action -> Load ReaScript..., select reaper_bridge.lua, then Run it "
        "(or right-click -> 'Run on startup' so it's always active). No extensions required."
    )
    return results
