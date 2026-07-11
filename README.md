# reaper-mcp

A comprehensive MCP for the Reaper Digital Audio Workstation (DAW).

It connects Claude to a live, running REAPER instance for direct control of
transport, tracks, FX/plugins, MIDI, media items, markers, view/zoom,
rendering, project state, and native UI actions, with a `run_reascript`
escape hatch for anything not covered by a dedicated tool. Unlike
integrations that only read project files, this operates on REAPER while
it's running, and includes a discovery layer that locates REAPER
installs, the running REAPER process (with PID), and bridge reachability,
so failures are diagnosable rather than silent.

## Architecture

```
Claude  <--stdio-->  reaper-mcp (Python, uv)  <--file-based IPC-->  reaper_bridge.lua (inside REAPER)  -->  reaper.* API
```

The Python server writes one JSON request file per call into a bridge
directory; `reaper_bridge.lua` picks it up on its next `reaper.defer()` tick
(REAPER's UI frame rate, so ~16-33ms round trips) and writes a JSON response
back. No REAPER extensions are required - this uses only the standard
`io`/`os` Lua libraries and the `reaper.*` API, both built into vanilla
ReaScript. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for details.

## Requirements

- [uv](https://docs.astral.sh/uv/) (Python package/venv manager)
- REAPER (tested on 6.x/7.x) - nothing else. No extensions to install.

## Setup

1. From this repo's root, run:
   ```
   build_and_install.bat
   ```
   This runs `uv sync`, copies `lua/reaper_bridge.lua` into your REAPER
   `Scripts` folder, and wires it into REAPER's native `__startup.lua` so it
   auto-runs on every future REAPER launch (idempotent, non-destructive -
   see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for how), then exits -
   it does not start or hold open an MCP server itself. Claude Code/Desktop
   launches `uv run reaper-mcp` on its own per `.mcp.json` (step 3), so
   nothing here needs to be left running.
2. If REAPER is **already open**, the startup hook won't retroactively start
   it this session - either fully quit and reopen REAPER, or load it once
   manually: **Actions -> Show action list -> New action -> Load ReaScript...**,
   select `reaper_bridge.lua`, then **Run** it. You should see the small
   "reaper-mcp" status window appear (see "Status window" below).
3. Point Claude Code (or Claude Desktop) at the server. Example
   `.mcp.json` / `claude_desktop_config.json` entry:
   ```json
   {
     "mcpServers": {
       "reaper": {
         "command": "uv",
         "args": ["--directory", "<path-to-this-repo>", "run", "reaper-mcp"]
       }
     }
   }
   ```

If a tool call errors with "bridge heartbeat not found or stale", the bridge
script isn't currently running in REAPER (REAPER was closed, the script was
stopped, or it crashed) - reload/rerun it via the Actions list, or reinstall
via `build_and_install.bat` and fully restart REAPER.

## Status window

Once the bridge is running, a small "reaper-mcp" window appears in REAPER,
**docked by default** (not floating). It shows:
- **"Status: ON"** - the bridge has no on/off toggle; once loaded it runs for
  as long as REAPER is open, independent of whether this window is open
- Green **"Active"** if a request was processed in the last ~3 seconds, gray
  **"Idle"** otherwise, plus a running request count
- A quick reference list of available tool domains (Transport/Tracks/FX,
  MIDI/Items/Compose, Markers/View/Render, Actions/Project)

The bridge doesn't try to guess a pixel-precise screen position - REAPER's
dockers are separate panel regions (like where the Mixer docks), not
overlays on the native toolbar, so there's no reliable way to place it
"on top of" a specific toolbar button. Drag it once to whichever docker
position you prefer; that choice is remembered across REAPER restarts.
The console no longer opens automatically on startup either (it only pops
up now if the bridge or status window hits a real error).

**Want REAPER itself to open in a specific layout** (no tracks, mixer
visible, video window docked right, etc.) **every launch?** That's a better
fit for REAPER's own built-in tools than for ReaScript: set a blank
project as your default template (Preferences -> Project -> "Default
project template"), arrange the windows the way you want once, then save
that arrangement as a Screenset (View -> Screensets) set to load on
startup. Those are native, reliable REAPER features - safer than us
scripting window geometry blind. If you want that screenset to also load
automatically via our `__startup.lua` hook, find its action/command ID in
REAPER's Actions list and use the `action_run` tool to trigger it from here.

Honest caveat: because this is file-polling IPC rather than a persistent
socket, the window can only reflect "the bridge script is running" and "a
request was last seen N seconds ago" - it is **not** a live "Claude is
connected right now" indicator, since an MCP client only reaches out when
actively calling a tool. Closing the window doesn't stop the bridge itself;
REAPER control keeps working either way.

## Tool overview

| Domain | Examples |
|---|---|
| Status | `reaper_status`, `install_bridge`, `reaper_info` |
| Transport | `transport_play/stop/pause/record/seek/set_tempo/get_state` |
| Tracks | `track_add/remove/rename/set_volume_db/set_pan/set_mute/set_solo/set_color/list` |
| FX | `fx_add/remove/set_enabled/list/set_param/get_param` |
| MIDI | `midi_add_item`, `midi_add_note` |
| Items | `item_split/move/glue_selected/render_in_place_selected` |
| Compose | `compose_and_render(output_path, notes, track_name)` - one call: new track + MIDI notes + render to audio (see below) |
| Markers | `marker_add`, `region_add` |
| View | `view_zoom_to_selection`, `view_scroll_to`, `view_set_arrange_zoom` |
| Actions | `action_run(command_id)`, `action_get_toggle_state(command_id)` - drive any native UI toggle (snap, ripple edit, etc.) or custom action; look up `command_id` via REAPER's Actions list ("Copy selected action ID") |
| Project | `project_save`, `project_undo` |
| Render | `render_project(output_path, start_sec, end_sec)` - explicit time bounds by default (see below), not whatever REAPER's render dialog last had configured |
| Escape hatch | `run_reascript(code)` - arbitrary ReaScript Lua |

### Composing and rendering audio

`compose_and_render` collapses the track/MIDI-item/notes/render sequence
into one call: give it a list of notes (`{pitch, start_sec, end_sec,
velocity?, channel?}`) and an `output_path`, and it creates the track, MIDI
item, every note, sets the render time range to exactly match the composed
notes, and renders. MIDI notes are silent without a virtual instrument on
the track, so by default it also adds REAPER's built-in ReaSynth to the new
track if it has no instrument yet - pass `auto_instrument=false` to skip
this if you're adding your own instrument/FX chain first.

`render_project` got the same time-range hardening as `compose_and_render` -
both now default to explicit bounds (0 to the content/project length)
instead of trusting whatever range REAPER's render dialog last had
configured, which previously meant a fresh REAPER install could render the
wrong length or fail outright.

**If `output_path` already exists**, both tools render to the next
available name by default (`tone_1.wav` -> `tone_1_2.wav` -> `tone_1_3.wav`,
etc.) - the actual path used is returned in the result. REAPER would
otherwise show a blocking "overwrite?" dialog, and since that's a modal
dialog, the bridge's own polling loop is frozen while it's open with no way
to detect or dismiss it once it appears - the auto-increment default avoids
the condition entirely rather than trying to work around it. Pass
`overwrite=true` to delete the existing file and render to the exact
requested path instead.

**Audio format (WAV, MP3, bit depth, etc.) is not set by either tool** -
REAPER's render-format setting is a base64-encoded binary value, not a
plain string, and isn't safe to set without a verified reference encoding.
Open REAPER's render dialog once (File -> Render) and pick your format;
after that, both tools will use it on every subsequent render.

## Development

```
uv sync --group dev
uv run pytest
```

## License

Licensed under the [MIT License](LICENSE).
