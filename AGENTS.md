# AGENTS.md

Project brief for picking up work on `dawmcp` in a new conversation.

## What this is

An MCP server (Rust, Cargo workspace) that lets an LLM client drive a live,
running DAW instance in real time: transport control, tracks, FX, and
rendering, through a DAW-agnostic tool surface with a per-DAW adapter behind
it. REAPER is the first adapter, with tool parity for transport, tracks, FX,
MIDI, media items, markers, view/zoom, rendering, project state, native UI
action control, `compose_and_render`, and a `run_reascript` escape hatch.
Audacity is the next planned adapter. See [README.md](README.md) for
user-facing setup/usage and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for
the full design writeup (the latter still describes the REAPER bridge
protocol specifically; it predates the multi-DAW trait split and hasn't been
fully updated for it yet).

## Current status

Rust rewrite complete for REAPER: full tool parity with the original Python
implementation, verified against a real running REAPER instance (live
`--install-bridge` run, live tool round-trip over stdio). The Python
implementation (`reaper-mcp`) has been removed from the repo; `git log` has
its history if needed. `dawmcp-audacity` exists as a scaffold: the
`mod-script-pipe` wire protocol is implemented and verified against
Audacity's own reference client source, but almost every `DawBackend`
method returns `DawError::Unsupported` pending verification of exact
command parameter names (see that crate's `backend.rs` doc comment for
specifics), and it is **not yet wired into `dawmcp-server`'s binary** - no
backend-selection CLI flag exists yet; `main.rs` always constructs
`ReaperBackend`.

## Architecture (short version)

```
Claude (MCP client) --stdio--> dawmcp-server (rmcp) --> DawBackend trait --> adapter
                                                                               |
                                                                  dawmcp-reaper: file IPC --> reaper_bridge.lua (inside REAPER) --> reaper.* API
                                                                  dawmcp-audacity (planned): mod-script-pipe named pipes
```

- `dawmcp-core` defines DAW-agnostic traits (`Transport`, `Tracks`, `Fx`,
  `Render`, `Project`, `Status`, aggregated into `DawBackend`) plus shared
  types (`TrackInfo`, `TransportState`, `FxInfo`, `RenderRequest`,
  `DawError`). An adapter implements `DawBackend` once; `dawmcp-server`
  exposes the same MCP tools over whichever backend is active.
- DAW-specific concepts that don't generalize (REAPER's `run_reascript`
  escape hatch, MIDI note editing, markers/regions, view/zoom, native action
  control, the `compose_and_render` composite tool) live as extra inherent
  methods on the adapter (`dawmcp-reaper/src/extra.rs`), not on `DawBackend`.
  `dawmcp-server` calls these directly against the concrete `ReaperBackend`
  type, alongside the generic `Arc<dyn DawBackend>` tools. Once a second
  adapter exists, this needs to become adapter-aware/conditionally
  registered rather than always present - flagged in `main.rs`.
- The REAPER adapter (`dawmcp-reaper`) talks to REAPER exactly like the
  Python implementation did: `dawmcp-server` writes one JSON request file
  per tool call into a bridge directory; `lua/reaper_bridge.lua` (unchanged
  by the Rust port) picks it up on its next `reaper.defer()` tick (REAPER's
  UI frame rate, ~16-33ms round trip) and writes a JSON response back. No
  REAPER extensions required.
- Bridge directory: `<REAPER resource path>/Scripts/reaper_mcp_bridge/`
  (`requests/`, `responses/`, `heartbeat.txt`). Override with
  `REAPER_MCP_BRIDGE_DIR` if REAPER's resource path is nonstandard.
- Request/response filenames are scoped per `BridgeClient` instance (random
  `client_id` + counter), not a bare shared counter, so two concurrent MCP
  processes (e.g. Claude Code and Claude Desktop both connected) don't
  collide on the same request ID.
- Running `dawmcp` with no flags auto-installs the bridge for every detected
  DAW (currently just REAPER) before serving - `install_bridge()` runs on
  every startup, idempotently, logging to stderr (stdout is the MCP
  JSON-RPC stream once `serve(stdio())` starts). `--no-install` skips this;
  `--install-bridge`/`--status` run that step (or discovery) standalone and
  exit without starting the server.
- Tool design is deliberately curated (~40 well-named tools grouped by
  domain), not a 1:1 wrapper of REAPER's hundreds of ReaScript functions,
  with `run_reascript(code)` as the pressure-release valve for anything
  uncovered, plus `compose_and_render` as a higher-level composite tool for
  the common "write notes, get audio" workflow.

