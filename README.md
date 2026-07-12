# dawmcp

[![CI](https://github.com/vecnode/dawmcp/actions/workflows/ci.yml/badge.svg)](https://github.com/vecnode/dawmcp/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](Cargo.toml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A comprehensive MCP for Digital Audio Workstations (DAWs).

It connects Claude to a live, running DAW instance - controlling transport,
tracks, FX, and rendering - through a DAW-agnostic tool surface, with a
per-DAW adapter behind it.

## Architecture

```
Claude  <--stdio-->  dawmcp (Rust, rmcp, --daw=reaper|audacity)  -->  DawBackend trait  -->  adapter
                                                                                              |
                                                                     dawmcp-reaper    (file IPC --> reaper_bridge.lua --> reaper.* API)
                                                                     dawmcp-audacity  (mod-script-pipe named pipes/FIFOs --> Audacity)
```

`dawmcp-core` defines DAW-agnostic traits (`Transport`, `Tracks`, `Fx`,
`Render`, `Project`, `Status`) so the same MCP tools work over any DAW that
implements them. DAW-specific concepts that don't generalize (REAPER's
`run_reascript` escape hatch, Audacity's lack of a persistent FX chain) live
only in that adapter, not in the shared core. `dawmcp-server` picks which
adapter to serve via `--daw=reaper` (default) or `--daw=audacity`; `.mcp.json`
registers both as separate MCP server entries so Claude can connect to
either (or both) at once.

The REAPER adapter (`crates/dawmcp-reaper`) talks to REAPER the same way the
original Python implementation did: the server writes one JSON request file
per call into a bridge directory, `lua/reaper_bridge.lua` picks it up on its
next `reaper.defer()` tick (REAPER's UI frame rate, ~16-33ms round trip) and
writes a JSON response back - no REAPER extensions required. See
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full design writeup and
why this is file IPC rather than a socket.

The Audacity adapter (`crates/dawmcp-audacity`) talks to Audacity's built-in
`mod-script-pipe` module (named pipes on Windows, FIFOs on Linux/Mac) - every
command it sends is verified against Audacity's own source, not assumed.
Audacity must have mod-script-pipe enabled once yourself (Edit > Preferences
> Modules) - dawmcp won't flip that on for you, since it's a
security-relevant setting Audacity's own docs warn about (any program can
then control Audacity with no notification).

## Status

The Rust workspace has full REAPER tool parity with the original Python
implementation, verified against a real running REAPER instance (the Python
package has been removed; `git log` has its history). The Audacity adapter
covers transport (play/stop/pause/record), track add/remove/rename/list,
per-track volume/pan/mute/solo, and render/export - also verified against a
real Audacity install's process/path discovery on this machine. Not yet
supported for Audacity (by design, not by omission - see
[AGENTS.md](AGENTS.md) for why): seeking/tempo/transport-state queries, FX,
and per-track color.

## Requirements

- [Rust](https://rustup.rs/) (stable toolchain, via `rustup`)
- REAPER (tested on 6.x/7.x) for the REAPER adapter, and/or Audacity (3.x)
  for the Audacity adapter - use either or both. REAPER needs no
  extensions. Audacity needs `mod-script-pipe` enabled once yourself:
  Edit > Preferences > Modules > set `mod-script-pipe` to "Enabled", then
  restart Audacity.

## Setup

1. Run [build.bat](build.bat) (double-click it, or `cargo build --release`
   yourself), which builds `dawmcp` and installs REAPER's bridge for every
   detected REAPER install.
2. Point Claude Code (or Claude Desktop) at it - `.mcp.json` in this repo
   already registers both:
   ```json
   {
     "mcpServers": {
       "reaper": {
         "command": "<path-to-this-repo>/target/release/dawmcp"
       },
       "audacity": {
         "command": "<path-to-this-repo>/target/release/dawmcp",
         "args": ["--daw=audacity"]
       }
     }
   }
   ```
   For REAPER: on every launch, `dawmcp` auto-detects and installs
   `lua/reaper_bridge.lua` (plus the default project and startup hook, see
   below) into every REAPER install it finds, idempotently, before serving.
   No separate setup script or manual Actions-list step needed. Pass
   `--no-install` to skip this and just serve.
3. If REAPER is **already open** when `dawmcp` first installs the startup
   hook, it won't retroactively start the bridge this session - either fully
   quit and reopen REAPER, or load it once manually: **Actions -> Show
   action list -> New action -> Load ReaScript...**, select
   `reaper_bridge.lua`, then **Run** it. You should see the small status
   window appear (see "Status window" below).

Useful standalone commands (no MCP client needed):
```
dawmcp --status                  # diagnostics for every detected DAW: installs found? reachable?
dawmcp --install-bridge          # install/update the REAPER bridge for every detected REAPER, then exit
dawmcp --daw=audacity            # serve Audacity's tools instead of REAPER's
```

If a REAPER tool call errors with "bridge heartbeat not found or stale",
the bridge script isn't currently running in REAPER (REAPER was closed, the
script was stopped, or it crashed) - reload/rerun it via the Actions list.
If an Audacity tool call errors with "could not open Audacity's
mod-script-pipe", either Audacity isn't running or `mod-script-pipe` isn't
enabled (see Requirements above). Call the `daw_status` tool for
diagnostics either way.

## Default project

[reaper_project/default.RPP](reaper_project/default.RPP) is a blank,
track-less project checked into this repo (generated by REAPER itself, not
hand-authored, so its file format is guaranteed valid). `dawmcp` copies it
into REAPER's resource path as `reaper-mcp-default.RPP` and adds a
`reaper.Main_openProject(...)` call to the same `__startup.lua` block that
installs the bridge, so REAPER opens this clean project automatically on
every launch instead of whatever it would otherwise default to.

## Status window

Once the bridge is running, a small status window appears in REAPER,
**docked by default** (not floating). It shows:
- **"Status: ON"** - the bridge has no on/off toggle; once loaded it runs for
  as long as REAPER is open, independent of whether this window is open
- Green **"Active"** if a request was processed in the last ~3 seconds, gray
  **"Idle"** otherwise, plus a running request count
- A quick reference list of available tool domains

## Tool overview

DAW-agnostic tools (`dawmcp-core`'s `DawBackend` trait), backed by whichever
adapter is active (`--daw=reaper`, the default, or `--daw=audacity`):

| Domain | Tools | Audacity support |
|---|---|---|
| Status | `daw_status` | yes |
| Transport | `transport_play/stop/pause/record/seek/set_tempo/get_state` | play/stop/pause/record only - no verified seek, tempo, or state query |
| Tracks | `track_add/remove/rename/set_volume_db/set_pan/set_mute/set_solo/set_color/list` - `-1` as `track_index` means the master bus on REAPER, everywhere a track-taking tool accepts one | all but `set_color` (Audacity's colour is a small preset enum, not RGB) |
| FX | `fx_add/remove/set_enabled/list/set_param/get_param` | not supported - Audacity has no persistent per-track FX chain like REAPER's |
| Render | `render_project(output_path, start_seconds, end_seconds, overwrite)` | yes, via `Export2` |
| Project | `project_save`, `project_undo` | yes (`Save`/`Undo` - see AGENTS.md for an unverified edge case on never-saved projects) |

REAPER-only tools (no cross-DAW equivalent, exposed via `dawmcp-reaper`'s
inherent methods rather than the shared trait):

| Domain | Tools |
|---|---|
| MIDI | `midi_add_item`, `midi_add_note` |
| Items | `item_split/move/glue_selected/render_in_place_selected` |
| Markers | `marker_add`, `region_add` |
| View | `view_zoom_to_selection`, `view_scroll_to`, `view_set_arrange_zoom` |
| Actions | `action_run(command_id)`, `action_get_toggle_state(command_id)` |
| Compose | `compose_and_render(output_path, notes, track_name)` - one call: new track + MIDI notes + render to audio |
| Escape hatch | `run_reascript(code)` - arbitrary ReaScript Lua |

These REAPER-only tools return a clear error if called against
`--daw=audacity` instead of silently doing nothing.

## Development

```
cargo build
cargo test --workspace
```

## License

Licensed under the [MIT License](LICENSE).
