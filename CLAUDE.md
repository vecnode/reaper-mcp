# CLAUDE.md

See [AGENTS.md](AGENTS.md) for the full project brief (architecture, key
files, setup/test loop, past decisions, conventions, and open next steps).
Read that file first when picking up work here in a new conversation.

## Claude Code specific notes

- This repo has no CI configured yet - `uv run pytest` is the only
  automated check; there is nothing else to wait on before merging.
- The MCP server itself (`uv run reaper-mcp`) is stdio-based and blocks
  waiting for a client - that's expected behavior, not a hang, when run
  directly rather than through a client like Claude Code/Desktop.
- Live end-to-end testing requires a real running REAPER instance with
  `lua/reaper_bridge.lua` loaded via REAPER's Actions list; this can't be
  verified from an automated test run alone. `uv run reaper-mcp --status`
  is the fastest way to check reachability without needing an MCP client.
