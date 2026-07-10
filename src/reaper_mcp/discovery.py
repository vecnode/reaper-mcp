"""Engineer-style discovery of REAPER installs, running processes, and bridge state.

Scans known install locations across OS, inspects the running process list for
reaper/REAPER executables (reporting PIDs), and checks the bridge's heartbeat
file so we can give the user actionable diagnostics instead of a bare error.
"""

from __future__ import annotations

import os
import platform
import shutil
from dataclasses import dataclass, field
from pathlib import Path

import psutil


@dataclass
class ReaperProcess:
    pid: int
    exe: str | None
    name: str


@dataclass
class ReaperInstall:
    resource_path: str
    scripts_dir: str
    source: str  # how we found it: "known_path" or "env"


@dataclass
class DiscoveryReport:
    os_name: str
    running_processes: list[ReaperProcess] = field(default_factory=list)
    installs: list[ReaperInstall] = field(default_factory=list)
    bridge_reachable: bool = False
    bridge_dir: str = ""

    def as_dict(self) -> dict:
        return {
            "os": self.os_name,
            "running_processes": [p.__dict__ for p in self.running_processes],
            "installs": [i.__dict__ for i in self.installs],
            "bridge_reachable": self.bridge_reachable,
            "bridge_dir": self.bridge_dir,
        }


REAPER_PROCESS_NAMES = {"reaper.exe", "reaper"}


def find_running_reaper() -> list[ReaperProcess]:
    found = []
    for proc in psutil.process_iter(["pid", "name", "exe"]):
        try:
            name = (proc.info.get("name") or "").lower()
            if name in REAPER_PROCESS_NAMES:
                found.append(
                    ReaperProcess(pid=proc.info["pid"], exe=proc.info.get("exe"), name=proc.info.get("name"))
                )
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            continue
    return found


def _candidate_resource_paths() -> list[tuple[str, str]]:
    """Return (path, source_label) candidates for REAPER's resource directory per-OS."""
    system = platform.system()
    candidates: list[tuple[str, str]] = []

    if system == "Windows":
        appdata = os.environ.get("APPDATA")
        if appdata:
            candidates.append((str(Path(appdata) / "REAPER"), "known_path"))
        for env_var in ("PROGRAMFILES", "PROGRAMFILES(X86)"):
            base = os.environ.get(env_var)
            if base:
                candidates.append((str(Path(base) / "REAPER (x64)"), "known_path"))
                candidates.append((str(Path(base) / "REAPER"), "known_path"))
    elif system == "Darwin":
        home = Path.home()
        candidates.append((str(home / "Library" / "Application Support" / "REAPER"), "known_path"))
        candidates.append(("/Applications/REAPER64.app", "known_path"))
    else:  # Linux
        home = Path.home()
        candidates.append((str(home / ".config" / "REAPER"), "known_path"))

    env_override = os.environ.get("REAPER_RESOURCE_PATH")
    if env_override:
        candidates.insert(0, (env_override, "env"))

    return candidates


def find_reaper_installs() -> list[ReaperInstall]:
    installs = []
    seen = set()
    for path_str, source in _candidate_resource_paths():
        path = Path(path_str)
        if not path.exists() or path_str in seen:
            continue
        seen.add(path_str)
        scripts_dir = path / "Scripts"
        installs.append(
            ReaperInstall(resource_path=str(path), scripts_dir=str(scripts_dir), source=source)
        )
    return installs


def run_discovery() -> DiscoveryReport:
    from .bridge_client import BridgeClient

    client = BridgeClient()
    report = DiscoveryReport(os_name=platform.system(), bridge_dir=str(client.config.bridge_dir))
    report.running_processes = find_running_reaper()
    report.installs = find_reaper_installs()
    report.bridge_reachable = client.is_alive()
    return report


def which_reaper() -> str | None:
    """Best-effort lookup of a reaper executable on PATH."""
    return shutil.which("reaper") or shutil.which("reaper.exe")
