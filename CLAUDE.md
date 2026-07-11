# CLAUDE.md

See [AGENTS.md](AGENTS.md) for the full project brief (architecture, key
files, setup/test loop, past decisions, conventions, and open next steps).
Read that file first when picking up work here in a new conversation.

## Claude Code specific notes

- CI runs `uv run pytest` on push/PR to `main` via
  `.github/workflows/ci.yml` (windows-latest, matching the project's
  primary target). Check `gh pr checks` before merging if CI has had time
  to run; `uv run pytest` locally is still the faster loop while iterating.
- The MCP server itself (`uv run reaper-mcp`) is stdio-based and blocks
  waiting for a client - that's expected behavior, not a hang, when run
  directly rather than through a client like Claude Code/Desktop.
- Live end-to-end testing requires a real running REAPER instance with
  `lua/reaper_bridge.lua` loaded via REAPER's Actions list; this can't be
  verified from an automated test run alone. `uv run reaper-mcp --status`
  is the fastest way to check reachability without needing an MCP client.
