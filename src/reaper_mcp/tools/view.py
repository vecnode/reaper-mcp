"""Arrangement view: zoom and scroll."""

from __future__ import annotations

from ..app import call_bridge, mcp


@mcp.tool()
def view_zoom_to_selection() -> dict:
    """Zoom the arrange view horizontally to fit the currently selected item(s)."""
    return call_bridge("view_zoom_to_selection")


@mcp.tool()
def view_scroll_to(position_sec: float) -> dict:
    """Scroll the arrange view so the given position (seconds) is visible."""
    return call_bridge("view_scroll_to", position_sec=position_sec)


@mcp.tool()
def view_set_arrange_zoom(pixels_per_sec: float) -> dict:
    """Set the arrange view's horizontal zoom level, in pixels per second."""
    return call_bridge("view_set_arrange_zoom", pixels_per_sec=pixels_per_sec)
