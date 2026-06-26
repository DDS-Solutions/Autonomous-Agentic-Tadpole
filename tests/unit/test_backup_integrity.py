import unittest
import os
import sqlite3
import shutil
import tempfile
from pathlib import Path
import sys

# Add execution directory to sys.path so we can import backup_sqlite/restore_sqlite
sys.path.append(str(Path(__file__).parent.parent.parent / "execution"))

import backup_sqlite
import restore_sqlite

class TestBackupRestoreIntegrity(unittest.TestCase):
    def setUp(self):
        # Create a temp directory for DB testing
        self.test_dir = tempfile.mkdtemp()
        self.db_path = Path(self.test_dir) / "test_tadpole.db"
        
        # Point environment variable DATABASE_URL to this temp database
        os.environ["DATABASE_URL"] = f"sqlite:{self.db_path}"
        
        # Create database and seed some tables
        self.conn = sqlite3.connect(self.db_path)
        self.conn.execute("CREATE TABLE test_agents (id TEXT PRIMARY KEY, name TEXT)")
        self.conn.execute("INSERT INTO test_agents (id, name) VALUES ('1', 'Agent Zero')")
        self.conn.commit()
        self.conn.close()

    def tearDown(self):
        # Clean up temp directory
        shutil.rmtree(self.test_dir)
        if "DATABASE_URL" in os.environ:
            del os.environ["DATABASE_URL"]

    def test_backup_creates_valid_file_and_meta(self):
        # Run backup
        backup_file = backup_sqlite.backup_sqlite()
        
        # Verify backup exists
        self.assertTrue(backup_file.exists())
        self.assertTrue(backup_file.with_suffix(".meta.json").exists())
        
        # Verify backup internal integrity
        conn = sqlite3.connect(backup_file)
        res = conn.execute("PRAGMA integrity_check").fetchone()[0]
        conn.close()
        self.assertEqual(res, "ok")

    def test_restore_recovers_state(self):
        # Run backup first
        backup_file = backup_sqlite.backup_sqlite()
        
        # Modify the active database: insert a row
        conn = sqlite3.connect(self.db_path)
        conn.execute("INSERT INTO test_agents (id, name) VALUES ('2', 'Agent One')")
        conn.commit()
        conn.close()
        
        # Call restore
        restore_sqlite.restore_sqlite(str(backup_file))
        
        # Verify active database was rolled back to original backup state (only 1 row)
        conn = sqlite3.connect(self.db_path)
        rows = conn.execute("SELECT id, name FROM test_agents").fetchall()
        conn.close()
        
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0], ('1', 'Agent Zero'))

if __name__ == "__main__":
    unittest.main()
