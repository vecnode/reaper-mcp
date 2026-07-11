# reaper-mcp

An MCP server that lets Claude drive a **live, running REAPER DAW instance** -
transport control, tracks, FX/plugins, MIDI, media items, markers, view/zoom,
rendering, and project state - plus a `run_reascript` escape hatch for
anything not covered by a dedicated tool.

Unlike file-parsing REAPER integrations, this talks to REAPER while it's
running, and includes an engineer-style discovery layer that finds your
REAPER install(s), the running REAPER process (with PID), and whether the
bridge is actually reachable - so failures are diagnosable, not silent.

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
   build_and_run.bat
   ```
   This runs `uv sync`, copies `lua/reaper_bridge.lua` into your REAPER
   `Scripts` folder, wires it into REAPER's native `__startup.lua` (so it
   auto-runs on every future REAPER launch - see "Auto-start" below), and
   starts the MCP server over stdio.
2. If REAPER is **already open**, the startup hook won't retroactively start
   it this session - either fully quit and reopen REAPER, or load it once
   manually: **Actions -> Show action list -> New action -> Load ReaScript...**,
   select `reaper_bridge.lua`, then **Run** it. You should see
   `[reaper_mcp] reaper_bridge starting, watching ...` in REAPER's console
   (Extensions -> ReaScript console), and a small "MCP" status window should
   appear (see "Status window" below).
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

## Testing it end to end

1. **Open REAPER** and load+run `reaper_bridge.lua` as described above (step 2
   in Setup). Confirm the console printed the "watching ..." line.
2. **Confirm the bridge is actually reachable**, independent of the MCP
   server, with the CLI diagnostics command:
   ```
   uv run reaper-mcp --status
   ```
   With REAPER open and the bridge script running, `bridge_reachable` should
   be `true` and `running_processes` should list REAPER's PID. If it's
   `false`, the bridge script either isn't loaded/running in REAPER, or
   REAPER's resource path doesn't match what was detected - check `bridge_dir`
   in the output against the folder the `.lua` file actually got copied to.
3. **Run the batch file** if you haven't already:
   ```
   build_and_run.bat
   ```
   It's stdio-based, so once started it just waits for an MCP client - that's
   expected, not a hang.
4. **Wire it into Claude** using the `.mcp.json`/Claude Desktop config snippet
   above, then restart Claude Code/Desktop.
5. **Exercise it from Claude**: ask it to call `reaper_status` first (sanity
   check), then something with a visible effect in REAPER, e.g. "add a track
   named Test" (`track_add`) or "play" (`transport_play`), and confirm REAPER
   actually reacts. `track_list` is a good read-only check if you want to
   confirm state without changing anything.

If a tool call errors with "bridge heartbeat not found or stale", the bridge
script isn't currently running in REAPER (REAPER was closed, the script was
stopped, or it crashed) - reload/rerun it via the Actions list, or confirm
the `__startup.lua` hook is in place (see "Auto-start" below).

## Auto-start (no more manual "run it every session")

`build_and_run.bat` (and `uv run reaper-mcp --install-bridge`) wire the
bridge into REAPER's native `__startup.lua` file - a script REAPER runs
automatically at launch, no extension required. This is done idempotently
and non-destructively: our addition lives inside marker comments
(`-- reaper-mcp:start` / `-- reaper-mcp:end`) inside `__startup.lua`, so any
of your own startup script content in that file is preserved and only our
block gets updated on reinstall. Takes effect on REAPER's *next* launch -
fully quit and reopen REAPER to see it, not just close/reopen a project.

## Status window

Once the bridge is running, a small "MCP" window appears in REAPER (dockable
- drag it into REAPER's docker like any other panel, and it remembers
whether you left it docked or floating across restarts). It shows:
- Green **"Bridge: Active"** if a request was processed in the last ~3 seconds
- Gray **"Bridge: Idle"** otherwise, plus a running request count

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
| Markers | `marker_add`, `region_add` |
| View | `view_zoom_to_selection`, `view_scroll_to`, `view_set_arrange_zoom` |
| Actions | `action_run(command_id)`, `action_get_toggle_state(command_id)` - drive any native UI toggle (snap, ripple edit, etc.) or custom action; look up `command_id` via REAPER's Actions list ("Copy selected action ID") |
| Project | `project_save`, `project_undo` |
| Render | `render_project` |
| Escape hatch | `run_reascript(code)` - arbitrary ReaScript Lua |

## Development

```
uv sync --group dev
uv run pytest
```

## License

Licensed under the MIT License.
