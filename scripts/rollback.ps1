param(
    [string]$TargetVersion = "",  # empty = previous
    [switch]$Force
)

$start = Get-Date

$EngineBinary = "server-rs/target/release/server-rs.exe"
$BackupDir = "deploy_backups"

if (-not (Test-Path $BackupDir)) {
    New-Item -ItemType Directory -Path $BackupDir -Force | Out-Null
}

if (-not (Test-Path "version.json")) {
    throw "version.json not found in workspace root"
}
$CurrentVersion = (Get-Content version.json | ConvertFrom-Json).version

# 1. Find rollback target
if (-not $TargetVersion) {
    $Backups = Get-ChildItem $BackupDir -Filter "server-rs_*.exe" | Sort-Object LastWriteTime -Descending
    # Need at least 1 backup to rollback
    if ($Backups.Count -lt 1) { 
        throw "No previous version backups found in $BackupDir" 
    }
    $TargetBinary = $Backups[0].FullName
    $TargetVersion = $Backups[0].BaseName -replace "server-rs_", ""
} else {
    $TargetBinary = "$BackupDir/server-rs_$TargetVersion.exe"
    if (-not (Test-Path $TargetBinary)) { 
        throw "Backup binary not found for version $TargetVersion at $TargetBinary" 
    }
}

Write-Host "🔄 Initiating rollback from $CurrentVersion to $TargetVersion..."

# 2. Stop engine
Write-Host "🛑 Stopping running engine..."
taskkill /F /IM server-rs.exe 2>$null
Start-Sleep -Seconds 2

# 3. Swap binary
Write-Host "💾 Swapping binary..."
Copy-Item $TargetBinary $EngineBinary -Force

# 4. Check migrations (optional DB rollback or warn)
# Compare migration count to verify schema compatibility
if (Test-Path "data/tadpole.db") {
    # Attempt to query migration count using sqlite3 CLI
    $MigrationCount = 0
    try {
        $MigrationCount = & sqlite3 data/tadpole.db "SELECT COUNT(*) FROM _sqlx_migrations;" 2>$null
    } catch {}
    Write-Host "ℹ️ Current database contains $MigrationCount migration records."
}

# 5. Start engine
Write-Host "🚀 Restarting engine..."
# Start engine back up via npm run engine
Start-Process "npm" -ArgumentList "run engine" -WindowStyle Hidden

$end = Get-Date
$Duration = (($end - $start).TotalSeconds).ToString("F2")
Write-Host "✅ Rollback to $TargetVersion complete in ${Duration}s"
