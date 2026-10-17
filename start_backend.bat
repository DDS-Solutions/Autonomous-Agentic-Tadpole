@echo off
chcp 65001 >nul
TITLE Tadpole Engine [Backend]
echo 🚀 [Kernel] Starting Autonomous Tadpole Engine...
cd /d "%~dp0"

if not exist ".env" (
    echo ⚠️ [.env] Warning: .env file not found in root. Using default environment.
)

npm run engine

if %ERRORLEVEL% neq 0 (
    echo ❌ [Kernel] Backend terminated with error code %ERRORLEVEL%.
    pause
    exit /b %ERRORLEVEL%
)
