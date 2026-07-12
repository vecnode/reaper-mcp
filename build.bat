@echo off
REM Builds dawmcp and installs its bridges into every detected DAW, then
REM exits. Claude starts the actual MCP server itself per .mcp.json
REM ("cargo run --release -p dawmcp-server") - this script just makes sure
REM the release binary is pre-built and every DAW is wired up before you
REM reload Claude, so that first launch is instant.

cd /d "%~dp0"

echo Building dawmcp (release)...
cargo build --release -p dawmcp-server
if errorlevel 1 (
    echo.
    echo Build failed. Fix the error above and re-run this script.
    pause
    exit /b 1
)

echo.
echo Installing bridges for every detected DAW...
target\release\dawmcp.exe --install-bridge

echo.
echo Done. Reload Claude to connect - it starts the MCP server itself.
pause
