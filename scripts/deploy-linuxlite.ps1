# deploy-linuxlite.ps1
# Build and deploy script that archives the active binary as a rollback point.

$BackupDir = "deploy_backups"
$EngineBinary = "server-rs/target/release/server-rs.exe"

if (-not (Test-Path $BackupDir)) {
    New-Item -ItemType Directory -Path $BackupDir -Force | Out-Null
}

# 1. Read version
if (-not (Test-Path "version.json")) {
    @{ version = "1.1.57" } | ConvertTo-Json | Set-Content "version.json"
}
$Version = (Get-Content version.json | ConvertFrom-Json).version

# 2. Archive active binary if exists
if (Test-Path $EngineBinary) {
    $ArchiveBinary = "$BackupDir/server-rs_$Version.exe"
    Write-Host "📦 Archiving current binary version $Version to $ArchiveBinary"
    Copy-Item $EngineBinary $ArchiveBinary -Force
}

# 3. Build new version
Write-Host "🏗️ Compiling engine..."
# Run cargo build in release mode
Push-Location server-rs
cargo build --release
Pop-Location

if ($LASTEXITCODE -ne 0) {
    Write-Error "❌ Build failed. Deployment aborted."
    exit $LASTEXITCODE
}

Write-Host "✅ Deployment successful. New binary compiled."
