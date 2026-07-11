# Architecture

```
Claude Code / Claude Desktop (MCP client)
        │  stdio (MCP protocol)
        ▼
Python MCP server -- src/reaper_mcp/  (uv-run, FastMCP)
    ├── server.py        entrypoint, CLI flags (--install-bridge, --status)
    ├── app.py           shared FastMCP instance + BridgeClient
    ├── bridge_client.py file-based IPC client, polls for response files
    ├── discovery.py     REAPER install scan, running-process/PID lookup (psutil), heartbeat check
    ├── installer.py     copies lua/reaper_bridge.lua into REAPER's Scripts dir,
    │                     wires it into REAPER's native __startup.lua
    └── tools/           one module per domain, each a thin @mcp.tool() wrapper
        │  file IPC: <bridge_dir>/requests/req_<id>.json -> .../responses/resp_<id>.json
        │  bridge_dir = <REAPER resource path>/Scripts/reaper_mcp_bridge
        ▼
lua/reaper_bridge.lua -- runs inside REAPER via reaper.defer(), auto-loaded
by __startup.lua on REAPER launch
    - every defer() tick: touches heartbeat.txt, scans requests/ for new
      req_*.json files, dispatches each via a lookup table (op name -> handler
      function -> reaper.* API calls), writes the JSON result to responses/
    - includes a run_reascript op (arbitrary Lua escape hatch) and
      action_run/action_get_state ops (native UI toggles like snap, ripple
      edit, or any custom action, by command ID)
    - same defer() tick also draws a small "MCP" gfx status window (see below)
        │  reaper.* ReaScript API
        ▼
REAPER DAW process
```

## Why file-based IPC, and why it's not the file-IPC we set out to avoid

The original design called for a persistent TCP socket bridge to avoid the
poll-interval latency and race conditions of `total-reaper-mcp`'s file-based
IPC. That called for a socket API inside vanilla ReaScript Lua; the plan
assumed the `js_ReaScriptAPI` extension provided one (`JS_Socket_*`
functions). **That was wrong** - verified against the actual js_ReaScriptAPI
function reference, no such functions exist. ReaScript Lua has no raw socket
API without a custom-compiled C++ extension, which isn't a reasonable install
step to ask of a user.

Given that constraint, this bridge still improves meaningfully on the
reference projects' file IPC: it polls on every `reaper.defer()` tick (REAPER
redraws the UI at high frequency, so this is a ~16-33ms round trip) rather
than on a coarse timer, and both sides write via a temp-file-then-rename so a
reader never observes a half-written file. It's slower than a real socket
would have been, but it needs zero extra REAPER extensions, which is a
reasonable trade for setup simplicity - the honest option to present after
the socket assumption turned out to be false, rather than re-introducing a
new unverified dependency to preserve the original latency target.

## Why a curated tool set instead of a 1:1 ReaScript wrapper

REAPER's ReaScript API has hundreds of functions. Exposing all of them as
flat MCP tools (as `total-reaper-mcp` does, 600+) makes tool selection hard
for the model and hard to keep documented. Instead:

- Common workflows (tracks, FX, transport, markers, view, rendering, MIDI,
  items) get well-named, well-typed tools in `tools/*.py`.
- `run_reascript(code)` is the pressure release valve: anything not covered
  by a dedicated tool can still be done by sending Lua straight to the
  bridge's `run_reascript` op, which `load()`s and executes it in REAPER's
  ReaScript environment.

## Discovery / diagnostics

`discovery.py` answers "is this actually working" without guessing:

- `find_running_reaper()` walks `psutil.process_iter()` for `reaper`/`reaper.exe`
  processes and reports PIDs.
- `find_reaper_installs()` checks OS-specific known locations
  (`%APPDATA%\REAPER`, `~/Library/Application Support/REAPER`, `~/.config/REAPER`,
  `Program Files\REAPER (x64)`, plus a `REAPER_RESOURCE_PATH` env override).
- `BridgeClient.is_alive()` checks the mtime of `heartbeat.txt` in the bridge
  directory (touched every `defer()` tick the Lua script is running) rather
  than just checking the directory exists, distinguishing "REAPER open but
  script not started/crashed" from "bridge actually live."

This backs both `uv run reaper-mcp --status` (human/CI use) and the
`reaper_status` MCP tool (so Claude can self-diagnose a broken bridge instead
of just surfacing a bare error).

## Request ID scoping (multiple concurrent clients)

