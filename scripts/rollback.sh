#!/bin/bash
# ================================================================
# Tadpole OS — POSIX Automated Binary Rollback
# ================================================================
# Usage: ./scripts/rollback.sh [target_version]
# If target_version is omitted, rolls back to the most recent backup.
# ================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

BACKUP_DIR="$REPO_ROOT/deploy_backups"
ENGINE_BINARY="$REPO_ROOT/server-rs/target/release/server-rs"
VERSION_FILE="$REPO_ROOT/version.json"

if [ ! -d "$BACKUP_DIR" ]; then
    mkdir -p "$BACKUP_DIR"
fi

if [ ! -f "$VERSION_FILE" ]; then
    echo "❌ Error: version.json not found in workspace root: $REPO_ROOT" >&2
    exit 1
fi

CURRENT_VERSION=$(grep -o '"version": "[^"]*"' "$VERSION_FILE" | cut -d'"' -f4)
TARGET_VERSION="${1:-}"

# 1. Resolve rollback binary target
if [ -z "$TARGET_VERSION" ]; then
    LATEST_BACKUP=$(find "$BACKUP_DIR" -maxdepth 1 -name "server-rs_*" ! -name "*.exe" | sort -r | head -n 1)
    if [ -z "$LATEST_BACKUP" ]; then
        echo "❌ Error: No previous Linux binary backups found in $BACKUP_DIR" >&2
        exit 1
    fi
    TARGET_BINARY="$LATEST_BACKUP"
    TARGET_VERSION=$(basename "$LATEST_BACKUP" | sed 's/server-rs_//')
else
    TARGET_BINARY="$BACKUP_DIR/server-rs_$TARGET_VERSION"
    if [ ! -f "$TARGET_BINARY" ]; then
        echo "❌ Error: Backup binary not found for version $TARGET_VERSION at $TARGET_BINARY" >&2
        exit 1
    fi
fi

echo "🔄 Initiating rollback from $CURRENT_VERSION to $TARGET_VERSION..."

# 2. Stop running engine if active
echo "🛑 Stopping running server-rs process if present..."
pkill -f "server-rs" || true
sleep 1

# 3. Swap binary
echo "💾 Restoring binary: $TARGET_BINARY -> $ENGINE_BINARY..."
mkdir -p "$(dirname "$ENGINE_BINARY")"
cp -f "$TARGET_BINARY" "$ENGINE_BINARY"
chmod +x "$ENGINE_BINARY"

# 4. Verify database schema record count
DB_PATH="$REPO_ROOT/data/tadpole.db"
if [ -f "$DB_PATH" ] && command -v sqlite3 >/dev/null 2>&1; then
    MIGRATION_COUNT=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM _sqlx_migrations;" 2>/dev/null || echo "0")
    echo "ℹ️ Current database contains $MIGRATION_COUNT migration records."
fi

echo "✅ Rollback to version $TARGET_VERSION completed successfully."
