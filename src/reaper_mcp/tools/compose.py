"""One-call composition: new track + MIDI notes + render to audio.

This is a higher-level tool over track_add/midi_add_item/midi_add_note/
render_project for the common "write a melody, get an audio file" workflow -
it collapses what would otherwise be a track_add call, a midi_add_item call,
one midi_add_note call per note, and a render_project call into one request/
response round trip against the bridge, and sets exact render bounds
matching the composed notes (see render.py's render_project docstring for
why explicit bounds matter).
"""

from __future__ import annotations

from ..app import call_bridge, mcp


@mcp.tool()
def compose_and_render(
    output_path: str,
    notes: list[dict],
    track_name: str = "Composed",
    overwrite: bool = False,
    auto_instrument: bool = True,
) -> dict:
    """Create a new track, write a MIDI melody to it from a note list, and
    render it to an audio file, in one call. Each note in notes is a dict:
    {pitch, start_sec, end_sec, velocity?, channel?} - pitch 0-127 (60 =
    middle C), velocity 1-127 (defaults to 64), channel defaults to 0,
    start_sec/end_sec are absolute seconds from the start of the new MIDI
    item (which spans 0 to the last note's end_sec + 0.5s tail). Returns
    track_index, render_end_sec, and the actual output_path used.

    MIDI notes are silent without a virtual instrument on the track -
    neither live playback nor a render produces audible sound otherwise.
    By default (auto_instrument=True), if the new track has no instrument
    loaded, this adds REAPER's built-in ReaSynth so the render is actually
    audible. Pass auto_instrument=False to skip this if you're adding your
    own instrument/FX chain separately before rendering.

    If output_path already exists: by default, renders to the next
    available name instead (output_path with "_2", "_3", etc. inserted
    before the extension) - never touches the existing file, never shows a
    dialog. Pass overwrite=True to delete the existing file and render to
    the exact requested output_path instead.

    Audio format/codec (WAV, MP3, bit depth, etc.) is NOT set by this tool -
    it comes from REAPER's currently configured render settings. Use
    File -> Render once in REAPER to set the format you want; this tool
    only controls the notes, the render time range, the output path, and
    overwrite behavior.
    """
    return call_bridge(
        "compose_and_render",
        timeout=60,
        output_path=output_path,
        notes=notes,
        track_name=track_name,
        overwrite=overwrite,
        auto_instrument=auto_instrument,
    )
