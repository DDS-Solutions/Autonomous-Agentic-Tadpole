#!/bin/bash
# ================================================================
# Tadpole OS — Automated Database Backup (WAL-Safe & Verified)
# ================================================================
# Uses SQLite's .backup command for an atomic, consistent snapshot.
# Validates structural integrity and creates SHA-256 metadata.
#
# Usage: ./scripts/backup_db.sh
# ================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

BACKUP_DIR="${BACKUP_DIR:-$REPO_ROOT/data/backups}"
DB_PATH="${DB_PATH:-$REPO_ROOT/data/tadpole.db}"
KEEP_DAYS="${KEEP_DAYS:-7}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="$BACKUP_DIR/tadpole_$TIMESTAMP.db"
META_FILE="$BACKUP_DIR/tadpole_$TIMESTAMP.meta.json"

# Ensure backup directory exists
mkdir -p "$BACKUP_DIR"

# Verify source database exists
if [ ! -f "$DB_PATH" ]; then
    echo "[BACKUP] ERROR: Database not found at $DB_PATH" >&2
    exit 1
fi

# Create atomic backup
echo "[BACKUP] Creating snapshot from $DB_PATH..."
sqlite3 "$DB_PATH" ".backup '$BACKUP_FILE'"

# Verify backup integrity
echo "[BACKUP] Verifying PRAGMA integrity_check..."
INTEGRITY=$(sqlite3 "$BACKUP_FILE" "PRAGMA integrity_check;")
if [ "$INTEGRITY" != "ok" ]; then
    echo "[BACKUP] ERROR: Integrity check failed: $INTEGRITY" >&2
    rm -f "$BACKUP_FILE"
    exit 1
fi

# Generate SHA-256 sidecar
if command -v sha256sum >/dev/null 2>&1; then
    SHA256=$(sha256sum "$BACKUP_FILE" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
    SHA256=$(shasum -a 256 "$BACKUP_FILE" | awk '{print $1}')
else
    SHA256="unavailable"
fi

cat <<EOF > "$META_FILE"
{
  "timestamp": "$TIMESTAMP",
  "source_db": "$DB_PATH",
  "backup_file": "$BACKUP_FILE",
  "sha256": "$SHA256",
  "integrity": "$INTEGRITY"
}
EOF

SIZE=$(du -h "$BACKUP_FILE" | cut -f1)
echo "[BACKUP] ✅ Created verified backup: $BACKUP_FILE ($SIZE, SHA256: $SHA256)"

# Prune old backups constrained to maxdepth 1
PRUNED=$(find "$BACKUP_DIR" -maxdepth 1 -name "tadpole_*.db" -mtime +"$KEEP_DAYS" -delete -print 2>/dev/null | wc -l)
find "$BACKUP_DIR" -maxdepth 1 -name "tadpole_*.meta.json" -mtime +"$KEEP_DAYS" -delete 2>/dev/null || true
if [ "$PRUNED" -gt 0 ]; then
    echo "[BACKUP] Pruned $PRUNED backup(s) older than $KEEP_DAYS days"
fi
