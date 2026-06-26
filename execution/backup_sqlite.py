#!/usr/bin/env python3
"""
@docs ARCHITECTURE:Infrastructure:Execution
@docs OPERATIONS_MANUAL:Database

### AI Assist Note
**SQLite Backup (WAL-safe)**: Performs an online backup of the SQLite database
by utilizing python's native `sqlite3.Connection.backup` API. Avoids locking issues
common under high write concurrency in WAL mode. Validates structural integrity and
creates SHA-256 signatures for security auditing.

### 🔍 Debugging & Observability
- **Failure Path**: Target backup path permission errors, corrupt DB sources causing
  integrity checks to fail, or missing sqlite3 module dependencies.
- **Telemetry Link**: Search `[backup_sqlite]` in system logs.
"""

import sqlite3
import hashlib
import json
import sys
import os
from pathlib import Path
from datetime import datetime

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

def backup_sqlite():
    db_path = resolve_default_db_path()
    if not db_path.exists():
        print(f"❌ Error: Database not found at {db_path}", file=sys.stderr)
        sys.exit(1)

    backup_dir = db_path.parent / "backups"
    backup_dir.mkdir(parents=True, exist_ok=True)

    ts = datetime.utcnow().strftime("%Y%m%d_%H%M%S")
    backup_path = backup_dir / f"tadpole_{ts}.db"
    
    try:
        # Use SQLite .backup API (safe for WAL mode)
        # Open source database in read-only mode to prevent write interference
        src_uri = f"file:{db_path.resolve()}?mode=ro"
        conn = sqlite3.connect(src_uri, uri=True)
        dest = sqlite3.connect(backup_path)
        conn.backup(dest)
        dest.close()
        conn.close()
        
        # Verify integrity of the backup database
        verify = sqlite3.connect(backup_path)
        cursor = verify.execute("PRAGMA integrity_check")
        result = cursor.fetchone()[0]
        verify.close()
        
        if result != "ok":
            if backup_path.exists():
                backup_path.unlink()
            raise RuntimeError(f"Backup integrity check failed: {result}")
        
        # Checksum for tamper detection
        backup_bytes = backup_path.read_bytes()
        sha256 = hashlib.sha256(backup_bytes).hexdigest()
        
        meta = {
            "timestamp": ts,
            "sha256": sha256,
            "size": len(backup_bytes),
            "source": str(db_path)
        }
        
        meta_path = backup_path.with_suffix(".meta.json")
        meta_path.write_text(json.dumps(meta, indent=2))
        
        print(f"✅ Backup created: {backup_path} ({meta['size']} bytes, sha256={sha256[:16]}...)")
        
        # Prune old backups (keep last 30)
        prune_backups(backup_dir)
        return backup_path
        
    except Exception as e:
        print(f"❌ Backup failed: {str(e)}", file=sys.stderr)
        if backup_path.exists():
            backup_path.unlink()
        sys.exit(1)

def prune_backups(backup_dir: Path):
    """Keeps the last 30 backup files and metadata, deleting older ones."""
    backups = sorted(backup_dir.glob("tadpole_*.db"), key=os.path.getmtime)
    if len(backups) > 30:
        to_delete = backups[:-30]
        for b in to_delete:
            try:
                b.unlink()
                meta = b.with_suffix(".meta.json")
                if meta.exists():
                    meta.unlink()
                print(f"🧹 Pruned old backup: {b.name}")
            except Exception as e:
                print(f"⚠️ Failed to delete old backup {b}: {e}", file=sys.stderr)

def list_backups():
    db_path = resolve_default_db_path()
    backup_dir = db_path.parent / "backups"
    if not backup_dir.exists():
        print("No backups found.")
        return
        
    backups = sorted(backup_dir.glob("tadpole_*.db"), reverse=True)
    if not backups:
        print("No backups found.")
        return
        
    print(f"📋 Last {min(10, len(backups))} Backups:")
    for b in backups[:10]:
        meta = b.with_suffix(".meta.json")
        if meta.exists():
            try:
                data = json.loads(meta.read_text())
                print(f"  {b.name} - Size: {data['size']} bytes, SHA256: {data['sha256'][:16]}... (Created: {data['timestamp']})")
            except Exception:
                print(f"  {b.name}  (corrupt metadata)")
        else:
            print(f"  {b.name}  (no metadata)")

if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--list":
        list_backups()
    else:
        backup_sqlite()
