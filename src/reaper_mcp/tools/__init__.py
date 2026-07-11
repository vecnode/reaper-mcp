"""Importing this package registers every tool module's @mcp.tool() functions."""

from . import (  # noqa: F401
    actions,
    compose,
    fx,
    items,
    markers,
    midi,
    project,
    raw,
    render,
    status,
    tracks,
    transport,
    view,
)
