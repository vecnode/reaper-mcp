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
    ├── installer.py     copies lua/reaper_bridge.lua + reaper_project/default.RPP
    │                     into the REAPER install, wires both into __startup.lua
    └── tools/           one module per domain, each a thin @mcp.tool() wrapper
        │  file IPC: <bridge_dir>/requests/req_<id>.json -> .../responses/resp_<id>.json
        │  bridge_dir = <REAPER resource path>/Scripts/reaper_mcp_bridge
        ▼
lua/reaper_bridge.lua -- runs inside REAPER via reaper.defer(), auto-loaded
by __startup.lua on REAPER launch
    - every defer() tick: touches heartbeat.txt, scans requests/ for new
      req_*.json files, dispatches each via a lookup table (op name -> handler
      function -> reaper.* API calls), writes the JSON result to responses/
    - includes a run_reascript op (arbitrary Lua escape hatch),
      action_run/action_get_state ops (native UI toggles like snap, ripple
      edit, or any custom action, by command ID), and compose_and_render
      (new track + MIDI notes + render, one call)
    - same defer() tick also draws a small "reaper-mcp" gfx status window (see below)
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

`reaper_bridge.lua` also draws a small "reaper-mcp" indicator window using
REAPER's built-in `gfx` Lua library (no extension), in the *same*
`reaper.defer()` loop as the IPC pump - not a separate script, since two
independently-loaded ReaScripts run as separate Lua VMs with no shared
state, and the window needs to see the pump's request activity directly.

- `last_request_time` / `request_count` locals are updated in
  `process_one_request` and read by `draw_status_window`.
- The window's docked/floating position persists across REAPER restarts via
  `reaper.SetExtState("reaper_mcp", "gfx_dock_v2", ...)` / `GetExtState`,
  read back on `gfx.init`, defaulting to **docked** (not floating) on
  first-ever run. The `_v2` key exists because an earlier version defaulted
  to floating and saved that back to `ExtState`; `tonumber("0") or DEFAULT`
  evaluates to `0` in Lua (0 is truthy, only `nil`/`false` are falsy), so a
  stale saved `"0"` silently overrode the new docked default under the old
  key - the key rename plus an explicit empty-string check fixed both the
  stale-state issue and the root-cause logic bug.
- Closing the window (`gfx.getchar() < 0`) calls `gfx.quit()` and stops
  drawing, but the IPC pump keeps running in the same `defer()` loop
  regardless - REAPER control never depends on the status window being open.
- Below the status lines, a static (hand-maintained, not generated) list of
  tool category groupings gives a quick at-a-glance reference of what's
  available without needing REAPER's own Actions list.

**Deliberately conservative about what "connected" means**: this is
file-polling IPC, not a persistent socket, so there is no actual live
connection state to report. "Status: ON" reflects that the bridge has no
on/off toggle (it runs for as long as REAPER is open, once loaded), and
separately "Active"/"Idle" reflects recent request activity - never "Claude:
Connected", since an MCP client only reaches the bridge at all when actively
calling a tool.

No interactive buttons: an earlier iteration added Play/Stop buttons to the
window, but they were removed as unnecessary - this is a status panel, not a
control surface, and REAPER's own transport controls are always available.

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
- Called automatically from `install_bridge()`, so `build_and_install.bat`
  alone is sufficient; no separate flag or manual step. Takes effect on
  REAPER's *next* launch, not the currently-running instance.
  `build_and_install.bat` only runs setup (sync deps, install bridge +
  startup hook) and exits - it never starts/holds open an MCP server itself,
  since Claude Code/Desktop spawns `uv run reaper-mcp` on its own per
  `.mcp.json`.

The same block also opens the bundled default project (see next section)
via a second `pcall`-wrapped line, so a missing/failed default project open
can't break the bridge `dofile` line either - each line fails independently.

## Default project (`reaper_project/default.RPP`)

Committed to the repo, generated by REAPER itself rather than hand-authored
(the RPP format has non-obvious binary-ish details - base64-encoded
`RENDER_CFG`/`RECORD_CFG` blobs, packed `METRONOME PATTERN` integers - that
would be real risks to get subtly wrong writing from scratch with no way to
validate short of live-testing in REAPER; letting REAPER write it via a
one-time manual Save As and copying the result byte-for-byte guarantees
validity instead).