REAPER only expects one bridge script running, but nothing stops two
separate `reaper-mcp` processes from connecting to it at once - e.g. Claude
Code and Claude Desktop both configured against the same REAPER instance.
Request/response filenames are `req_<id>.json` / `resp_<id>.json`; if `<id>`
were a bare per-process counter ("1", "2", ...), two processes would both
produce `req_1.json` and could read each other's responses. `BridgeClient`
generates a short random `client_id` per instance (`uuid.uuid4().hex[:8]`)
and scopes its counter under that (`req_<client_id>-<n>.json`), so concurrent
clients never collide. The Lua side needs no changes for this - it already
echoes back whatever `id` string it's given and derives the response
filename from the request filename verbatim.

## Bridge directory resolution

Both sides need to agree on the same folder without explicit configuration:

- The Lua script asks REAPER directly: `reaper.GetResourcePath() ..
  "/Scripts/reaper_mcp_bridge"` - this is always correct for whichever REAPER
  instance is actually running the script.
- The Python side can't ask a running REAPER process the same question, so it
  mirrors the OS-specific known-path logic in `discovery.find_reaper_installs()`
  (same paths listed above) and uses the first match. Set
  `REAPER_MCP_BRIDGE_DIR` to override this on either side if REAPER's resource
  path is nonstandard.

## Native UI control (`action_run` / `action_get_state`)

REAPER exposes both native UI toggles (snap to grid, ripple editing, etc.)
and custom/ReaScript actions through the same mechanism: a numeric command
ID, scoped to a section (0 = main). `ops.action_run` calls
`reaper.Main_OnCommand(command_id, section)` - for a toggle action, calling
it again flips the toggle, same as clicking the UI control it corresponds
to. `ops.action_get_state` calls `reaper.GetToggleCommandStateEx` to read
on/off state. Deliberately, no command IDs are hardcoded anywhere in this
codebase: REAPER assigns them per-install/version, and after getting burned
once by asserting an unverified REAPER API from memory (see above), the
correct move is to have the caller look them up live via REAPER's own
Actions list rather than trust a remembered number.

REAPER's API does not expose a way to inject new buttons into its *native*
timeline/arrange toolbar - that's simply not something ReaScript can do.
What "add buttons" actually means here is the status window below, which
draws its own UI via `gfx` and can have its own clickable regions.

## Status window

`reaper_bridge.lua` also draws a small "MCP" indicator window using
REAPER's built-in `gfx` Lua library (no extension), in the *same*
`reaper.defer()` loop as the IPC pump - not a separate script, since two
independently-loaded ReaScripts run as separate Lua VMs with no shared
state, and the window needs to see the pump's request activity directly.

- `last_request_time` / `request_count` locals are updated in
  `process_one_request` and read by `draw_status_window`.
- The window's docked/floating position persists across REAPER restarts via
  `reaper.SetExtState("reaper_mcp", "gfx_dock", ...)` / `GetExtState`,
  read back on `gfx.init`. First-ever run defaults to floating rather than
  guessing a specific dock slot ID.
- Closing the window (`gfx.getchar() < 0`) calls `gfx.quit()` and stops
  drawing, but the IPC pump keeps running in the same `defer()` loop
  regardless - REAPER control never depends on the status window being open.

**Deliberately conservative about what "connected" means**: this is
file-polling IPC, not a persistent socket, so there is no actual live
connection state to report. The window shows "Bridge: Active" (a request
was processed in the last ~3s) or "Bridge: Idle" (script running, no recent
activity) - never "Claude: Connected", since an MCP client only reaches the
bridge at all when actively calling a tool.

## Auto-start via `__startup.lua`

REAPER runs a file named exactly `__startup.lua` in the Scripts folder
automatically at launch - a native mechanism, no SWS or other extension
required (confirmed via research before relying on it, given the earlier
js_ReaScriptAPI mistake). `installer.install_startup_hook()` manages a
marker-delimited block (`-- reaper-mcp:start` / `-- reaper-mcp:end`) inside
that file:

- Block content is a `pcall`-wrapped `dofile(...)` pointing at the installed
  `reaper_bridge.lua`, so a syntax error in our bridge can't break the
  user's other startup scripts.
- If `__startup.lua` doesn't exist, it's created with just our block. If it
  exists with foreign content, our block is appended, preserving the rest.
  If it exists with our markers already present (from a previous install),
  only the content between them is replaced - never duplicated.
- Called automatically from `install_bridge()`, so `build_and_run.bat` alone
  is sufficient; no separate flag or manual step. Takes effect on REAPER's
  *next* launch, not the currently-running instance.
