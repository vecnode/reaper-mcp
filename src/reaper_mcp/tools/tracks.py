"""Track CRUD: add/remove/rename/volume/pan/mute/solo/color/list."""

from __future__ import annotations

from ..app import call_bridge, mcp


@mcp.tool()
def track_add(name: str | None = None, index: int | None = None) -> dict:
    """Insert a new track, optionally at a specific 0-based index (default: end of
    track list) and with a given name."""
    kwargs = {}
    if index is not None:
        kwargs["index"] = index
    if name is not None:
        kwargs["name"] = name
    return call_bridge("track_add", **kwargs)


@mcp.tool()
def track_remove(track_index: int) -> dict:
    """Delete the track at the given 0-based index."""
    return call_bridge("track_remove", track_index=track_index)


@mcp.tool()
def track_rename(track_index: int, name: str) -> dict:
    """Rename the track at the given 0-based index."""
    return call_bridge("track_rename", track_index=track_index, name=name)


@mcp.tool()
def track_set_volume_db(track_index: int, db: float) -> dict:
    """Set a track's volume fader in dB (0 = unity gain)."""
    return call_bridge("track_set_volume_db", track_index=track_index, db=db)


@mcp.tool()
def track_set_pan(track_index: int, pan: float) -> dict:
    """Set a track's pan, from -1.0 (full left) to 1.0 (full right)."""
    return call_bridge("track_set_pan", track_index=track_index, pan=pan)


@mcp.tool()
def track_set_mute(track_index: int, mute: bool) -> dict:
    """Mute or unmute a track."""
    return call_bridge("track_set_mute", track_index=track_index, mute=mute)


@mcp.tool()
def track_set_solo(track_index: int, solo: bool) -> dict:
    """Solo or unsolo a track."""
    return call_bridge("track_set_solo", track_index=track_index, solo=solo)


@mcp.tool()
def track_set_color(track_index: int, r: int, g: int, b: int) -> dict:
    """Set a track's color using 0-255 RGB components."""
    return call_bridge("track_set_color", track_index=track_index, r=r, g=g, b=b)


@mcp.tool()
def track_list() -> dict:
    """List all tracks in the current project with name, mute/solo state, volume (dB), and pan."""
    return call_bridge("track_list")
