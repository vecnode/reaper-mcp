"""Entry point for the reaper-mcp server.

Usage:
    uv run reaper-mcp                 # run the MCP server over stdio
    uv run reaper-mcp --install-bridge # install/update reaper_bridge.lua, then exit
    uv run reaper-mcp --status         # print discovery/diagnostics, then exit
"""

from __future__ import annotations

import argparse
import json
import sys

from .app import mcp
from . import tools  # noqa: F401  (import registers all @mcp.tool() functions)


def _print_status() -> None:
    from .discovery import run_discovery

    print(json.dumps(run_discovery().as_dict(), indent=2))


def _install_bridge() -> None:
    from .installer import install_bridge

    for line in install_bridge():
        print(line)


def main() -> None:
    parser = argparse.ArgumentParser(prog="reaper-mcp")
    parser.add_argument(
        "--install-bridge",
        action="store_true",
        help="Copy reaper_bridge.lua into the detected REAPER Scripts folder(s), then exit.",
    )
    parser.add_argument(
        "--status",
        action="store_true",
        help="Print REAPER discovery/diagnostics (installs, running PIDs, bridge reachability), then exit.",
    )
    args = parser.parse_args()

    if args.install_bridge:
        _install_bridge()
        return
    if args.status:
        _print_status()
        return

    mcp.run(transport="stdio")


if __name__ == "__main__":
    sys.exit(main())