- `installer.default_project_source_path()` points at the repo file;
  `install_default_project()` copies it into
  `<resource_path>/reaper-mcp-default.RPP` - a dedicated filename (not
  overwriting any of the user's own projects), called from `install_bridge()`
  alongside the bridge script and startup hook.
- Unlike the bridge script (essential - `bridge_source_path()` raises if
  missing, and that should propagate), a missing default project is a soft
  skip: `install_default_project()` catches `FileNotFoundError` and returns
  a status message instead of failing `install_bridge()` entirely. The
  default project is a convenience, not a requirement for the bridge to work.
- The `__startup.lua` block calls
  `reaper.Main_openProject(reaper.GetResourcePath() .. "/reaper-mcp-default.RPP")`,
  `pcall`-wrapped, after the bridge `dofile` line - verified via research to
  be a real ReaScript function (not asserted from memory).
- No `reaper.ini` preference is read or written anywhere for this. Multiple
  searches (including for REAPER's actual "Open project(s) on startup"
  General preference, which is real but has no confirmed underlying
  `reaper.ini` key - Cockos forum threads that likely have it are behind a
  bot-check this environment can't get through) turned up nothing verifiable
  enough to guess at, given this project has already been burned twice by
  confidently-wrong REAPER internals (`js_ReaScriptAPI` sockets,
  `RENDER_FORMAT` as a plain string). Driving the open via our own
  already-verified `__startup.lua` + `Main_openProject` sidesteps needing to
  understand REAPER's own preference storage at all.

## Master safety limiter

`ops.track_add` and `ops.compose_and_render` both call a shared
`ensure_master_limiter()` by default (opt-out via `args.auto_limiter ~=
false`, mirroring the `auto_instrument` convention): it checks the master
track's FX chain for an FX whose name contains `"ReaLimit"` (REAPER's
built-in true-peak brickwall limiter, verified real and specifically
recommended for master-bus safety limiting), adding one via the same
`TrackFX_AddByName(master, "ReaLimit", false, -1)` pattern already proven
working for `ReaSynth`, only if none is present yet - idempotent, so
creating N tracks never stacks N limiters.

This required generalizing `get_track(idx)`: `idx == -1` now resolves to
`reaper.GetMasterTrack(0)` instead of `reaper.GetTrack(0, idx)` (which would
just fail for a negative index). This is deliberately not special-cased to
only the limiter helper - every track-taking op (`fx_add`, `fx_list`,
`track_set_volume_db`, `track_set_pan`, etc.) now works against the master
bus for free by passing `track_index: -1`, which is generally useful beyond
this one feature. `track_list` is unaffected, since it intentionally only
enumerates regular tracks via `reaper.CountTracks`/`GetTrack`, never the
master, and that's still correct.

## Composing and rendering (`compose_and_render`)

`ops.midi_add_item`/`ops.midi_add_note` are real Lua ops (not
`run_reascript`-templated strings like an earlier version - promoted for
consistency with every other op in this file, and so `compose_and_render`
below can reuse their exact insertion logic via a shared local
`insert_midi_note` helper instead of duplicating it).

`ops.compose_and_render` collapses `track_add` + `midi_add_item` +
`midi_add_note` (xN) + `render_project` into one call: it creates a track,
one MIDI item spanning `[0, last_note.end_sec + 0.5]`, inserts every note,
sets render bounds to match that exact range, and triggers the render.

Both `compose_and_render` and the hardened `render_project` use a shared
`set_render_bounds(start_sec, end_sec)` helper that sets
`RENDER_BOUNDSFLAG=0` (custom time range), `RENDER_STARTPOS`, and
`RENDER_ENDPOS` via `reaper.GetSetProjectInfo` - verified via REAPER API
research (not asserted from memory) to be plain numeric keys. When
`render_project` isn't given an explicit range, it defaults to `0` through
`reaper.GetProjectLength(0)` rather than trusting whatever bounds mode/range
REAPER's render dialog last had configured, which could otherwise render the
wrong length (or fail) on a fresh REAPER install with no prior manual render.

**`RENDER_FORMAT` (codec/bitrate) is deliberately never set anywhere in this
file.** Research turned up that it's a base64-encoded binary fourcc value,
not a plain string - setting it without a verified reference encoding risks
silently corrupting the render configuration. Audio format stays whatever
the user last configured via REAPER's own File -> Render dialog; only time
range and output path are controlled programmatically.
