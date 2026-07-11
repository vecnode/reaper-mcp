"""MIDI item and note editing."""

from __future__ import annotations

from ..app import call_bridge, mcp


@mcp.tool()
def midi_add_item(track_index: int, start_sec: float, end_sec: float) -> dict:
    """Create an empty MIDI item on a track spanning start_sec to end_sec.
    Use midi_add_note with the same track/item_start_sec afterward to add
    notes to it."""
    return call_bridge("midi_add_item", track_index=track_index, start_sec=start_sec, end_sec=end_sec)


@mcp.tool()
def midi_add_note(
    track_index: int,
    item_start_sec: float,
    pitch: int,
    velocity: int,
    note_start_sec: float,
    note_end_sec: float,
    channel: int = 0,
) -> dict:
    """Add a MIDI note to the MIDI item on a track that starts at item_start_sec.
    note_start_sec/note_end_sec are absolute project time in seconds. pitch is
    0-127 (60 = middle C), velocity is 1-127."""
    return call_bridge(
        "midi_add_note",
        track_index=track_index,
        item_start_sec=item_start_sec,
        pitch=pitch,
        velocity=velocity,
        note_start_sec=note_start_sec,
        note_end_sec=note_end_sec,
        channel=channel,
    )
