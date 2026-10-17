# deploy-bunker-2.ps1
# Build and deploy script for Bunker 2 that archives the active binary as a rollback point.
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$BackupDir = "deploy_backups"
$EngineBinary = "target/release/server-rs.exe"
if (-not (Test-Path $EngineBinary)) {
    $EngineBinary = "server-rs/target/release/server-rs.exe"
}

# 0. Check for running instance
$RunningProcs = Get-Process -Name "server-rs" -ErrorAction SilentlyContinue
if ($RunningProcs) {
    Write-Warning "⚠️ server-rs is currently running. Stopping or file lock conflict may occur."
}

if (-not (Test-Path $BackupDir)) {
    New-Item -ItemType Directory -Path $BackupDir -Force | Out-Null
}

# 1. Read version
if (-not (Test-Path "version.json")) {
    @{ version = "1.1.58" } | ConvertTo-Json | Set-Content "version.json"
}
$Version = (Get-Content "version.json" | ConvertFrom-Json).version

# 2. Archive active binary if exists
if (Test-Path $EngineBinary) {
    $ArchiveBinary = "$BackupDir/server-rs_$Version.exe"
    Write-Host "📦 Archiving current binary version $Version to $ArchiveBinary"
    Copy-Item -Path $EngineBinary -Destination $ArchiveBinary -Force
}

# 3. Build new version via workspace
Write-Host "🏗️ Compiling engine for Bunker 2..."
Remove-Item Env:\RUSTC_WRAPPER -ErrorAction SilentlyContinue
cargo build --release --package server-rs

if ($LASTEXITCODE -ne 0) {
    Write-Error "❌ Build failed. Deployment aborted."
    exit $LASTEXITCODE
}

Write-Host "✅ Deployment to Bunker 2 successful. New binary compiled."
