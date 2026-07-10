import json
import threading
import time

import pytest

from reaper_mcp.bridge_client import BridgeClient, BridgeConfig, BridgeError, BridgeNotConnected


class MockBridgeWorker:
    """Stand-in for reaper_bridge.lua's defer-loop pump: watches the requests
    dir, answers known ops, and keeps the heartbeat file fresh."""

    def __init__(self, bridge_dir):
        self.bridge_dir = bridge_dir
        self.requests_dir = bridge_dir / "requests"
        self.responses_dir = bridge_dir / "responses"
        self.heartbeat_file = bridge_dir / "heartbeat.txt"
        self.requests_dir.mkdir(parents=True, exist_ok=True)
        self.responses_dir.mkdir(parents=True, exist_ok=True)
        self._stop = False
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()

    def _run(self):
        while not self._stop:
            self.heartbeat_file.write_text(str(time.time()))
            for req_file in list(self.requests_dir.glob("req_*.json")):
                try:
                    req = json.loads(req_file.read_text())
                except (OSError, json.JSONDecodeError):
                    continue
                req_file.unlink(missing_ok=True)
                if req["op"] == "ping":
                    resp = {"id": req["id"], "ok": True, "result": {"pong": True}}
                else:
                    resp = {"id": req["id"], "ok": False, "error": f"unknown op {req['op']}"}
                resp_path = self.responses_dir / f"resp_{req['id']}.json"
                resp_path.write_text(json.dumps(resp))
            time.sleep(0.01)

    def stop(self):
        self._stop = True
        self._thread.join(timeout=1)


@pytest.fixture
def mock_bridge(tmp_path):
    worker = MockBridgeWorker(tmp_path / "reaper_mcp_bridge")
    yield worker
    worker.stop()


def test_call_success(mock_bridge):
    client = BridgeClient(BridgeConfig(bridge_dir=mock_bridge.bridge_dir))
    result = client.call("ping")
    assert result == {"pong": True}


def test_call_unknown_op_raises(mock_bridge):
    client = BridgeClient(BridgeConfig(bridge_dir=mock_bridge.bridge_dir))
    with pytest.raises(BridgeError):
        client.call("nonexistent_op")


def test_no_heartbeat_raises_bridge_not_connected(tmp_path):
    client = BridgeClient(BridgeConfig(bridge_dir=tmp_path / "no_bridge_here"))
    with pytest.raises(BridgeNotConnected):
        client.call("ping")


def test_probe(mock_bridge, tmp_path):
    client = BridgeClient(BridgeConfig(bridge_dir=mock_bridge.bridge_dir))
    time.sleep(0.05)
    assert client.probe() is True

    dead_client = BridgeClient(BridgeConfig(bridge_dir=tmp_path / "dead"))
    assert dead_client.probe() is False