## Key files

| File | Purpose |
|---|---|
| `Cargo.toml` | Workspace root: members, shared dependency versions |
| `lua/reaper_bridge.lua` | Runs inside REAPER; polls requests, dispatches to `reaper.*`, writes responses + heartbeat, draws the status window. Unchanged by the Rust rewrite. |
| `crates/dawmcp-core/src/traits.rs` | DAW-agnostic `Transport`/`Tracks`/`Fx`/`Render`/`Project`/`Status`/`DawBackend` traits |
| `crates/dawmcp-core/src/types.rs` | Shared types: `TrackInfo`, `TransportState`, `FxInfo`, `RenderRequest`, `TrackIndex` |
| `crates/dawmcp-core/src/error.rs` | `DawError`/`DawResult` |
| `crates/dawmcp-reaper/src/bridge_client.rs` | File-IPC protocol client, ported from `bridge_client.py`; per-instance `client_id` scoping |
| `crates/dawmcp-reaper/src/discovery.rs` | Finds REAPER installs, running REAPER processes (`sysinfo`), checks bridge heartbeat; ported from `discovery.py` |
| `crates/dawmcp-reaper/src/installer.rs` | Copies `reaper_bridge.lua` + `reaper_project/default.RPP` into the REAPER install, wires both into `__startup.lua`; ported from `installer.py` |
| `crates/dawmcp-reaper/src/backend.rs` | `DawBackend` trait impl for REAPER, one bridge op per method |
| `crates/dawmcp-reaper/src/extra.rs` | REAPER-only ops with no cross-DAW equivalent (MIDI, items, markers, view, actions, compose, run_reascript) |
| `crates/dawmcp-server/src/main.rs` | MCP tool definitions (`rmcp` `#[tool]` macros), CLI flag handling, entrypoint |
| `reaper_project/default.RPP` | Blank, track-less project checked into the repo, generated by REAPER itself (not hand-authored); auto-opened via the startup hook |

## Setup / test loop

```
cargo build --workspace           # build everything
cargo test --workspace            # unit tests, no REAPER required
./target/debug/dawmcp --status    # diagnostics: REAPER installs found? bridge heartbeat fresh?
./target/debug/dawmcp             # auto-installs bridge for detected DAWs, then runs the MCP server over stdio
```
Claude Code/Desktop spawns the `dawmcp` binary itself per `.mcp.json` - there
is no separate "run the server" step for normal use. In REAPER, the bridge
auto-runs via `__startup.lua` after the first `dawmcp` launch (which
auto-installs it); manual fallback: Actions -> Show action list -> Load
ReaScript... -> `reaper_bridge.lua` -> Run. Full walkthrough in
[README.md](README.md#setup).

## Decisions worth knowing before changing the architecture

- **Why Rust and not staying in Python**: not a raw-speed argument - the
  actual latency ceiling is REAPER's own `defer()` tick rate (~16-33ms,
  UI-frame-locked) and the file-IPC round trip, not the language runtime.
  The reasons were single-binary distribution (no Python/`uv` runtime to
  install) and future-proofing for lower-level DAW protocols (sockets/shared
  memory) if a DAW ever needs one. The official Rust MCP SDK (`rmcp`) is
  real and used here, though less battle-tested than the Python SDK the
  original implementation used.
- **Why a `DawBackend` trait instead of one REAPER-specific server**: the
  project's scope expanded from REAPER-only to "MCP for DAWs generally"
  (Audacity next). DAW-specific concepts that don't generalize live as extra
  methods on the concrete adapter type, not forced into the shared trait.
- **Why file IPC and not a TCP socket** (REAPER adapter specifically): the
  original plan called for a persistent socket via the `js_ReaScriptAPI`
  extension's `JS_Socket_*` functions. That was a mistaken assumption -
  verified against the real API docs, no such functions exist. Vanilla
  ReaScript Lua has no socket API without a custom-compiled C++ extension,
  which isn't reasonable to ask a user to install. File IPC polled every
  `defer()` tick was the honest fallback: no extra dependency, at the cost
  of ~1-frame latency instead of true socket immediacy. Don't re-introduce a
  socket-based design without verifying the underlying REAPER/extension API
  actually exists first.
- **Why curated tools instead of full ReaScript coverage**: differentiator
  from `total-reaper-mcp` (600+ auto-generated tools, unwieldy for tool
  selection). `run_reascript` covers the long tail instead.
