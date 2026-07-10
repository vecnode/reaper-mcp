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
    ├── installer.py     copies lua/reaper_bridge.lua into REAPER's Scripts dir
    └── tools/           one module per domain, each a thin @mcp.tool() wrapper
        │  file IPC: <bridge_dir>/requests/req_<id>.json -> .../responses/resp_<id>.json
        │  bridge_dir = <REAPER resource path>/Scripts/reaper_mcp_bridge
        ▼
lua/reaper_bridge.lua -- runs inside REAPER via reaper.defer()
    - every defer() tick: touches heartbeat.txt, scans requests/ for new
      req_*.json files, dispatches each via a lookup table (op name -> handler
      function -> reaper.* API calls), writes the JSON result to responses/
    - includes a run_reascript op that loads+executes arbitrary Lua for the
      run_reascript MCP tool escape hatch
        │  reaper.* ReaScript API
        ▼
REAPER DAW process
```

## Why file-based IPC, and why it's not the file-IPC we set out to avoid

The original design called for a persistent TCP socket bridge to avoid the
poll-interval latency and race conditions of `total-reaper-mcp`'s file-based
IPC. That called for a socket API inside vanilla ReaScript Lua; the plan
assumed the `js_ReaScriptAPI` extension provided one (`JS_Socket_*`
functions). **That was wrong** — verified against the actual js_ReaScriptAPI
function reference, no such functions exist. ReaScript Lua has no raw socket
API without a custom-compiled C++ extension, which isn't a reasonable install
step to ask of a user.

Given that constraint, this bridge still improves meaningfully on the
reference projects' file IPC: it polls on every `reaper.defer()` tick (REAPER
redraws the UI at high frequency, so this is a ~16-33ms round trip) rather
than on a coarse timer, and both sides write via a temp-file-then-rename so a
reader never observes a half-written file. It's slower than a real socket
would have been, but it needs zero extra REAPER extensions, which is a
reasonable trade for setup simplicity — the honest option to present after
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

## Bridge directory resolution

Both sides need to agree on the same folder without explicit configuration:

- The Lua script asks REAPER directly: `reaper.GetResourcePath() ..
  "/Scripts/reaper_mcp_bridge"` — this is always correct for whichever REAPER
  instance is actually running the script.
- The Python side can't ask a running REAPER process the same question, so it
  mirrors the OS-specific known-path logic in `discovery.find_reaper_installs()`
  (same paths listed above) and uses the first match. Set
  `REAPER_MCP_BRIDGE_DIR` to override this on either side if REAPER's resource
  path is nonstandard.
