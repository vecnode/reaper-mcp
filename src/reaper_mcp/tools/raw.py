"""Escape hatch: execute arbitrary ReaScript Lua inside REAPER."""

from __future__ import annotations

from ..app import call_bridge, mcp


@mcp.tool()
def run_reascript(code: str) -> dict:
    """Execute arbitrary Lua code inside REAPER's ReaScript environment (full
    access to the reaper.* API) and return its result as a string. Use this for
    anything not covered by a dedicated tool -- e.g. advanced routing, item
    editing, or calling ReaScript functions with no wrapper yet. Be precise:
    this runs with the same permissions as any ReaScript."""
    return call_bridge("run_reascript", code=code)
