# AGENTS.md

Project brief for picking up work on `reaper-mcp` in a new conversation.

## What this is

An MCP server (`uv`-managed Python package) that lets an LLM client drive a
live, running REAPER DAW instance in real time: transport control, tracks,
FX/plugins, MIDI, media items, markers, view/zoom, rendering, project
state, native UI action control, and a one-call MIDI-compose-and-render
workflow, plus a `run_reascript` escape hatch for anything not covered by a
dedicated tool. See [README.md](README.md) for user-facing setup/usage and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full design writeup.

## Current status

Working end to end on `main`, verified repeatedly against a real running
REAPER instance across several PRs (initial server, UI integration/auto-start,
concurrency hardening + CI, compose_and_render). A "reaper-mcp" status
window is docked in REAPER's UI showing bridge activity.

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
- Request/response filenames are scoped per `BridgeClient` instance (random
  `client_id` + counter), not a bare shared counter, so two concurrent MCP
  processes (e.g. Claude Code and Claude Desktop both connected) don't
  collide on the same request ID.
- The bridge auto-starts via REAPER's native `__startup.lua` (installed by
  `installer.install_startup_hook()`) - no manual Actions-list step after
  first install.
- Tool design is deliberately curated (~40+ well-named tools grouped by
  domain), not a 1:1 wrapper of REAPER's hundreds of ReaScript functions,
  with `run_reascript(code)` as the pressure-release valve for anything
  uncovered, plus `compose_and_render` as a higher-level composite tool for
  the common "write notes, get audio" workflow.

## Key files

| File | Purpose |
|---|---|
| `lua/reaper_bridge.lua` | Runs inside REAPER; polls requests, dispatches to `reaper.*`, writes responses + heartbeat, draws the status window |
| `src/reaper_mcp/bridge_client.py` | Python side of the file-IPC protocol, per-instance `client_id` scoping |
| `src/reaper_mcp/discovery.py` | Finds REAPER installs, running REAPER processes/PIDs (`psutil`), checks bridge heartbeat |
| `src/reaper_mcp/installer.py` | Copies `reaper_bridge.lua` into detected REAPER `Scripts` dirs, wires `__startup.lua` |
| `src/reaper_mcp/app.py` | Shared `FastMCP` instance + bridge client |
| `src/reaper_mcp/tools/*.py` | One module per domain (transport, tracks, fx, midi, items, compose, markers, view, project, render, actions, status, raw) |
| `src/reaper_mcp/server.py` | Entrypoint; `--install-bridge` / `--status` CLI flags, else runs the MCP server over stdio |
| `build_and_install.bat` | Root-level setup script: `uv sync` -> install bridge + startup hook -> exit (does not run/hold open an MCP server) |
| `tests/` | `pytest` unit tests for discovery, the bridge client, and the installer (mocked bridge, no real REAPER needed) |

## Setup / test loop

```
build_and_install.bat             # uv sync, install bridge + startup hook, exits
uv run reaper-mcp --status        # diagnostics: REAPER PID found? bridge heartbeat fresh?
uv run pytest                     # unit tests, no REAPER required
```
Claude Code/Desktop spawns `uv run reaper-mcp` itself per `.mcp.json` - there
is no separate "run the server" step for normal use. In REAPER, the bridge
auto-runs via `__startup.lua` after the first install; manual fallback:
Actions -> Show action list -> Load ReaScript... -> `reaper_bridge.lua` -> Run.
Full walkthrough in [README.md](README.md#testing-it-end-to-end).

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
- **Why request IDs are per-instance-scoped, not a bare counter**: a bare
  `itertools.count(1)` per `BridgeClient` meant two separate `reaper-mcp`
  processes both writing `req_1.json` could read each other's responses.
  Each client now generates a random `client_id` and scopes its counter
  under it. See `bridge_client.py` and `docs/ARCHITECTURE.md`.
- **Why the status window defaults to docked via a `_v2` ExtState key**: an
  earlier version defaulted to floating and saved that back to `ExtState`;
  `tonumber("0") or DEFAULT` evaluates to `0` in Lua (0 is truthy), so a
  stale saved `"0"` silently kept overriding a later "default to docked"
  fix. Fixed the unset-vs-zero logic *and* migrated the key so existing
  stale values don't keep applying. Don't assume `x or default` is a safe
  "fall back if unset" pattern in Lua when `x` can legitimately be `0`.
- **Why `RENDER_FORMAT` is never set by this codebase**: it's a
  base64-encoded binary fourcc value, not a plain string (verified via
  research, not memory). `RENDER_BOUNDSFLAG`/`RENDER_STARTPOS`/`RENDER_ENDPOS`
  are plain numeric `GetSetProjectInfo` keys and are safe to set - used by
  `render_project` and `compose_and_render` to fix render time-range bugs.
  Audio codec/format stays whatever the user configured last via REAPER's
  own render dialog.
- **No interactive buttons in the status window**: Play/Stop buttons were
  added, then removed at the user's request - it's a status panel, not a
  control surface, and REAPER's own transport is always available. Don't
  re-add UI controls to this window without it being explicitly requested.
- **Render "overwrite" is opt-in, not automatic**: `render_project`/
  `compose_and_render` render to an existing `output_path` without asking
  only when `overwrite=true` is passed. An earlier version silently deleted
  any pre-existing file at `output_path` on every render call to dodge
  REAPER's blocking "overwrite?" dialog (another modal that freezes the
  bridge while open, same as the missing-render-settings dialog) - that was
  a standing, unbounded deletion mechanism with no confirmation, correctly
  flagged and reverted. Don't reintroduce silent file deletion as a
  workaround for a REAPER dialog; require explicit opt-in instead.

## Conventions for this repo

- No em dashes in prose - use a plain hyphen (`-`).
- Never put personal usernames (e.g. a local Windows username/path) in commit
  messages, PR titles, or PR descriptions.
- New commits, not amends, unless explicitly asked (except pre-push local
  cleanup of a commit that hasn't been shared yet).
- Before using any REAPER/extension API not already used elsewhere in this
  codebase, verify it actually exists (web search or official docs) rather
  than asserting it from memory - this project has been burned twice by
  confidently-wrong REAPER API assumptions (`js_ReaScriptAPI` sockets,
  initially-assumed `RENDER_FORMAT` as a plain string).

## Possible next steps (not started)

- Lint/type-checking (ruff/mypy) - deliberately deferred during the CI
  hardening round; only the pytest-based CI check exists so far.
- Track routing/sends, item fades/crossfades.
- Streaming/subscription-style tools for live transport position (currently
  poll-only via `transport_get_state`).
- Cross-platform verification (macOS/Linux path handling in `discovery.py`
  is written but untested against a real non-Windows REAPER install).
- MIDI CC automation / quantize beyond note insertion.
