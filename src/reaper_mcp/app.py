"""Shared FastMCP app instance and bridge client, imported by every tools/*.py module."""

from __future__ import annotations

from mcp.server.fastmcp import FastMCP

from .bridge_client import BridgeClient, BridgeError

mcp = FastMCP(
    "reaper-mcp",
    instructions=(
        "Tools for controlling a live REAPER DAW instance: transport, tracks, FX, "
        "markers, view/zoom, rendering, and project state. Call reaper_status first "
        "if you're unsure whether REAPER and the bridge are reachable. Use "
        "run_reascript for anything not covered by a dedicated tool."
    ),
)

_client = BridgeClient()


def bridge() -> BridgeClient:
    return _client


def call_bridge(op: str, timeout: float | None = None, **args) -> dict:
    """Convenience wrapper: call an op on the bridge, raising a clear error for tool callers.

    Args:
        op: operation name
        timeout: optional timeout in seconds (default: 5s, or config default)
        **args: operation arguments
    """
    try:
        return _client.call(op, args, timeout=timeout)
    except BridgeError as exc:
        raise RuntimeError(str(exc)) from exc


__all__ = ["mcp", "bridge", "call_bridge", "BridgeError"]
