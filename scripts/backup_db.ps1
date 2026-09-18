<#
.SYNOPSIS
    Tadpole OS - Automated Database Backup for Windows PowerShell
.DESCRIPTION
    Creates an atomic SQLite backup with 7-day retention pruning.
#>
param (
    [string]$BackupDir = "data/backups",
    [string]$DbPath = "data/tadpole.db",
    [int]$KeepDays = 7
)

if (-not (Test-Path $DbPath)) {
    Write-Error "[BACKUP] Database not found at $DbPath"
    exit 1
}

if (-not (Test-Path $BackupDir)) {
    New-Item -ItemType Directory -Path $BackupDir -Force | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$backupFile = Join-Path $BackupDir "tadpole_$timestamp.db"

# Execute SQLite backup via python sqlite3 standard library (no external binary required)
$pyScript = @"
import sqlite3
source = sqlite3.connect('$($DbPath -replace '\\', '/')')
dest = sqlite3.connect('$($backupFile -replace '\\', '/')')
with dest:
    source.backup(dest)
dest.close()
source.close()
print('OK')
"@

$result = python -c $pyScript
if ($LASTEXITCODE -eq 0 -and (Test-Path $backupFile)) {
    $size = (Get-Item $backupFile).Length
    Write-Host "[BACKUP] Created: $backupFile ($([math]::Round($size / 1KB, 2)) KB)"
} else {
    Write-Error "[BACKUP] ERROR: Backup failed"
    exit 1
}

# Prune old backups
$cutoff = (Get-Date).AddDays(-$KeepDays)
$oldBackups = Get-ChildItem -Path $BackupDir -Filter "tadpole_*.db" | Where-Object { $_.LastWriteTime -lt $cutoff }
foreach ($old in $oldBackups) {
    Remove-Item $old.FullName -Force
    Write-Host "[BACKUP] Pruned: $($old.Name)"
}
