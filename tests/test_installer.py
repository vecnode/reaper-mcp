from unittest.mock import patch

from reaper_mcp import installer
from reaper_mcp.discovery import ReaperInstall


def _install_for(tmp_path):
    resource_path = tmp_path / "REAPER"
    scripts_dir = resource_path / "Scripts"
    scripts_dir.mkdir(parents=True)
    return ReaperInstall(resource_path=str(resource_path), scripts_dir=str(scripts_dir), source="test")


def test_install_startup_hook_creates_fresh_file(tmp_path):
    install = _install_for(tmp_path)
    results = installer.install_startup_hook(install)

    dest = tmp_path / "REAPER" / "Scripts" / "__startup.lua"
    assert dest.exists()
    content = dest.read_text()
    assert installer.STARTUP_START_MARKER in content
    assert installer.STARTUP_END_MARKER in content
    assert "reaper_bridge.lua" in content
    assert any("installed" in r for r in results)


def test_install_startup_hook_preserves_foreign_content(tmp_path):
    install = _install_for(tmp_path)
    dest = tmp_path / "REAPER" / "Scripts" / "__startup.lua"
    dest.write_text("-- my own startup logic\nreaper.ShowConsoleMsg('hi')\n")

    installer.install_startup_hook(install)

    content = dest.read_text()
    assert "my own startup logic" in content
    assert "ShowConsoleMsg" in content
    assert installer.STARTUP_START_MARKER in content


def test_install_startup_hook_is_idempotent(tmp_path):
    install = _install_for(tmp_path)
    dest = tmp_path / "REAPER" / "Scripts" / "__startup.lua"

    installer.install_startup_hook(install)
    first_content = dest.read_text()
    results = installer.install_startup_hook(install)
    second_content = dest.read_text()

    assert first_content == second_content
    assert first_content.count(installer.STARTUP_START_MARKER) == 1
    assert any("up to date" in r for r in results)


def test_install_startup_hook_replaces_existing_block_not_duplicates(tmp_path):
    install = _install_for(tmp_path)
    dest = tmp_path / "REAPER" / "Scripts" / "__startup.lua"
    dest.write_text(
        "-- before\n"
        f"{installer.STARTUP_START_MARKER}\n"
        "-- stale content that should be replaced\n"
        f"{installer.STARTUP_END_MARKER}\n"
        "-- after\n"
    )

    installer.install_startup_hook(install)

    content = dest.read_text()
    assert content.count(installer.STARTUP_START_MARKER) == 1
    assert content.count(installer.STARTUP_END_MARKER) == 1
    assert "-- before" in content
    assert "-- after" in content
    assert "stale content" not in content
    assert "reaper_bridge.lua" in content


def test_install_bridge_also_installs_startup_hook(tmp_path):
    install = _install_for(tmp_path)
    with patch("reaper_mcp.installer.bridge_source_path") as mock_src:
        fake_bridge = tmp_path / "fake_bridge.lua"
        fake_bridge.write_text("-- fake bridge\n")
        mock_src.return_value = fake_bridge

        results = installer.install_bridge(install)

    startup_file = tmp_path / "REAPER" / "Scripts" / "__startup.lua"
    assert startup_file.exists()
    assert any("startup hook" in r for r in results)
