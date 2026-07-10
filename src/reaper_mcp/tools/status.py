"""Discovery / diagnostics tool: is REAPER running, where, and is the bridge live."""

from __future__ import annotations

from ..app import mcp
from ..discovery import run_discovery


@mcp.tool()
def reaper_status() -> dict:
    """Report REAPER installs found, running REAPER processes (with PIDs), and
    whether the reaper_bridge.lua socket bridge is reachable. Call this first
    when diagnosing connection issues."""
    return run_discovery().as_dict()


@mcp.tool()
def install_bridge() -> dict:
    """Copy reaper_bridge.lua into the detected REAPER Scripts folder(s). This
    does not start it -- REAPER must load it once via its Actions list (or you
    can set it to run on startup)."""
    from ..installer import install_bridge as _install

    return {"messages": _install()}
