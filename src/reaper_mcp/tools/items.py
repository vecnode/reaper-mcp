"""Media item operations: split, move, glue, render-in-place -- via run_reascript."""

from __future__ import annotations

from ..app import call_bridge, mcp


@mcp.tool()
def item_split(track_index: int, item_start_sec: float, split_at_sec: float) -> dict:
    """Split the media item on a track that starts at item_start_sec, at split_at_sec."""
    code = f"""
local tr = reaper.GetTrack(0, {track_index})
for i = 0, reaper.CountTrackMediaItems(tr) - 1 do
  local it = reaper.GetTrackMediaItem(tr, i)
  if math.abs(reaper.GetMediaItemInfo_Value(it, "D_POSITION") - ({item_start_sec})) < 0.001 then
    reaper.SplitMediaItem(it, {split_at_sec})
    return "ok"
  end
end
error("no item found starting at {item_start_sec} on track {track_index}")
"""
    return call_bridge("run_reascript", code=code)


@mcp.tool()
def item_move(track_index: int, item_start_sec: float, new_start_sec: float) -> dict:
    """Move the media item on a track that starts at item_start_sec to new_start_sec."""
    code = f"""
local tr = reaper.GetTrack(0, {track_index})
for i = 0, reaper.CountTrackMediaItems(tr) - 1 do
  local it = reaper.GetTrackMediaItem(tr, i)
  if math.abs(reaper.GetMediaItemInfo_Value(it, "D_POSITION") - ({item_start_sec})) < 0.001 then
    reaper.SetMediaItemInfo_Value(it, "D_POSITION", {new_start_sec})
    return "ok"
  end
end
error("no item found starting at {item_start_sec} on track {track_index}")
"""
    return call_bridge("run_reascript", code=code)


@mcp.tool()
def item_glue_selected() -> dict:
    """Glue all currently selected media items into one item each (per track)."""
    code = """
reaper.Main_OnCommand(41588, 0) -- Item: Glue items (ignoring time selection)
return "ok"
"""
    return call_bridge("run_reascript", code=code)


@mcp.tool()
def item_render_in_place_selected() -> dict:
    """Render selected items in place (applies FX/pitch/rate destructively to new item)."""
    code = """
reaper.Main_OnCommand(41999, 0) -- Item: Render items to new take (mono, use pitch/rate)
return "ok"
"""
    return call_bridge("run_reascript", code=code)
