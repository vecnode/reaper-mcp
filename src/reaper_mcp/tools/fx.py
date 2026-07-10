"""FX / plugin control: add/remove/enable/params on a track's FX chain."""

from __future__ import annotations

from ..app import call_bridge, mcp


@mcp.tool()
def fx_add(track_index: int, fx_name: str) -> dict:
    """Add an FX/plugin (VST/VST3/AU/JS by name, e.g. 'ReaEQ' or 'VST3: Pro-Q 3
    (FabFilter)') to the end of a track's FX chain. Returns the new fx_index."""
    return call_bridge("fx_add", track_index=track_index, fx_name=fx_name)


@mcp.tool()
def fx_remove(track_index: int, fx_index: int) -> dict:
    """Remove the FX at fx_index from a track's FX chain."""
    return call_bridge("fx_remove", track_index=track_index, fx_index=fx_index)


@mcp.tool()
def fx_set_enabled(track_index: int, fx_index: int, enabled: bool) -> dict:
    """Bypass (False) or enable (True) an FX on a track."""
    return call_bridge("fx_set_enabled", track_index=track_index, fx_index=fx_index, enabled=enabled)


@mcp.tool()
def fx_list(track_index: int) -> dict:
    """List all FX on a track's FX chain with their index, name, and enabled state."""
    return call_bridge("fx_list", track_index=track_index)


@mcp.tool()
def fx_set_param(track_index: int, fx_index: int, param_index: int, value: float) -> dict:
    """Set a normalized (0.0-1.0) parameter value on a track's FX."""
    return call_bridge(
        "fx_set_param", track_index=track_index, fx_index=fx_index, param_index=param_index, value=value
    )


@mcp.tool()
def fx_get_param(track_index: int, fx_index: int, param_index: int) -> dict:
    """Get a normalized (0.0-1.0) parameter value from a track's FX."""
    return call_bridge("fx_get_param", track_index=track_index, fx_index=fx_index, param_index=param_index)
