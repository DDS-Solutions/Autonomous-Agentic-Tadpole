@echo off
setlocal enabledelayedexpansion
cd /d %~dp0

echo 🚀 [System] Launching Full Autonomous Tadpole Stack...
echo.

:: ─────────────────────────────────────────────────────────────
:: Pre-flight: check .env exists
:: ─────────────────────────────────────────────────────────────
if not exist ".env" (
    echo ❌ [ERROR] .env file not found. Copy .env.example to .env and configure it.
    pause
    exit /b 1
)

:: Read PORT from .env (default 8000)
set "PORT=8000"
for /f "usebackq tokens=1,* delims==" %%A in (".env") do (
    if /i "%%A"=="PORT" set "PORT=%%B"
)

:: ─────────────────────────────────────────────────────────────
:: Pre-flight: check port is free
:: ─────────────────────────────────────────────────────────────
netstat -ano 2>nul | findstr ":!PORT!.*LISTENING" >nul
if !errorlevel! == 0 (
    echo ⚠️  [WARNING] Port !PORT! is already in use. Is a previous instance still running?
    echo    Run stop_AAtadpole.bat first, then retry.
    pause
    exit /b 1
)

:: ─────────────────────────────────────────────────────────────
:: Step 1: Start backend — use /k so window STAYS open on crash
:: ─────────────────────────────────────────────────────────────
echo 🧠 Starting Sovereign Kernel on port !PORT!...
start "Tadpole Engine [Backend]" cmd /k "npm run engine"

:: ─────────────────────────────────────────────────────────────
:: Step 2: Poll /health until the engine is ready (max 120s)
::         instead of blindly sleeping 5s
:: ─────────────────────────────────────────────────────────────
echo ⏳ Waiting for kernel heartbeat on http://localhost:!PORT!/health ...
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
    if !ATTEMPTS! geq 40 goto HEALTH_TIMEOUT
    echo    Attempt !ATTEMPTS!/40 — engine initializing...
    goto HEALTH_POLL

:HEALTH_TIMEOUT
echo ❌ [ERROR] Engine did not respond after 120s. Check "Tadpole Engine [Backend]" window.
pause
exit /b 1

:HEALTH_DONE
echo ✅ Kernel heartbeat confirmed.
echo.

:: ─────────────────────────────────────────────────────────────
:: Step 3: Start frontend — also /k to persist on crash
:: ─────────────────────────────────────────────────────────────
echo 🎨 Starting Dashboard UI...
start "Tadpole Dashboard [Frontend]" cmd /k "npm run dev"

echo.
echo ✅ [SUCCESS] Both systems are running.
echo 🌐 Dashboard: http://localhost:5173
echo 🧠 Engine:    http://localhost:!PORT!
echo.
pause
