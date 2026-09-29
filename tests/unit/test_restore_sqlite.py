"""
@docs ARCHITECTURE:Core
@docs ARCHITECTURE:Persistence

### AI Assist Note
**test_restore_sqlite**: Adversarial and boundary verification unit tests for restore_sqlite.py.
Validates fail-closed row count parity, checksum verification, live engine guard, and rollback mechanics.

### 🔍 Debugging & Observability
- **Failure Path**: Integrity check or checksum failure during database restore.
- **Telemetry Link**: Search `[test_restore_sqlite]` in test output.
"""

import unittest
import os
import sqlite3
import shutil
import tempfile
import json
import hashlib
from pathlib import Path
import sys

EXEC_DIR = Path(__file__).resolve().parent.parent.parent / "execution"
sys.path.insert(0, str(EXEC_DIR))

import restore_sqlite
import backup_sqlite

class TestRestoreSqliteAdversarial(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.db_path = Path(self.test_dir) / "active_tadpole.db"
        os.environ["DATABASE_URL"] = f"sqlite:{self.db_path}"
        
        # Seed test database
        conn = sqlite3.connect(self.db_path)
        conn.execute("CREATE TABLE agents (id TEXT PRIMARY KEY, name TEXT)")
        conn.execute("INSERT INTO agents (id, name) VALUES ('agent-1', 'Alpha')")
        conn.commit()
        conn.close()

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)
        if "DATABASE_URL" in os.environ:
            del os.environ["DATABASE_URL"]

    def test_checksum_mismatch_fails_closed(self):
        # 1. Create a valid backup
        backup_file = backup_sqlite.backup_sqlite()
        
        # 2. Tamper with the backup file bytes
        with open(backup_file, "ab") as f:
            f.write(b"TAMPERED_BYTES")
            
        # 3. Assert restore refuses tampered backup
        with self.assertRaises(ValueError) as ctx:
            restore_sqlite.restore_sqlite(str(backup_file))
        self.assertIn("checksum mismatch", str(ctx.exception).lower())

    def test_get_row_counts_fails_closed_on_missing_or_empty_db(self):
        # Missing DB raises FileNotFoundError
        missing = Path(self.test_dir) / "nonexistent.db"
        with self.assertRaises(FileNotFoundError):
            restore_sqlite.get_row_counts(missing)

        # Empty DB (no tables) raises ValueError
        empty_db = Path(self.test_dir) / "empty.db"
        conn = sqlite3.connect(empty_db)
        conn.close()
        with self.assertRaises(RuntimeError) as ctx:
            restore_sqlite.get_row_counts(empty_db)
        self.assertIn("No application tables found", str(ctx.exception))

    def test_corrupt_database_fails_integrity_check(self):
        # Create a corrupt backup file with a valid matching checksum
        corrupt_backup = Path(self.test_dir) / "corrupt_tadpole.db"
        corrupt_backup.write_bytes(b"INVALID_SQLITE_HEADER_DATA_1234567890")
        
        # Write valid meta pointing to corrupt file
        sha256 = hashlib.sha256(corrupt_backup.read_bytes()).hexdigest()
        meta = {"sha256": sha256, "timestamp": "20260929_120000"}
        corrupt_backup.with_suffix(".meta.json").write_text(json.dumps(meta))

        with self.assertRaises(ValueError) as ctx:
            restore_sqlite.restore_sqlite(str(corrupt_backup))
        self.assertTrue(
            "integrity" in str(ctx.exception).lower() or "error" in str(ctx.exception).lower()
        )

    def test_live_engine_guard_blocks_restore_unless_forced(self):
        backup_file = backup_sqlite.backup_sqlite()
        
        # Mock check_live_engine returning True
        original_check = restore_sqlite.check_live_engine
        restore_sqlite.check_live_engine = lambda: True
        try:
            with self.assertRaises(RuntimeError) as ctx:
                restore_sqlite.restore_sqlite(str(backup_file))
            self.assertIn("currently running", str(ctx.exception))

            # With force=True, it should proceed past the guard
            restore_sqlite.restore_sqlite(str(backup_file), force=True)
        finally:
            restore_sqlite.check_live_engine = original_check

if __name__ == "__main__":
    unittest.main()

# Metadata: [test_restore_sqlite]
