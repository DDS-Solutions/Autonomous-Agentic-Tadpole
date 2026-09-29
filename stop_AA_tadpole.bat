@echo off
setlocal enabledelayedexpansion

echo 🛑 [System] Initiating graceful Tadpole OS shutdown...
echo.

:: ─────────────────────────────────────────────────────────────
:: Step 1: Read port from .env (default 8000) and call the
::         graceful shutdown endpoint. This flushes the WAL,
::         persists all agent state, and drains in-flight work.
:: ─────────────────────────────────────────────────────────────

:: Load NEURAL_TOKEN and PORT from .env if present
set "PORT=8000"
set "NEURAL_TOKEN="
if exist "%~dp0.env" (
    for /f "usebackq tokens=1,* delims==" %%A in ("%~dp0.env") do (
        if /i "%%A"=="PORT"         set "PORT=%%~B"
        if /i "%%A"=="NEURAL_TOKEN" set "NEURAL_TOKEN=%%~B"
    )
)

:: SEC-ARGV: Never pass secret tokens in command-line arguments (visible in Win32_Process.CommandLine).
:: Write headers to a short-lived temporary config file and delete immediately after execution.
set "CURL_CFG=%TEMP%\tad_shutdown_%RANDOM%_%TIME:~6,2%.cfg"
(
    echo header = "Authorization: Bearer !NEURAL_TOKEN!"
    echo header = "Content-Type: application/json"
) > "!CURL_CFG!"

echo ⏳ Calling graceful shutdown: POST http://localhost:!PORT!/v1/engine/shutdown
curl -s -X POST "http://localhost:!PORT!/v1/engine/shutdown" ^
     -K "!CURL_CFG!" ^
     --max-time 8 ^
     -o nul -w "   HTTP status: %%{http_code}"
del /f /q "!CURL_CFG!" 2>nul
echo.

:: Wait for the engine to persist state and exit cleanly (it self-exits after 500ms)
echo ⏳ Waiting 4s for engine to flush and exit...
timeout /t 4 /nobreak >nul

:: ─────────────────────────────────────────────────────────────
:: Step 2: Only terminate if the engine process bound to PORT
::         is still running after graceful exit (safety net)
:: ─────────────────────────────────────────────────────────────
set "ENGINE_PID="
for /f "tokens=5" %%P in ('netstat -ano 2^>nul ^| findstr /R /C:":!PORT!  .*LISTENING"') do (
    set "ENGINE_PID=%%P"
)
if defined ENGINE_PID (
    echo ⚠️  Engine (PID !ENGINE_PID!) still active after graceful shutdown — terminating...
    taskkill /F /PID !ENGINE_PID! /T 2>nul
) else (
    echo ✅ Engine exited cleanly.
)

:: ─────────────────────────────────────────────────────────────
:: Step 3: Kill ONLY the Tadpole dashboard Node process by its
::         window title — NOT all node.exe on the machine.
:: ─────────────────────────────────────────────────────────────
echo ⏳ Stopping Dashboard UI...
taskkill /FI "WINDOWTITLE eq Tadpole Dashboard [Frontend]" /T /F 2>nul
:: Fallback: kill by port 5173 only if still bound
for /f "tokens=5" %%P in ('netstat -ano 2^>nul ^| findstr /R /C:":5173  .*LISTENING"') do (
    taskkill /F /PID %%P /T 2>nul
)

echo.
echo ✅ Tadpole OS shut down cleanly. State persisted to database.
echo.
pause
