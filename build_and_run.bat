@echo off
setlocal

cd /d "%~dp0"

where uv >nul 2>nul
if errorlevel 1 (
    echo [reaper-mcp] "uv" was not found on PATH. Install it from https://docs.astral.sh/uv/
    exit /b 1
)

echo [reaper-mcp] Syncing dependencies with uv...
uv sync
if errorlevel 1 exit /b 1

if not exist "lua\reaper_bridge.lua" (
    echo [reaper-mcp] lua\reaper_bridge.lua not found -- something is wrong with this checkout.
    exit /b 1
)

echo [reaper-mcp] Installing/updating the REAPER bridge script...
uv run reaper-mcp --install-bridge

echo [reaper-mcp] Starting MCP server (stdio)...
uv run reaper-mcp

endlocal
