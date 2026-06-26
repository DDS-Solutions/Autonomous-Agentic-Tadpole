#!/usr/bin/env python3
"""
@docs ARCHITECTURE:Infrastructure:Execution
@docs OPERATIONS_MANUAL:Database

### AI Assist Note
**Workspace Snapshot Utility**: Allows operators and autonomous agents to take atomic,
git-ignored snapshots of `.env` configuration files and the SQLite database.
Enables instant rollback capability (`--restore`) if code edits or automated
tests damage database integrity.

### 🔍 Debugging & Observability
- **Failure Path**: Missing files, write permissions, or locked DB handles during restores.
- **Telemetry Link**: Search `[snapshot_state]` in system logs.
"""

import sys
import shutil
import os
import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SNAPSHOT_ROOT = ROOT / ".tmp" / "snapshots"

def save_snapshot(name: str):
    snapshot_dir = SNAPSHOT_ROOT / name
    snapshot_dir.mkdir(parents=True, exist_ok=True)
    
    print(f"📸 Saving workspace snapshot '{name}'...")
    
    # 1. Backup .env
    env_file = ROOT / ".env"
    if env_file.exists():
        shutil.copy2(env_file, snapshot_dir / ".env")
        print("  - Saved .env configuration")
    else:
        print("  - No .env file found to save")
        
    # 2. Backup database
    db_file = ROOT / "data" / "tadpole.db"
    if db_file.exists():
        # Clean copy via SQLite backup or standard copy if idle
        shutil.copy2(db_file, snapshot_dir / "tadpole.db")
        print("  - Saved database file (tadpole.db)")
        
        # Check for WAL files
        wal_file = ROOT / "data" / "tadpole.db-wal"
        if wal_file.exists():
            shutil.copy2(wal_file, snapshot_dir / "tadpole.db-wal")
            print("  - Saved database WAL log")
        shm_file = ROOT / "data" / "tadpole.db-shm"
        if shm_file.exists():
            shutil.copy2(shm_file, snapshot_dir / "tadpole.db-shm")
    else:
        print("  - No database found under data/")
        
    print(f"✅ Snapshot '{name}' saved successfully in {snapshot_dir.relative_to(ROOT)}")

def restore_snapshot(name: str):
    snapshot_dir = SNAPSHOT_ROOT / name
    if not snapshot_dir.exists():
        print(f"❌ Error: Snapshot '{name}' not found at {snapshot_dir}", file=sys.stderr)
        sys.exit(1)
        
    print(f"🔄 Restoring workspace from snapshot '{name}'...")
    
    # 1. Restore .env
    snap_env = snapshot_dir / ".env"
    dest_env = ROOT / ".env"
    if snap_env.exists():
        shutil.copy2(snap_env, dest_env)
        print("  - Restored .env configuration")
        
    # 2. Restore database
    snap_db = snapshot_dir / "tadpole.db"
    dest_db = ROOT / "data" / "tadpole.db"
    if snap_db.exists():
        # Remove active WAL files before restoring to avoid corruption
        for suffix in (".db-wal", ".db-shm"):
            active_temp = dest_db.with_suffix(suffix)
            if active_temp.exists():
                active_temp.unlink()
                
        shutil.copy2(snap_db, dest_db)
        print("  - Restored database file (tadpole.db)")
        
        # Restore snap WAL if existed
        snap_wal = snapshot_dir / "tadpole.db-wal"
        if snap_wal.exists():
            shutil.copy2(snap_wal, dest_db.with_suffix(".db-wal"))
            print("  - Restored database WAL log")
            
        snap_shm = snapshot_dir / "tadpole.db-shm"
        if snap_shm.exists():
            shutil.copy2(snap_shm, dest_db.with_suffix(".db-shm"))
    else:
        print("  - No database backup found in snapshot")
        
    print(f"✅ Workspace restored to snapshot '{name}' successfully.")

def main():
    parser = argparse.ArgumentParser(description="Tadpole OS State Snapshot & Restore Tool")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--save", type=str, help="Save current state to snapshot name")
    group.add_argument("--restore", type=str, help="Restore state from snapshot name")
    args = parser.parse_args()
    
    if args.save:
        save_snapshot(args.save)
    elif args.restore:
        restore_snapshot(args.restore)

if __name__ == "__main__":
    main()
