"""Project-level operations: save, undo."""

from __future__ import annotations

from ..app import call_bridge, mcp


@mcp.tool()
def project_save() -> dict:
    """Save the current project."""
    return call_bridge("project_save")


@mcp.tool()
def project_undo() -> dict:
    """Undo the last action in REAPER."""
    return call_bridge("project_undo")


@mcp.tool()
def reaper_info() -> dict:
    """Get REAPER version, resource path, current project path, track count, and play state."""
    return call_bridge("get_reaper_info")
