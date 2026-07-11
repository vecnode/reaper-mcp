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
    assert "Main_openProject" in content
    assert installer.DEFAULT_PROJECT_INSTALLED_FILENAME in content
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


def test_install_default_project_copies_file(tmp_path):
    install = _install_for(tmp_path)
    with patch("reaper_mcp.installer.default_project_source_path") as mock_src:
        fake_project = tmp_path / "fake_default.RPP"
        fake_project.write_text("<REAPER_PROJECT 0.1 fake\n>\n")
        mock_src.return_value = fake_project

        results = installer.install_default_project(install)

    dest = tmp_path / "REAPER" / installer.DEFAULT_PROJECT_INSTALLED_FILENAME
    assert dest.exists()
    assert dest.read_text() == fake_project.read_text()
    assert any("installed" in r for r in results)


def test_install_default_project_is_idempotent(tmp_path):
    install = _install_for(tmp_path)
    with patch("reaper_mcp.installer.default_project_source_path") as mock_src:
        fake_project = tmp_path / "fake_default.RPP"
        fake_project.write_text("<REAPER_PROJECT 0.1 fake\n>\n")
        mock_src.return_value = fake_project

        installer.install_default_project(install)
        results = installer.install_default_project(install)

    assert any("up to date" in r for r in results)


def test_install_default_project_missing_source_is_soft_skip(tmp_path):
    install = _install_for(tmp_path)
    with patch(
        "reaper_mcp.installer.default_project_source_path",
        side_effect=FileNotFoundError,
    ):
        results = installer.install_default_project(install)

    dest = tmp_path / "REAPER" / installer.DEFAULT_PROJECT_INSTALLED_FILENAME
    assert not dest.exists()
    assert any("skipped" in r for r in results)


def test_install_bridge_also_installs_default_project(tmp_path):
    install = _install_for(tmp_path)
    with (
        patch("reaper_mcp.installer.bridge_source_path") as mock_bridge_src,
        patch("reaper_mcp.installer.default_project_source_path") as mock_project_src,
    ):
        fake_bridge = tmp_path / "fake_bridge.lua"
        fake_bridge.write_text("-- fake bridge\n")
        mock_bridge_src.return_value = fake_bridge

        fake_project = tmp_path / "fake_default.RPP"
        fake_project.write_text("<REAPER_PROJECT 0.1 fake\n>\n")
        mock_project_src.return_value = fake_project

        results = installer.install_bridge(install)

    dest = tmp_path / "REAPER" / installer.DEFAULT_PROJECT_INSTALLED_FILENAME
    assert dest.exists()
    assert any("default project" in r for r in results)
