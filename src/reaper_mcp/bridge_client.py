"""File-based IPC client for talking to the reaper_bridge.lua ReaScript.

reaper_bridge.lua polls a "requests" directory on every reaper.defer() tick
(REAPER's UI frame rate), so round trips land in roughly one frame
(~16-33ms) without requiring any REAPER extension. This client writes one
JSON file per request, waits for the matching response file to appear, then
cleans both up.

Directory layout (mirrors the Lua side, which derives it from
reaper.GetResourcePath()):
    <bridge_dir>/requests/req_<id>.json
    <bridge_dir>/responses/resp_<id>.json
    <bridge_dir>/heartbeat.txt   (touched every tick the bridge script is alive)
"""

from __future__ import annotations

import itertools
import json
import os
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path

HEARTBEAT_STALE_AFTER_SEC = 2.0


class BridgeError(RuntimeError):
    """Raised when the bridge is unreachable or returns an error response."""


class BridgeNotConnected(BridgeError):
    """Raised when the bridge directory/heartbeat indicates the script isn't running."""


def default_bridge_dir() -> Path:
    """Resolve the bridge IPC directory, matching reaper_bridge.lua's own
    reaper.GetResourcePath() + "/Scripts/reaper_mcp_bridge" derivation."""
    override = os.environ.get("REAPER_MCP_BRIDGE_DIR")
    if override:
        return Path(override)

    from .discovery import find_reaper_installs

    installs = find_reaper_installs()
    if installs:
        return Path(installs[0].resource_path) / "Scripts" / "reaper_mcp_bridge"

    # last-resort fallback so callers still get a stable path to report in errors
    return Path.home() / ".reaper_mcp_bridge"


@dataclass
class BridgeConfig:
    bridge_dir: Path = field(default_factory=default_bridge_dir)
    request_timeout: float = 5.0
    poll_interval: float = 0.01


class BridgeClient:
    """Writes request files and polls for response files in the bridge directory."""

    def __init__(self, config: BridgeConfig | None = None):
        self.config = config or BridgeConfig()
        self._id_counter = itertools.count(1)

    @property
    def requests_dir(self) -> Path:
        return self.config.bridge_dir / "requests"

    @property
    def responses_dir(self) -> Path:
        return self.config.bridge_dir / "responses"

    @property
    def heartbeat_file(self) -> Path:
        return self.config.bridge_dir / "heartbeat.txt"

    def is_alive(self) -> bool:
        """True if the Lua bridge has touched its heartbeat file recently."""
        try:
            mtime = self.heartbeat_file.stat().st_mtime
        except FileNotFoundError:
            return False
        return (time.time() - mtime) < HEARTBEAT_STALE_AFTER_SEC

    def probe(self) -> bool:
        return self.is_alive()

    def close(self) -> None:
        pass  # no persistent connection to release

    # -- request/response ------------------------------------------------------

    def call(self, op: str, args: dict | None = None, retries: int = 0, timeout: float | None = None) -> dict:
        last_exc: Exception | None = None
        for attempt in range(retries + 1):
            try:
                return self._call_once(op, args or {}, timeout)
            except BridgeError as exc:
                last_exc = exc
                if attempt < retries:
                    time.sleep(0.2)
                    continue
        assert last_exc is not None
        raise last_exc

    def _call_once(self, op: str, args: dict, timeout: float | None = None) -> dict:
        if not self.is_alive():
            raise BridgeNotConnected(
                f"REAPER bridge heartbeat not found or stale at {self.heartbeat_file}. "
                "Is REAPER running with reaper_bridge.lua loaded (Actions -> Show action "
                "list -> run reaper_bridge.lua)? Run the reaper_status tool for diagnostics."
            )

        self.requests_dir.mkdir(parents=True, exist_ok=True)
        req_id = next(self._id_counter)
        payload = json.dumps({"id": req_id, "op": op, "args": args})
        self._write_atomic(self.requests_dir / f"req_{req_id}.json", payload)

        response_path = self.responses_dir / f"resp_{req_id}.json"
        effective_timeout = timeout if timeout is not None else self.config.request_timeout
        deadline = time.monotonic() + effective_timeout
        while time.monotonic() < deadline:
            if response_path.exists():
                try:
                    content = response_path.read_text(encoding="utf-8")
                except OSError:
                    time.sleep(self.config.poll_interval)
                    continue
                response_path.unlink(missing_ok=True)
                try:
                    msg = json.loads(content)
                except json.JSONDecodeError as exc:
                    raise BridgeError(f"malformed response from bridge: {content!r}") from exc
                if not msg.get("ok", False):
                    raise BridgeError(f"REAPER bridge error for op '{op}': {msg.get('error')}")
                return msg.get("result", {})
            time.sleep(self.config.poll_interval)

        raise BridgeError(f"timed out waiting for bridge response to op '{op}' (id={req_id})")

    @staticmethod
    def _write_atomic(path: Path, content: str) -> None:
        fd, tmp_name = tempfile.mkstemp(dir=str(path.parent), prefix=path.name + ".", suffix=".tmp")
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as f:
                f.write(content)
            os.replace(tmp_name, path)
        except BaseException:
            if os.path.exists(tmp_name):
                os.remove(tmp_name)
            raise


_default_client: BridgeClient | None = None


def get_default_client() -> BridgeClient:
    global _default_client
    if _default_client is None:
        _default_client = BridgeClient()
    return _default_client
