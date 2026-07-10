"""Project markers and regions."""

from __future__ import annotations

from ..app import call_bridge, mcp


@mcp.tool()
def marker_add(position_sec: float, name: str | None = None) -> dict:
    """Add a project marker at the given position in seconds."""
    return call_bridge("marker_add", position_sec=position_sec, name=name or "")


@mcp.tool()
def region_add(start_sec: float, end_sec: float, name: str | None = None) -> dict:
    """Add a project region spanning start_sec to end_sec."""
    return call_bridge("region_add", start_sec=start_sec, end_sec=end_sec, name=name or "")
