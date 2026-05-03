@echo off
echo 🚀 [System] Launching Full Autonomous Tadpole Stack...
cd /d %~dp0

:: Start the Backend in its own window
echo 🧠 Starting Sovereign Kernel...
start "Tadpole Engine [Backend]" cmd /c "npm run engine"

:: Brief pause to let the database lock and API server initialize
echo ⏳ Waiting for kernel heartbeat...
timeout /t 5 /nobreak >nul

:: Start the Frontend in its own window
echo 🎨 Starting Dashboard UI...
start "Tadpole Dashboard [Frontend]" cmd /c "npm run dev"

echo.
echo ✅ [SUCCESS] Both systems are booting in separate windows.
echo 🌐 Dashboard will be available at http://localhost:5173
echo.
pause
