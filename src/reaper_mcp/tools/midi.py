"""MIDI note editing via the run_reascript escape hatch.

REAPER's MIDI API (MIDI_InsertNote, MIDI_Sort, etc.) operates on a take, which
requires an existing MIDI item -- these helpers build the right ReaScript calls
so callers don't have to hand-write Lua for common MIDI workflows.
"""

from __future__ import annotations

from ..app import call_bridge, mcp


@mcp.tool()
def midi_add_item(track_index: int, start_sec: float, end_sec: float) -> dict:
    """Create an empty MIDI item on a track spanning start_sec to end_sec.
    Returns nothing useful yet to reference the item by -- use midi_add_note
    with the same track/time range, which locates the item itself."""
    code = f"""
local tr = reaper.GetTrack(0, {track_index})
local item = reaper.CreateNewMIDIItemInProj(tr, {start_sec}, {end_sec}, false)
return "ok"
"""
    return call_bridge("run_reascript", code=code)


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
    code = f"""
local tr = reaper.GetTrack(0, {track_index})
local item = nil
for i = 0, reaper.CountTrackMediaItems(tr) - 1 do
  local it = reaper.GetTrackMediaItem(tr, i)
  if math.abs(reaper.GetMediaItemInfo_Value(it, "D_POSITION") - ({item_start_sec})) < 0.001 then
    item = it
    break
  end
end
if not item then error("no MIDI item found starting at {item_start_sec} on track {track_index}") end
local take = reaper.GetActiveTake(item)
local item_pos = reaper.GetMediaItemInfo_Value(item, "D_POSITION")
local ppq_start = reaper.MIDI_GetPPQPosFromProjTime(take, {note_start_sec})
local ppq_end = reaper.MIDI_GetPPQPosFromProjTime(take, {note_end_sec})
reaper.MIDI_InsertNote(take, false, false, ppq_start, ppq_end, {channel}, {pitch}, {velocity}, false)
reaper.MIDI_Sort(take)
return "ok"
"""
    return call_bridge("run_reascript", code=code)
