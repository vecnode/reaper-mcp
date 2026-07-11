--[[
 reaper_bridge.lua

 File-based IPC bridge for reaper-mcp, polled every reaper.defer() tick
 (REAPER's UI frame rate, so effectively ~16-33ms round trips -- fast enough
 for interactive control without needing a real socket).

 The Python MCP server (see src/reaper_mcp/bridge_client.py) writes one JSON
 file per request into <resource_path>/Scripts/reaper_mcp_bridge/requests/,
 this script picks it up on its next defer() tick, dispatches it to a thin
 wrapper function around the reaper.* ReaScript API, and writes the JSON
 response into .../responses/. A heartbeat.txt file is touched every tick so
 the Python side can tell "bridge loaded and running" apart from "REAPER
 open but script not started".

 No REAPER extensions required -- this uses only the standard io/os Lua
 libraries and the reaper.* API, both available in vanilla ReaScript.

 Also draws a small "reaper-mcp" status window (via REAPER's built-in gfx library)
 in the same defer() loop, docked by default, showing whether the bridge
 has processed a request recently. This is honest about what it can show:
 since this is file-polling IPC, not a live socket, it reflects "bridge
 script is running" and "last request seen Ns ago" -- not a persistent
 "Claude is attached right now" signal, since MCP clients only connect when
 actively calling a tool. Drag it to whichever docker position you prefer;
 that position is remembered across REAPER restarts.

 Does not call reaper.ShowConsoleMsg() on routine startup (that forces the
 ReaScript console open, disruptive now that this auto-runs via
 __startup.lua on every launch) -- only on real pump/gfx errors.

 Install: copy this file into REAPER's Scripts folder (or let
 `uv run reaper-mcp --install-bridge` do it for you, which also wires up
 REAPER's native __startup.lua so this runs automatically on every REAPER
 launch -- no manual Actions-list step needed after the first install).
 Manual load, if you ever need it: Actions -> Show action list -> New
 action -> Load ReaScript... -> select this file -> Run.
--]]

local SEP = package.config:sub(1, 1)
local BRIDGE_DIR = reaper.GetResourcePath() .. SEP .. "Scripts" .. SEP .. "reaper_mcp_bridge"
local REQUESTS_DIR = BRIDGE_DIR .. SEP .. "requests"
local RESPONSES_DIR = BRIDGE_DIR .. SEP .. "responses"
local HEARTBEAT_FILE = BRIDGE_DIR .. SEP .. "heartbeat.txt"

----------------------------------------------------------------------------
-- Minimal JSON encode/decode (self-contained, no external deps)
----------------------------------------------------------------------------

local json = {}

local function json_escape(s)
  local out = s:gsub('[%c"\\]', function(c)
    local map = { ['"'] = '\\"', ['\\'] = '\\\\', ['\n'] = '\\n', ['\r'] = '\\r', ['\t'] = '\\t' }
    return map[c] or string.format('\\u%04x', c:byte())
  end)
  return out
end

