"""Rendering / bouncing the project to an audio file."""

from __future__ import annotations

from ..app import call_bridge, mcp


@mcp.tool()
def render_project(
    output_path: str | None = None,
    start_sec: float | None = None,
    end_sec: float | None = None,
    overwrite: bool = False,
) -> dict:
    """Render the project to an audio file. If output_path is given, it
    overrides the configured render output file path first. If start_sec/
    end_sec are both given, renders exactly that time range; otherwise
    renders the full current project length (0 to the end of the last
    item) rather than depending on whatever range was last configured in
    REAPER's render dialog.

    If output_path already exists, this errors instead of rendering unless
    overwrite=True is passed - REAPER would otherwise show a blocking
    "overwrite?" dialog that freezes the bridge with no way to detect or
    dismiss it. Pass overwrite=True to replace an existing file, or choose
    a different output_path.

    Note: audio format/codec (WAV, MP3, bit depth, etc.) is NOT set by this
    tool - it still comes from REAPER's currently configured render
    settings. Use File -> Render once in REAPER to set the format you want;
    this tool only controls output path, time range, and overwrite behavior.

    Rendering can take time (up to 60 seconds depending on project length).
    """
    kwargs: dict = {"overwrite": overwrite}
    if output_path is not None:
        kwargs["output_path"] = output_path
    if start_sec is not None and end_sec is not None:
        kwargs["start_sec"] = start_sec
        kwargs["end_sec"] = end_sec
    return call_bridge("render_project", timeout=60, **kwargs)
