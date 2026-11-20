@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion
cd /d "%~dp0"

echo [System] Launching Full Autonomous Tadpole Stack...
echo.

:: ─────────────────────────────────────────────────────────────
:: Pre-flight: check required runtimes & node_modules
:: ─────────────────────────────────────────────────────────────
where node >nul 2>nul || (echo [ERROR] Node.js not found in PATH. Install Node.js 22+ and retry. & pause & exit /b 1)
where cargo >nul 2>nul || (echo [ERROR] Rust cargo not found in PATH. Install rustup/cargo and retry. & pause & exit /b 1)
where python >nul 2>nul || (echo [ERROR] Python not found in PATH. Install Python 3.11+ and retry. & pause & exit /b 1)
if not exist "node_modules" (
    echo [Setup] Installing frontend dependencies via npm install...
    call npm install || (echo [ERROR] npm install failed. & pause & exit /b 1)
)

:: ─────────────────────────────────────────────────────────────
:: Pre-flight: check .env exists
:: ─────────────────────────────────────────────────────────────
if not exist ".env" (
    echo [.env] No .env found. Launching Day-1 bootstrapper (execution\tadpole_init.py)...
    python execution\tadpole_init.py
    if not exist ".env" (
        echo [ERROR] .env file still missing. Run `npm run init` or copy .env.example to .env.
        pause
        exit /b 1
    )
)

:: Read PORT from .env (default 8000)
set "PORT=8000"
for /f "usebackq tokens=1,* delims==" %%A in (".env") do (
    if /i "%%A"=="PORT" set "PORT=%%~B"
)

:: ─────────────────────────────────────────────────────────────
:: Pre-flight: locale-agnostic TCP port availability checks
:: ─────────────────────────────────────────────────────────────
powershell -NoProfile -Command "if (Get-NetTCPConnection -LocalPort !PORT! -State Listen -ErrorAction SilentlyContinue) { exit 1 } else { exit 0 }" >nul 2>nul
if !errorlevel! neq 0 (
    echo [WARNING] Port !PORT! is already in use. Run stop_AA_tadpole.bat first, then retry.
    pause
    exit /b 1
)

powershell -NoProfile -Command "if (Get-NetTCPConnection -LocalPort 5173 -State Listen -ErrorAction SilentlyContinue) { exit 1 } else { exit 0 }" >nul 2>nul
if !errorlevel! neq 0 (
    echo [WARNING] Frontend port 5173 is already in use. Run stop_AA_tadpole.bat or free port 5173.
    pause
    exit /b 1
)

:: ─────────────────────────────────────────────────────────────
:: Step 1: Start backend — use /k so window STAYS open on crash
:: ─────────────────────────────────────────────────────────────
echo [Kernel] Starting Sovereign Kernel on port !PORT!...
start "Tadpole Engine [Backend]" cmd /k "npm run engine"

:: ─────────────────────────────────────────────────────────────
:: Step 2: Poll /health until ready (up to 600s for cold Rust builds)
:: ─────────────────────────────────────────────────────────────
echo [Kernel] Waiting for heartbeat on http://localhost:!PORT!/health (first cold Rust build may take 3-5 min)...
set "READY=0"
set "ATTEMPTS=0"
:HEALTH_POLL
    timeout /t 3 /nobreak >nul
    curl -s --max-time 2 "http://localhost:!PORT!/health" | findstr "tadpole_online" >nul
    if !errorlevel! == 0 (
        set "READY=1"
        goto HEALTH_DONE
    )
    set /a ATTEMPTS+=1
    if !ATTEMPTS! geq 200 goto HEALTH_TIMEOUT
    set /a MOD=ATTEMPTS %% 5
    if !MOD! == 0 echo    Elapsed ~!ATTEMPTS!x3s — still compiling/initializing kernel...
    goto HEALTH_POLL

:HEALTH_TIMEOUT
echo [ERROR] Engine did not respond after 600s. Check "Tadpole Engine [Backend]" window for compiler or config errors.
pause
exit /b 1

:HEALTH_DONE
echo [Kernel] Heartbeat confirmed.
echo.

:: ─────────────────────────────────────────────────────────────
:: Step 3: Start frontend — also /k to persist on crash
:: ─────────────────────────────────────────────────────────────
echo [UI] Starting Dashboard UI...
start "Tadpole Dashboard [Frontend]" cmd /k "npm run dev"

:: Verify frontend binds port 5173 (up to 15s, locale-agnostic)
set "UI_READY=0"
for /l %%I in (1,1,15) do (
    if !UI_READY! == 0 (
        timeout /t 1 /nobreak >nul
        powershell -NoProfile -Command "if (Get-NetTCPConnection -LocalPort 5173 -State Listen -ErrorAction SilentlyContinue) { exit 0 } else { exit 1 }" >nul 2>nul
        if !errorlevel! == 0 set "UI_READY=1"
    )
)

echo.
if !UI_READY! == 0 (
    echo [WARNING] Dashboard UI has not bound port 5173 yet. Check "Tadpole Dashboard [Frontend]" window.
) else (
    echo [UI] Dashboard confirmed on port 5173.
    echo [SUCCESS] Both systems are running.
)
echo Dashboard: http://localhost:5173
echo Engine:    http://localhost:!PORT!
echo.
pause
