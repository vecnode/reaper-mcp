"""Transport control: play/stop/pause/record/seek/tempo."""

from __future__ import annotations

from ..app import call_bridge, mcp


@mcp.tool()
def transport_play() -> dict:
    """Start playback from the current edit cursor position."""
    return call_bridge("transport_play")


@mcp.tool()
def transport_stop() -> dict:
    """Stop playback/recording."""
    return call_bridge("transport_stop")


@mcp.tool()
def transport_pause() -> dict:
    """Pause playback."""
    return call_bridge("transport_pause")


@mcp.tool()
def transport_record() -> dict:
    """Arm and start recording."""
    return call_bridge("transport_record")


@mcp.tool()
def transport_seek(position_sec: float) -> dict:
    """Move the edit cursor to an absolute position in seconds."""
    return call_bridge("transport_seek", position_sec=position_sec)


@mcp.tool()
def transport_set_tempo(bpm: float) -> dict:
    """Set the project tempo in beats per minute."""
    return call_bridge("transport_set_tempo", bpm=bpm)


@mcp.tool()
def transport_get_state() -> dict:
    """Get current play state, cursor position (seconds), and tempo."""
    return call_bridge("transport_get_state")
