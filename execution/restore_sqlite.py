#!/usr/bin/env python3
"""
@docs ARCHITECTURE:Infrastructure:Execution
@docs OPERATIONS_MANUAL:Database

### AI Assist Note
**SQLite Restore**: Restores the SQLite database from a specified backup path.
Validates the backup file's checksum against its metadata, backs up the current active
database as `tadpole.db.bak` before overwriting to prevent catastrophic data loss,
and runs a row-count parity check after restore.

### 🔍 Debugging & Observability
- **Failure Path**: Missing backup files, metadata mismatch, or locked database handles.
- **Telemetry Link**: Search `[restore_sqlite]` in system logs.
"""

import sqlite3
import hashlib
import json
import sys
import os
from pathlib import Path

def resolve_default_db_path() -> Path:
    """Resolves the database path dynamically from the environment."""
    db_url = os.getenv("DATABASE_URL")
    if db_url:
        if db_url.lower().startswith("sqlite:"):
            cleaned = db_url[7:]
            if cleaned.startswith("///"):
                cleaned = cleaned[3:]
            elif cleaned.startswith("//"):
                cleaned = cleaned[2:]
            return Path(cleaned)
        return Path(db_url)
    return Path("data/tadpole.db")

def verify_checksum(backup_path: Path) -> bool:
    """Verifies that the backup file matches the SHA-256 in its metadata."""
    meta_path = backup_path.with_suffix(".meta.json")
    if not meta_path.exists():
        print(f"⚠️ Warning: Metadata file not found for {backup_path}. Skipping checksum check.", file=sys.stderr)
        return True
        
    try:
        meta = json.loads(meta_path.read_text())
        expected_sha = meta.get("sha256")
        if not expected_sha:
            return True
            
        current_sha = hashlib.sha256(backup_path.read_bytes()).hexdigest()
        return current_sha == expected_sha
    except Exception as e:
        print(f"❌ Error reading metadata: {e}", file=sys.stderr)
        return False

def get_row_counts(db_path: Path) -> dict:
    """Retrieves a dict of table names and their row counts for validation."""
    counts = {}
    if not db_path.exists():
        return counts
    try:
        conn = sqlite3.connect(db_path)
        cursor = conn.cursor()
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table';")
        tables = [row[0] for row in cursor.fetchall()]
        for t in tables:
            # Skip SQLite internal tables
            if t.startswith("sqlite_"):
                continue
            cursor.execute(f"SELECT COUNT(*) FROM {t};")
            counts[t] = cursor.fetchone()[0]
        conn.close()
    except Exception as e:
        print(f"⚠️ Failed to get row counts for {db_path}: {e}", file=sys.stderr)
    return counts

def restore_sqlite(backup_file_path: str):
    backup_path = Path(backup_file_path)
    if not backup_path.exists():
        # Try checking in the default backups directory
        db_path = resolve_default_db_path()
        backup_dir = db_path.parent / "backups"
        alt_path = backup_dir / backup_file_path
        if alt_path.exists():
            backup_path = alt_path
        else:
            print(f"❌ Error: Backup file not found at {backup_file_path}", file=sys.stderr)
            sys.exit(1)

    print(f"🔍 Verifying backup {backup_path.name}...")
    if not verify_checksum(backup_path):
        print("❌ Error: Backup SHA-256 checksum mismatch. Backup file may be corrupted or tampered with.", file=sys.stderr)
        sys.exit(1)

    # Verify internal integrity
    try:
        verify = sqlite3.connect(backup_path)
        cursor = verify.execute("PRAGMA integrity_check")
        result = cursor.fetchone()[0]
        verify.close()
        if result != "ok":
            print(f"❌ Error: Backup integrity check failed: {result}", file=sys.stderr)
            sys.exit(1)
    except Exception as e:
        print(f"❌ Error checking integrity: {e}", file=sys.stderr)
        sys.exit(1)

    db_path = resolve_default_db_path()
    db_path.parent.mkdir(parents=True, exist_ok=True)

    # Get row counts of backup to verify later
    backup_counts = get_row_counts(backup_path)

    # Backup current DB first if it exists
    if db_path.exists():
        bak_path = db_path.with_suffix(".db.bak")
        print(f"📦 Backing up current database to {bak_path.name}...")
        try:
            if db_path.exists():
                # Perform clean copy
                src = sqlite3.connect(db_path)
                dest = sqlite3.connect(bak_path)
                src.backup(dest)
                dest.close()
                src.close()
        except Exception as e:
            print(f"❌ Failed to backup current database before restore: {e}", file=sys.stderr)
            sys.exit(1)

    print(f"⚡ Restoring database to {db_path}...")
    try:
        # Perform atomic copy
        src = sqlite3.connect(backup_path)
        dest = sqlite3.connect(db_path)
        src.backup(dest)
        dest.close()
        src.close()
        
        # Verify row count parity
        restored_counts = get_row_counts(db_path)
        mismatch = False
        for t, count in backup_counts.items():
            restored_count = restored_counts.get(t, 0)
            if restored_count != count:
                print(f"⚠️ Row count mismatch in table '{t}': backup had {count}, restored has {restored_count}", file=sys.stderr)
                mismatch = True
                
        if mismatch:
            print("❌ Restore completed with verification warnings. Please check the logs.", file=sys.stderr)
        else:
            print("✅ Restore completed successfully. All row counts match.")
            
    except Exception as e:
        print(f"❌ Restore failed: {str(e)}", file=sys.stderr)
        # Attempt to roll back from the .bak file
        bak_path = db_path.with_suffix(".db.bak")
        if bak_path.exists():
            print("🔄 Attempting to roll back to pre-restore state...", file=sys.stderr)
            try:
                src = sqlite3.connect(bak_path)
                dest = sqlite3.connect(db_path)
                src.backup(dest)
                dest.close()
                src.close()
                print("✅ Rolled back successfully.", file=sys.stderr)
            except Exception as rollback_err:
                print(f"🚨 CRITICAL: Rollback failed: {rollback_err}", file=sys.stderr)
        sys.exit(1)

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python restore_sqlite.py <backup_path_or_filename>")
        sys.exit(1)
        
    restore_sqlite(sys.argv[1])
