"""
@docs ARCHITECTURE:Core

### AI Assist Note
**test_snapshot_state**: Core technical resource for the Tadpole OS infrastructure.

### 🔍 Debugging & Observability
- **Failure Path**: Script crash or unexpected exception.
- **Telemetry Link**: Search `[test_snapshot_state]` in system logs.
"""

import unittest
import shutil
import tempfile
from pathlib import Path
import sys

# Add execution directory to sys.path
sys.path.append(str(Path(__file__).parent.parent.parent / "execution"))

import snapshot_state

class TestSnapshotState(unittest.TestCase):
    def setUp(self):
        # Create temp dir for testing
        self.test_dir = tempfile.mkdtemp()
        self.root_path = Path(self.test_dir)
        self.snapshot_root_path = self.root_path / ".tmp" / "snapshots"
        
        # Override the module constants
        self.original_root = snapshot_state.ROOT
        self.original_snap_root = snapshot_state.SNAPSHOT_ROOT
        
        snapshot_state.ROOT = self.root_path
        snapshot_state.SNAPSHOT_ROOT = self.snapshot_root_path
        
        # Setup source files
        self.env_file = self.root_path / ".env"
        self.env_file.write_text("TEST_SNAPSHOT=1")
        
        self.data_dir = self.root_path / "data"
        self.data_dir.mkdir(parents=True, exist_ok=True)
        self.db_file = self.data_dir / "tadpole.db"
        self.db_file.write_text("mock database content")
        self.wal_file = self.data_dir / "tadpole.db-wal"
        self.wal_file.write_text("mock wal content")

    def tearDown(self):
        # Clean up temp directory
        shutil.rmtree(self.test_dir)
        # Restore module constants
        snapshot_state.ROOT = self.original_root
        snapshot_state.SNAPSHOT_ROOT = self.original_snap_root

    def test_save_snapshot(self):
        # Save snapshot
        snapshot_state.save_snapshot("test_snap_1")
        
        # Check files exist in snapshot folder
        snap_dir = self.snapshot_root_path / "test_snap_1"
        self.assertTrue(snap_dir.exists())
        self.assertTrue((snap_dir / ".env").exists())
        self.assertTrue((snap_dir / "tadpole.db").exists())
        self.assertTrue((snap_dir / "tadpole.db-wal").exists())

    def test_restore_snapshot(self):
        # Save snapshot first
        snapshot_state.save_snapshot("test_snap_2")
        
        # Modify active files
        self.env_file.write_text("MODIFIED=true")
        self.db_file.write_text("modified db")
        self.wal_file.write_text("modified wal")
        
        # Restore snapshot
        snapshot_state.restore_snapshot("test_snap_2")
        
        # Check files were rolled back
        self.assertEqual(self.env_file.read_text(), "TEST_SNAPSHOT=1")
        self.assertEqual(self.db_file.read_text(), "mock database content")
        self.assertEqual(self.wal_file.read_text(), "mock wal content")

if __name__ == "__main__":
    unittest.main()

# Metadata: [test_snapshot_state]