- **Why request IDs are per-instance-scoped, not a bare counter**: a bare
  incrementing counter per `BridgeClient` meant two separate `dawmcp`
  processes both writing `req_1.json` could read each other's responses.
  Each client now generates a random `client_id` and scopes its counter
  under it. See `bridge_client.rs` and `docs/ARCHITECTURE.md`.
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
- **Default project auto-opens via `Main_openProject` in `__startup.lua`,
  not a `reaper.ini` preference**: REAPER does have a real "Open project(s)
  on startup" General preference, but no `reaper.ini` key for it was found
  verifiable enough to set programmatically (Cockos forum threads that
  likely document it are behind a bot-check this environment can't get
  through). Rather than guess at a third REAPER internal after
  `js_ReaScriptAPI` sockets and `RENDER_FORMAT`, the startup hook just calls
  the already-verified `reaper.Main_openProject()` directly. The project
  file itself (`reaper_project/default.RPP`) was generated by REAPER via a
  one-time manual Save As and copied byte-for-byte, not hand-authored - the
  RPP format has binary-ish details (base64 `RENDER_CFG`/`RECORD_CFG` blobs,
  packed `METRONOME PATTERN` ints) not safe to freehand without live
  validation.
- **`track_index: -1` means the master track**, everywhere a track-taking op
  accepts `track_index` (`fx_add`, `fx_list`, `track_set_volume_db`, etc.) -
  `get_track()` in `reaper_bridge.lua` resolves it to
  `reaper.GetMasterTrack(0)`. Added originally to support the master safety
  limiter (`track_add`/`compose_and_render` auto-add REAPER's built-in
  `ReaLimit` to the master bus by default, idempotent, opt-out via
  `auto_limiter=false`), but deliberately generalized to every track op
  rather than a limiter-only special case, since it's broadly useful (e.g.
  inspecting/adjusting master volume the same way as any track). Carried
  into `dawmcp_core::TrackIndex` as the same convention.
- **Repo root resolution for the installer uses `env!("CARGO_MANIFEST_DIR")`
  at compile time**, not a runtime `current_exe()`-relative lookup - stable
  regardless of where the built binary is invoked from, since
  `dawmcp-reaper`'s manifest path is always two levels under repo root in
  this workspace.
- **`dawmcp` auto-installs on every normal startup, not just via
  `--install-bridge`**: the Python version required a separate
  `build_and_install.bat` step before first use. Pointing an MCP client at
  the `dawmcp` binary is now enough on its own. Install output goes to
  stderr, not stdout, since stdout is the MCP JSON-RPC stream once the
  server starts serving.

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
  initially-assumed `RENDER_FORMAT` as a plain string). The same applies to
  Rust crate APIs (e.g. `rmcp`) - verify against the actual crate source/docs,
  not training-data memory, since these move fast.

## Possible next steps

- Finish the Audacity adapter (`dawmcp-audacity` - scaffold only right
  now): the pipe protocol (named pipes on Windows, FIFOs on Linux/Mac,
  plain text command/response) is implemented and verified against
  Audacity's official reference client source. Still needed, each requiring
  its own doc/source verification pass before implementing (don't assume
  parameter names from memory):
  - Exact parameter keys for `SetTrackAudio`/`SetTrackStatus`/`SetTrackVisuals`
    (mute/solo/gain/pan/rename/color).
  - `GetInfo: Type=Tracks` response format, to implement `track_list` and to
    resolve the new track index after `NewMonoTrack` for `track_add`.
  - `SelectTracks`'s exact index parameter name, needed before any
    track-index-targeted command can work (Audacity's commands act on
    "selected"/"focused" tracks, not a track index directly).
  - `Export2`'s parameter names, for `render_project`.
  - Project save/undo command names.
  - Wiring the finished backend into `dawmcp-server`'s binary (currently
    hardcoded to `ReaperBackend` - needs a backend-selection mechanism,
    e.g. a CLI flag or auto-detection of which DAW is running).
- `docs/ARCHITECTURE.md` still describes the REAPER-only Python-era design
  in places; needs a pass for the trait/adapter split.
- Lint/type-checking (`cargo clippy`) - not yet wired into CI.
- Track routing/sends, item fades/crossfades.
- Streaming/subscription-style tools for live transport position (currently
  poll-only via `transport_get_state`).
- Cross-platform verification (macOS/Linux path handling in
  `dawmcp-reaper/src/discovery.rs` is written but untested against a real
  non-Windows REAPER install).
- MIDI CC automation / quantize beyond note insertion.