function json.encode(v)
  local t = type(v)
  if t == "nil" then
    return "null"
  elseif t == "boolean" then
    return v and "true" or "false"
  elseif t == "number" then
    if v ~= v then return "0" end -- NaN guard
    return tostring(v)
  elseif t == "string" then
    return '"' .. json_escape(v) .. '"'
  elseif t == "table" then
    -- array if keys are a contiguous 1..n sequence
    local n = 0
    for _ in pairs(v) do n = n + 1 end
    local is_array = n > 0
    for i = 1, n do
      if v[i] == nil then is_array = false break end
    end
    if n == 0 then
      return v._is_array and "[]" or "{}"
    end
    if is_array then
      local parts = {}
      for i = 1, n do parts[i] = json.encode(v[i]) end
      return "[" .. table.concat(parts, ",") .. "]"
    else
      local parts = {}
      for k, val in pairs(v) do
        parts[#parts + 1] = '"' .. json_escape(tostring(k)) .. '":' .. json.encode(val)
      end
      return "{" .. table.concat(parts, ",") .. "}"
    end
  end
  return "null"
end

-- Small recursive-descent JSON parser.
local function json_decode(str)
  local pos = 1

  local function skip_ws()
    while pos <= #str and str:sub(pos, pos):match("%s") do pos = pos + 1 end
  end

  local parse_value

  local function parse_string()
    pos = pos + 1 -- opening quote
    local start = pos
    local buf = {}
    while true do
      local c = str:sub(pos, pos)
      if c == "" then error("unterminated string") end
      if c == '"' then
        pos = pos + 1
        break
      elseif c == "\\" then
        local nc = str:sub(pos + 1, pos + 1)
        local map = { n = "\n", r = "\r", t = "\t", ['"'] = '"', ["\\"] = "\\", ["/"] = "/" }
        if map[nc] then
          buf[#buf + 1] = map[nc]
          pos = pos + 2
        elseif nc == "u" then
          local hex = str:sub(pos + 2, pos + 5)
          buf[#buf + 1] = string.char(tonumber(hex, 16) % 256)
          pos = pos + 6
        else
          buf[#buf + 1] = nc
          pos = pos + 2
        end
      else
        buf[#buf + 1] = c
        pos = pos + 1
      end
    end
    return table.concat(buf)
  end

  local function parse_number()
    local start = pos
    while pos <= #str and str:sub(pos, pos):match("[%d%.%-%+eE]") do pos = pos + 1 end
    return tonumber(str:sub(start, pos - 1))
  end

  local function parse_object()
    pos = pos + 1
    local obj = {}
    skip_ws()
    if str:sub(pos, pos) == "}" then pos = pos + 1 return obj end
    while true do
      skip_ws()
      local key = parse_string()
      skip_ws()
      pos = pos + 1 -- ':'
      skip_ws()
      obj[key] = parse_value()
      skip_ws()
      local c = str:sub(pos, pos)
      if c == "," then
        pos = pos + 1
      elseif c == "}" then
        pos = pos + 1
        break
      end
    end
    return obj
  end

  local function parse_array()
    pos = pos + 1
    local arr = {}
    skip_ws()
    if str:sub(pos, pos) == "]" then pos = pos + 1 return arr end
    while true do
      skip_ws()
      arr[#arr + 1] = parse_value()
      skip_ws()
      local c = str:sub(pos, pos)
      if c == "," then
        pos = pos + 1
      elseif c == "]" then
        pos = pos + 1
        break
      end
    end
    return arr
  end

  parse_value = function()
    skip_ws()
    local c = str:sub(pos, pos)
    if c == '"' then
      return parse_string()
    elseif c == "{" then
      return parse_object()
    elseif c == "[" then
      return parse_array()
    elseif str:sub(pos, pos + 3) == "true" then
      pos = pos + 4
      return true
    elseif str:sub(pos, pos + 4) == "false" then
      pos = pos + 5
      return false
    elseif str:sub(pos, pos + 3) == "null" then
      pos = pos + 4
      return nil
    else
      return parse_number()
    end
  end

  local ok, result = pcall(function()
    skip_ws()
    return parse_value()
  end)
  if not ok then return nil, result end
  return result
end

json.decode = json_decode

----------------------------------------------------------------------------
-- Track / arg helpers
----------------------------------------------------------------------------

local function get_track(idx)
  -- 0-based track index, matching ReaScript convention
  return reaper.GetTrack(0, math.floor(idx))
end

local function track_or_error(args)
  local tr = get_track(args.track_index or 0)
  if not tr then error("no track at index " .. tostring(args.track_index)) end
  return tr
end

local function find_item_at_start(tr, start_sec)
  for i = 0, reaper.CountTrackMediaItems(tr) - 1 do
    local it = reaper.GetTrackMediaItem(tr, i)
    if math.abs(reaper.GetMediaItemInfo_Value(it, "D_POSITION") - start_sec) < 0.001 then
      return it
    end
  end
  return nil
end

-- Shared by ops.midi_add_note and ops.compose_and_render so both go through
-- the same insertion logic.
local function insert_midi_note(item, pitch, velocity, note_start_sec, note_end_sec, channel)
  local take = reaper.GetActiveTake(item)
  local ppq_start = reaper.MIDI_GetPPQPosFromProjTime(take, note_start_sec)
  local ppq_end = reaper.MIDI_GetPPQPosFromProjTime(take, note_end_sec)
  reaper.MIDI_InsertNote(take, false, false, ppq_start, ppq_end, channel or 0, pitch, velocity, false)
end

-- RENDER_BOUNDSFLAG=0 means "custom time range" (vs. entire project / time
-- selection / regions / selected items), with RENDER_STARTPOS/RENDER_ENDPOS
-- giving that range explicitly. Verified via REAPER API research: these are
-- plain numeric GetSetProjectInfo keys. Deliberately not touching
-- RENDER_FORMAT (codec/bitrate) anywhere in this file -- that's a
-- base64-encoded binary fourcc blob, not a plain string, and isn't safe to
-- set without a verified reference encoding, so audio format still comes
-- from whatever REAPER's render dialog was last configured with (File ->
-- Render once to set it up).
local function set_render_bounds(start_sec, end_sec)
  reaper.GetSetProjectInfo(0, "RENDER_BOUNDSFLAG", 0, true)
  reaper.GetSetProjectInfo(0, "RENDER_STARTPOS", start_sec, true)
  reaper.GetSetProjectInfo(0, "RENDER_ENDPOS", end_sec, true)
end

----------------------------------------------------------------------------
-- Op handlers: op name -> function(args) -> result table
----------------------------------------------------------------------------

local ops = {}

ops.ping = function(args)
  return { pong = true, time = reaper.time_precise() }
end

ops.get_reaper_info = function(args)
  local _, project_path = reaper.EnumProjects(-1, "")
  return {
    version = reaper.GetAppVersion(),
    resource_path = reaper.GetResourcePath(),
    project_path = project_path,
    track_count = reaper.CountTracks(0),
    play_state = reaper.GetPlayState(),
  }
end

ops.run_reascript = function(args)
  local chunk, err = load(args.code)
  if not chunk then error("compile error: " .. tostring(err)) end
  local ok, result = pcall(chunk)
  if not ok then error("runtime error: " .. tostring(result)) end
  return { result = tostring(result) }
end

-- transport
ops.transport_play = function(args) reaper.OnPlayButton() return {} end
ops.transport_stop = function(args) reaper.OnStopButton() return {} end
ops.transport_pause = function(args) reaper.OnPauseButton() return {} end
ops.transport_record = function(args) reaper.OnRecordButton() return {} end
ops.transport_seek = function(args) reaper.SetEditCurPos(args.position_sec, true, true) return {} end
ops.transport_set_tempo = function(args) reaper.SetCurrentBPM(0, args.bpm, true) return {} end
ops.transport_get_state = function(args)
  return {
    play_state = reaper.GetPlayState(),
    position_sec = reaper.GetPlayPosition(),
    tempo = reaper.Master_GetTempo(),
  }
end

-- tracks
ops.track_add = function(args)
  local idx = args.index or reaper.CountTracks(0)
  reaper.InsertTrackAtIndex(idx, true)
  local tr = get_track(idx)
  if args.name then reaper.GetSetMediaTrackInfo_String(tr, "P_NAME", args.name, true) end
  return { index = idx }
end

ops.track_remove = function(args)
  local tr = track_or_error(args)
  reaper.DeleteTrack(tr)
  return {}
end

ops.track_rename = function(args)
  local tr = track_or_error(args)
  reaper.GetSetMediaTrackInfo_String(tr, "P_NAME", args.name, true)
  return {}
end

ops.track_set_volume_db = function(args)
  local tr = track_or_error(args)
  reaper.SetMediaTrackInfo_Value(tr, "D_VOL", 10 ^ (args.db / 20))
  return {}
end

ops.track_set_pan = function(args)
  local tr = track_or_error(args)
  reaper.SetMediaTrackInfo_Value(tr, "D_PAN", args.pan)
  return {}
end

ops.track_set_mute = function(args)
  local tr = track_or_error(args)
  reaper.SetMediaTrackInfo_Value(tr, "B_MUTE", args.mute and 1 or 0)
  return {}
end

ops.track_set_solo = function(args)
  local tr = track_or_error(args)
  reaper.SetMediaTrackInfo_Value(tr, "I_SOLO", args.solo and 1 or 0)
  return {}
end

ops.track_set_color = function(args)
  local tr = track_or_error(args)
  reaper.SetTrackColor(tr, reaper.ColorToNative(args.r, args.g, args.b))
  return {}
end

ops.track_list = function(args)
  local tracks = {}
  for i = 0, reaper.CountTracks(0) - 1 do
    local tr = reaper.GetTrack(0, i)
    local _, name = reaper.GetSetMediaTrackInfo_String(tr, "P_NAME", "", false)
    tracks[#tracks + 1] = {
      index = i,
      name = name,
      mute = reaper.GetMediaTrackInfo_Value(tr, "B_MUTE") == 1,
      solo = reaper.GetMediaTrackInfo_Value(tr, "I_SOLO") ~= 0,
      volume_db = 20 * math.log(reaper.GetMediaTrackInfo_Value(tr, "D_VOL"), 10),
      pan = reaper.GetMediaTrackInfo_Value(tr, "D_PAN"),
    }
  end
  return { tracks = tracks }
end

-- fx
ops.fx_add = function(args)
  local tr = track_or_error(args)
  local idx = reaper.TrackFX_AddByName(tr, args.fx_name, false, -1)
  if idx < 0 then error("fx not found: " .. tostring(args.fx_name)) end
  return { fx_index = idx }
end

ops.fx_remove = function(args)
  local tr = track_or_error(args)
  reaper.TrackFX_Delete(tr, args.fx_index)
  return {}
end

ops.fx_set_enabled = function(args)
  local tr = track_or_error(args)
  reaper.TrackFX_SetEnabled(tr, args.fx_index, args.enabled)
  return {}
end

ops.fx_list = function(args)
  local tr = track_or_error(args)
  local fx = {}
  for i = 0, reaper.TrackFX_GetCount(tr) - 1 do
    local _, name = reaper.TrackFX_GetFXName(tr, i, "")
    fx[#fx + 1] = { index = i, name = name, enabled = reaper.TrackFX_GetEnabled(tr, i) }
  end
  return { fx = fx }
end

ops.fx_set_param = function(args)
  local tr = track_or_error(args)
  reaper.TrackFX_SetParam(tr, args.fx_index, args.param_index, args.value)
  return {}
end

ops.fx_get_param = function(args)
  local tr = track_or_error(args)
  local val = reaper.TrackFX_GetParam(tr, args.fx_index, args.param_index)
  return { value = val }
end

-- midi
ops.midi_add_item = function(args)
  local tr = track_or_error(args)
  reaper.CreateNewMIDIItemInProj(tr, args.start_sec, args.end_sec, false)
  return {}
end

ops.midi_add_note = function(args)
  local tr = track_or_error(args)
  local item = find_item_at_start(tr, args.item_start_sec)
  if not item then
    error("no MIDI item found starting at " .. tostring(args.item_start_sec)
      .. " on track " .. tostring(args.track_index))
  end
  insert_midi_note(item, args.pitch, args.velocity, args.note_start_sec, args.note_end_sec, args.channel)
  reaper.MIDI_Sort(reaper.GetActiveTake(item))
  return {}
end

-- compose: one call = new track + MIDI item + every note + render bounds +
-- render trigger, instead of the caller chaining track_add/midi_add_item/
-- midi_add_note (xN)/render_project separately. Reuses the same
-- insert_midi_note/set_render_bounds helpers those ops use, not a copy.
ops.compose_and_render = function(args)
  local idx = reaper.CountTracks(0)
  reaper.InsertTrackAtIndex(idx, true)
  local tr = get_track(idx)
  if args.track_name then reaper.GetSetMediaTrackInfo_String(tr, "P_NAME", args.track_name, true) end

  local notes = args.notes or {}
  local max_end = 0
  for _, n in ipairs(notes) do
    if n.end_sec and n.end_sec > max_end then max_end = n.end_sec end
  end
  local item_end = max_end + 0.5

  local item = reaper.CreateNewMIDIItemInProj(tr, 0, item_end, false)
  for _, n in ipairs(notes) do
    insert_midi_note(item, n.pitch, n.velocity or 64, n.start_sec, n.end_sec, n.channel)
  end
  reaper.MIDI_Sort(reaper.GetActiveTake(item))

  if args.output_path then
    reaper.GetSetProjectInfo_String(0, "RENDER_FILE", args.output_path, true)
  end
  set_render_bounds(0, item_end)
  reaper.Main_OnCommand(41824, 0) -- File: Render project, using the most recent render settings

  return { track_index = idx, render_end_sec = item_end }
end

-- markers / regions
ops.marker_add = function(args)
  local idx = reaper.AddProjectMarker(0, false, args.position_sec, 0, args.name or "", -1)
  return { index = idx }
end

ops.region_add = function(args)
  local idx = reaper.AddProjectMarker(0, true, args.start_sec, args.end_sec, args.name or "", -1)
  return { index = idx }
end

-- view / zoom
ops.view_zoom_to_selection = function(args)
  reaper.Main_OnCommand(40031, 0) -- View: Zoom to fit selected items (horiz)
  return {}
end

ops.view_scroll_to = function(args)
  reaper.SetEditCurPos(args.position_sec, false, false)
  reaper.Main_OnCommand(40150, 0) -- View: Move edit cursor into view
  return {}
end

ops.view_set_arrange_zoom = function(args)
  -- args.pixels_per_sec approximates horizontal zoom
  reaper.adjustZoom(args.pixels_per_sec, 0, true, -1)
  return {}
end

-- native/custom actions (transport, toggles like snap, ripple edit, etc.)
ops.action_run = function(args)
  reaper.Main_OnCommand(args.command_id, args.section or 0)
  return {}
end

ops.action_get_state = function(args)
  local state = reaper.GetToggleCommandStateEx(args.section or 0, args.command_id)
  return { state = state }
end

-- project
ops.project_save = function(args) reaper.Main_SaveProject(0, false) return {} end
ops.project_undo = function(args) reaper.Main_OnCommand(40029, 0) return {} end

ops.render_project = function(args)
  if args.output_path then
    reaper.GetSetProjectInfo_String(0, "RENDER_FILE", args.output_path, true)
  end
  if args.start_sec ~= nil and args.end_sec ~= nil then
    set_render_bounds(args.start_sec, args.end_sec)
  else
    -- Default to the full current project length rather than trusting
    -- whatever bounds mode/range REAPER's render dialog last had (which may
    -- not have been configured at all on a fresh install).
    set_render_bounds(0, reaper.GetProjectLength(0))
  end
  reaper.Main_OnCommand(41824, 0) -- File: Render project, using the most recent render settings
  return {}
end

----------------------------------------------------------------------------
-- File-based IPC, polled every defer() tick
----------------------------------------------------------------------------

local function log(msg)
  reaper.ShowConsoleMsg("[reaper_mcp] " .. tostring(msg) .. "\n")
end

-- status window state: tracks bridge activity, not a live "Claude is
-- attached" signal -- this is file-polling IPC, so the most honest thing we
-- can show is "the bridge script is running" and "a request was last seen
-- N seconds ago", not a persistent connection state.
local last_request_time = nil
local request_count = 0
local gfx_initialized = false

local function ensure_dirs()
  reaper.RecursiveCreateDirectory(REQUESTS_DIR, 0)
  reaper.RecursiveCreateDirectory(RESPONSES_DIR, 0)
end

local function read_file(path)
  local f = io.open(path, "rb")
  if not f then return nil end
  local content = f:read("*a")
  f:close()
  return content
end

-- Write via a temp file + rename so the Python side never reads a half-written file.
local function write_file_atomic(path, content)
  local tmp_path = path .. ".tmp"
  local f = io.open(tmp_path, "wb")
  if not f then return false end
  f:write(content)
  f:close()
  os.remove(path)
  return os.rename(tmp_path, path) ~= nil
end

local function handle_request(req)
  local handler = ops[req.op]
  if not handler then
    return { id = req.id, ok = false, error = "unknown op: " .. tostring(req.op) }
  end
  local ok, result = pcall(handler, req.args or {})
  if ok then
    return { id = req.id, ok = true, result = result }
  else
    return { id = req.id, ok = false, error = tostring(result) }
  end
end

local function process_one_request(filename)
  local req_path = REQUESTS_DIR .. SEP .. filename
  local raw = read_file(req_path)
  os.remove(req_path)
  if not raw then return end

  local req, decode_err = json.decode(raw)
  local resp
  if not req then
    resp = { ok = false, error = "bad json: " .. tostring(decode_err) }
  else
    resp = handle_request(req)
  end

  last_request_time = reaper.time_precise()
  request_count = request_count + 1

  -- filename convention: req_<id>.json -> resp_<id>.json
  local id_part = filename:match("^req_(.+)%.json$") or filename
  write_file_atomic(RESPONSES_DIR .. SEP .. "resp_" .. id_part .. ".json", json.encode(resp))
end

local function pump()
  ensure_dirs()
  write_file_atomic(HEARTBEAT_FILE, tostring(reaper.time_precise()))

  local i = 0
  while true do
    local filename = reaper.EnumerateFiles(REQUESTS_DIR, i)
    if not filename then break end
    if filename:match("^req_.+%.json$") then
      process_one_request(filename)
    end
    i = i + 1
  end
end

----------------------------------------------------------------------------
-- Status window (gfx), same defer loop as the IPC pump above
----------------------------------------------------------------------------

local STATUS_ACTIVE_WINDOW_SEC = 3.0

-- Default to docked (bit0=1, docker index 0) rather than floating. Which
-- physical docker "index 0" lands in depends on the user's REAPER docker
-- layout -- there's no reliable numeric ID for a specific screen corner, so
-- this is a starting point, not a guaranteed position. Drag it to your
-- preferred docker once; DOCK_STATE_KEY below remembers that choice.
--
-- Uses a "_v2" key deliberately: earlier versions defaulted to floating
-- (dock state 0) and saved that back to ExtState. `tonumber("0") or
-- DEFAULT` evaluates to 0, not DEFAULT, because 0 is truthy in Lua -- so a
-- stale "0" from before this fix would silently keep overriding the new
-- docked default forever. Switching keys resets that stale value once;
-- the empty-string check below prevents the same bug from recurring.
local DEFAULT_DOCK_STATE = 1
local DOCK_STATE_KEY = "gfx_dock_v2"

-- Window/font sizing. Roughly 2x REAPER's unset default gfx font, with the
-- window grown to fit the header, status lines, and tool category list below.
local WINDOW_W, WINDOW_H = 240, 170
local FONT_HEADER_SIZE = 20
local FONT_BODY_SIZE = 18
local FONT_LIST_SIZE = 15

-- The bridge itself has no on/off toggle -- once reaper_bridge.lua is
-- loaded (via __startup.lua), its defer() loop runs for as long as REAPER
-- is open, independent of whether this window is open. "Status: ON" reflects
-- that fact plainly rather than implying a control that doesn't exist.
-- "Active"/"Idle" below is a separate signal: recent request activity.

-- Static summary of available tool domains, kept in sync by hand with
-- tools/__init__.py -- not generated, since this is a fixed-size status
-- panel rather than a scrollable/interactive list.
local TOOL_CATEGORIES = {
  "Transport, Tracks, FX",
  "MIDI, Items, Compose",
  "Markers, View, Render",
  "Actions, Project",
}

local function draw_status_window()
  if not gfx_initialized then
    local raw_dock = reaper.GetExtState("reaper_mcp", DOCK_STATE_KEY)
    local saved_dock = (raw_dock ~= "" and tonumber(raw_dock)) or DEFAULT_DOCK_STATE
    gfx.init("reaper-mcp", WINDOW_W, WINDOW_H, saved_dock)
    gfx_initialized = true
  end

  local char = gfx.getchar()
  if char < 0 then
    -- user closed the window; the IPC pump keeps running regardless
    gfx.quit()
    gfx_initialized = false
    return
  end

  local dock = gfx.dock(-1)
  reaper.SetExtState("reaper_mcp", DOCK_STATE_KEY, tostring(dock), true)

  local active = last_request_time ~= nil
    and (reaper.time_precise() - last_request_time) < STATUS_ACTIVE_WINDOW_SEC

  gfx.set(0.15, 0.15, 0.15)
  gfx.rect(0, 0, gfx.w, gfx.h, 1)

  gfx.setfont(1, "Arial", FONT_HEADER_SIZE)
  gfx.set(1, 1, 1)
  gfx.x, gfx.y = 10, 6
  gfx.drawstr("reaper-mcp")

  gfx.setfont(1, "Arial", FONT_BODY_SIZE)
  gfx.set(0.6, 0.9, 1)
  gfx.x, gfx.y = 10, 32
  gfx.drawstr("Status: ON")

  if active then
    gfx.set(0.2, 0.85, 0.3)
  else
    gfx.set(0.65, 0.65, 0.65)
  end
  gfx.x, gfx.y = 10, 54
  local request_label = request_count == 1 and "request" or "requests"
  gfx.drawstr((active and "Active" or "Idle") .. " - " .. tostring(request_count) .. " " .. request_label)

  gfx.setfont(1, "Arial", FONT_LIST_SIZE)
  gfx.set(0.75, 0.75, 0.75)
  local list_y = 82
  for _, line in ipairs(TOOL_CATEGORIES) do
    gfx.x, gfx.y = 10, list_y
    gfx.drawstr(line)
    list_y = list_y + 20
  end

  gfx.update()
end

local function main_loop()
  local ok, err = pcall(pump)
  if not ok then log("error: " .. tostring(err)) end

  local gfx_ok, gfx_err = pcall(draw_status_window)
  if not gfx_ok then log("status window error: " .. tostring(gfx_err)) end

  reaper.defer(main_loop)
end

-- Deliberately not calling log() here: reaper.ShowConsoleMsg() forces
-- REAPER's ReaScript console window open, which is disruptive on every
-- single REAPER launch now that this runs via __startup.lua. The status
-- window is the intended way to confirm the bridge is running; log() is
-- reserved for real errors below (pump/gfx failures), where popping the
-- console open is actually useful.
main_loop()
