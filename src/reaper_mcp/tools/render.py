"""Rendering / bouncing the project to an audio file."""

from __future__ import annotations

from ..app import call_bridge, mcp


@mcp.tool()
def render_project(output_path: str | None = None) -> dict:
    """Render the project using its current render settings. If output_path is
    given, it overrides the configured render output file path first."""
    kwargs = {}
    if output_path is not None:
        kwargs["output_path"] = output_path
    return call_bridge("render_project", **kwargs)
