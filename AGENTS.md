# AGENTS.md

Project brief for picking up work on `reaper-mcp` in a new conversation.

## What this is

An MCP server (`uv`-managed Python package) that lets an LLM client drive a
live, running REAPER DAW instance in real time: transport control, tracks,
FX/plugins, MIDI, media items, markers, view/zoom, rendering, and project
state, plus a `run_reascript` escape hatch for anything not covered by a
dedicated tool. See [README.md](README.md) for user-facing setup/usage and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full design writeup.

## Current status

Working end to end, merged to `main` via [PR #1](https://github.com/vecnode/reaper-mcp/pull/1).
Verified against a real running REAPER instance: bridge loads, heartbeat is
detected, tool calls round-trip successfully.

## Architecture (short version)

```
Claude (MCP client) --stdio--> Python MCP server (src/reaper_mcp/) --file IPC--> reaper_bridge.lua (inside REAPER) --> reaper.* API
```

- The Python server writes one JSON request file per tool call into a bridge
  directory; `lua/reaper_bridge.lua` picks it up on its next `reaper.defer()`
  tick (REAPER's UI frame rate, ~16-33ms round trip) and writes a JSON
  response back. No REAPER extensions required.
- Bridge directory: `<REAPER resource path>/Scripts/reaper_mcp_bridge/`
  (`requests/`, `responses/`, `heartbeat.txt`). Override with
  `REAPER_MCP_BRIDGE_DIR` if REAPER's resource path is nonstandard.
- Tool design is deliberately curated (~40 well-named tools grouped by
  domain), not a 1:1 wrapper of REAPER's hundreds of ReaScript functions,
  with `run_reascript(code)` as the pressure-release valve for anything
  uncovered.

## Key files

| File | Purpose |
|---|---|
| `lua/reaper_bridge.lua` | Runs inside REAPER; polls requests, dispatches to `reaper.*`, writes responses + heartbeat |
| `src/reaper_mcp/bridge_client.py` | Python side of the file-IPC protocol |
| `src/reaper_mcp/discovery.py` | Finds REAPER installs, running REAPER processes/PIDs (`psutil`), checks bridge heartbeat |
| `src/reaper_mcp/installer.py` | Copies `reaper_bridge.lua` into detected REAPER `Scripts` dirs |
| `src/reaper_mcp/app.py` | Shared `FastMCP` instance + bridge client |
| `src/reaper_mcp/tools/*.py` | One module per domain (transport, tracks, fx, midi, items, markers, view, project, render, status, raw) |
| `src/reaper_mcp/server.py` | Entrypoint; `--install-bridge` / `--status` CLI flags, else runs the MCP server over stdio |
| `build_and_run.bat` | Root-level launcher: `uv sync` -> install/update bridge -> run server |
| `tests/` | `pytest` unit tests for discovery and the bridge client (mocked bridge, no real REAPER needed) |

## Setup / test loop

```
build_and_run.bat                 # uv sync, install bridge, run server
uv run reaper-mcp --status        # diagnostics: REAPER PID found? bridge heartbeat fresh?
uv run pytest                     # unit tests, no REAPER required
```
In REAPER: Actions -> Show action list -> Load ReaScript... -> `reaper_bridge.lua` -> Run
(optionally "Run on startup"). Full walkthrough in [README.md](README.md#testing-it-end-to-end).

## Decisions worth knowing before changing the architecture

- **Why file IPC and not a TCP socket**: the original plan called for a
  persistent socket via the `js_ReaScriptAPI` extension's `JS_Socket_*`
  functions. That was a mistaken assumption - verified against the real API
  docs, no such functions exist. Vanilla ReaScript Lua has no socket API
  without a custom-compiled C++ extension, which isn't reasonable to ask a
  user to install. File IPC polled every `defer()` tick was the honest
  fallback: no extra dependency, at the cost of ~1-frame latency instead of
  true socket immediacy. Don't re-introduce a socket-based design without
  verifying the underlying REAPER/extension API actually exists first.
- **Why curated tools instead of full ReaScript coverage**: differentiator
  from `total-reaper-mcp` (600+ auto-generated tools, unwieldy for tool
  selection). `run_reascript` covers the long tail instead.

## Conventions for this repo

- No em dashes in prose - use a plain hyphen (`-`).
- Never put personal usernames (e.g. a local Windows username/path) in commit
  messages, PR titles, or PR descriptions.
- New commits, not amends, unless explicitly asked (except pre-push local
  cleanup of a commit that hasn't been shared yet).

## Possible next steps (not started)

- Expand `tools/midi.py` beyond note insertion (CC automation, quantize).
- Track routing / sends, item fades/crossfades.
- Streaming/subscription-style tools for live transport position (currently
  poll-only via `transport_get_state`).
- Cross-platform verification (macOS/Linux path handling in `discovery.py`
  is written but untested against a real non-Windows REAPER install).
