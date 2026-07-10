from unittest.mock import patch

from reaper_mcp import discovery


def _fake_proc(pid, name, exe):
    class P:
        info = {"pid": pid, "name": name, "exe": exe}

    return P()


def test_find_running_reaper_matches_name(monkeypatch):
    procs = [
        _fake_proc(111, "reaper.exe", r"C:\Program Files\REAPER (x64)\reaper.exe"),
        _fake_proc(222, "chrome.exe", r"C:\chrome.exe"),
    ]
    with patch("reaper_mcp.discovery.psutil.process_iter", return_value=procs):
        found = discovery.find_running_reaper()
    assert len(found) == 1
    assert found[0].pid == 111
    assert found[0].name == "reaper.exe"


def test_find_running_reaper_empty(monkeypatch):
    with patch("reaper_mcp.discovery.psutil.process_iter", return_value=[]):
        found = discovery.find_running_reaper()
    assert found == []


def test_find_reaper_installs_respects_env_override(tmp_path, monkeypatch):
    fake_resource_dir = tmp_path / "REAPER"
    fake_resource_dir.mkdir()
    monkeypatch.setenv("REAPER_RESOURCE_PATH", str(fake_resource_dir))
    installs = discovery.find_reaper_installs()
    assert any(i.resource_path == str(fake_resource_dir) for i in installs)


def test_run_discovery_report_shape(tmp_path, monkeypatch):
    monkeypatch.setenv("REAPER_MCP_BRIDGE_DIR", str(tmp_path / "no_such_bridge_dir"))
    with patch("reaper_mcp.discovery.psutil.process_iter", return_value=[]):
        report = discovery.run_discovery()
    d = report.as_dict()
    assert set(d.keys()) == {
        "os",
        "running_processes",
        "installs",
        "bridge_reachable",
        "bridge_dir",
    }
    assert d["bridge_reachable"] is False


def test_run_discovery_bridge_reachable_when_heartbeat_fresh(tmp_path, monkeypatch):
    bridge_dir = tmp_path / "bridge"
    bridge_dir.mkdir()
    (bridge_dir / "heartbeat.txt").write_text("123.456")
    monkeypatch.setenv("REAPER_MCP_BRIDGE_DIR", str(bridge_dir))
    with patch("reaper_mcp.discovery.psutil.process_iter", return_value=[]):
        report = discovery.run_discovery()
    assert report.bridge_reachable is True
    assert report.bridge_dir == str(bridge_dir)
