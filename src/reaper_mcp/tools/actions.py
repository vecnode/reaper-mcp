"""Run and query REAPER's native/custom actions by command ID.

REAPER identifies both native UI toggles (snap to grid, ripple edit, etc.)
and custom/ReaScript actions by a numeric command ID, scoped to a "section"
(0 = main). This module does not hardcode any specific IDs -- REAPER
assigns them per-install and they must be looked up live: in REAPER, open
the Actions list, find the action, right-click -> "Copy selected action ID".
"""

from __future__ import annotations

from ..app import call_bridge, mcp


@mcp.tool()
def action_run(command_id: int, section: int = 0) -> dict:
    """Run a REAPER action (native or custom) by its numeric command ID.
    For toggle actions (e.g. snap to grid, ripple editing), calling this
    again flips the toggle. Look up command_id via REAPER's Actions list
    (right-click an action -> 'Copy selected action ID'). section defaults
    to 0 (Main)."""
    return call_bridge("action_run", command_id=command_id, section=section)


@mcp.tool()
def action_get_toggle_state(command_id: int, section: int = 0) -> dict:
    """Get the on/off state of a toggle action by command ID: 1 = on, 0 =
    off, -1 = not a toggle action (or state unknown). Use this before/after
    action_run to confirm a toggle actually changed."""
    return call_bridge("action_get_state", command_id=command_id, section=section)
