@echo off
echo 🛑 [System] Shutting down Tadpole OS processes...

:: Kill by image name
taskkill /F /IM server-rs.exe /T 2>nul
taskkill /F /IM node.exe /T 2>nul

echo ✅ All processes terminated.
pause
