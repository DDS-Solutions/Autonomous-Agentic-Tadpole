"""
@docs ARCHITECTURE:Core

### AI Assist Note
**Audit Chain Verifier Tests**: Comprehensive test suite validating the cryptographic
guarantees of the audit trail ledger verifier.

### 🔍 Debugging & Observability
- **Failure Path**: Cryptographic calculation deviation, false positive/negative on tampering.
- **Telemetry Link**: Search `[test_verify_audit_chain]` in test logs.
"""

import os
import sqlite3
import tempfile
import unittest
from datetime import datetime, timezone

from execution.verify_audit_chain import compute_audit_hash, verify_audit_trail


class TestVerifyAuditChain(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.db_path = os.path.join(self.temp_dir.name, "test_audit.db")
        self.conn = sqlite3.connect(self.db_path)
        self.conn.execute(
            """CREATE TABLE audit_trail (
                id TEXT PRIMARY KEY NOT NULL,
                agent_id TEXT NOT NULL,
                action TEXT NOT NULL,
                params TEXT NOT NULL,
                prev_hash TEXT NOT NULL,
                current_hash TEXT NOT NULL,
                timestamp DATETIME NOT NULL,
                signature TEXT,
                mission_id TEXT,
                user_id TEXT,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )"""
        )
        self.conn.commit()

    def tearDown(self):
        self.conn.close()
        self.temp_dir.cleanup()

    def _insert_entry(self, entry_id, agent_id, action, params, prev_hash, mission_id=None, user_id=None, timestamp=None):
        if timestamp is None:
            timestamp = datetime.now(timezone.utc).isoformat()
        current_hash = compute_audit_hash(
            prev_hash=prev_hash,
            agent_id=agent_id,
            mission_id=mission_id,
            user_id=user_id,
            action=action,
            params=params,
            timestamp=timestamp,
        )
        self.conn.execute(
            """INSERT INTO audit_trail (id, agent_id, action, params, prev_hash, current_hash, timestamp, mission_id, user_id)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)""",
            (entry_id, agent_id, action, params, prev_hash, current_hash, timestamp, mission_id, user_id),
        )
        self.conn.commit()
        return current_hash, timestamp

    def test_empty_audit_trail(self):
        """Happy Path: Empty table is valid genesis state."""
        success, message, stats = verify_audit_trail(self.db_path)
        self.assertTrue(success)
        self.assertEqual(stats["total"], 0)

    def test_valid_linear_chain(self):
        """Happy Path: Continuous unbroken chain passes verification."""
        genesis_prev = "0" * 64
        h1, _ = self._insert_entry("e1", "agent-1", "start", "{}", genesis_prev)
        h2, _ = self._insert_entry("e2", "agent-1", "step", '{"step": 1}', h1)
        h3, _ = self._insert_entry("e3", "agent-1", "finish", '{"status": "ok"}', h2)

        success, message, stats = verify_audit_trail(self.db_path, strict_continuity=True)
        self.assertTrue(success)
        self.assertEqual(stats["total_checked"], 3)
        self.assertEqual(stats["hash_mismatches"], 0)
        self.assertEqual(stats["orphan_parents"], 0)
        self.assertEqual(stats["continuity_breaks"], 0)

    def test_tampered_payload_detection(self):
        """Failure Path: Tampering with params breaks hash integrity."""
        genesis_prev = "0" * 64
        h1, _ = self._insert_entry("e1", "agent-1", "transfer", '{"amount": 100}', genesis_prev)

        # Adversary modifies params directly in SQLite
        self.conn.execute("UPDATE audit_trail SET params = '{\"amount\": 1000000}' WHERE id = 'e1'")
        self.conn.commit()

        success, message, stats = verify_audit_trail(self.db_path)
        self.assertFalse(success)
        self.assertEqual(stats["hash_mismatches"], 1)
        self.assertIn("hash mismatches", message)

    def test_orphan_parent_detection(self):
        """Failure Path: Entry with non-existent ancestor hash fails."""
        fake_prev = "deadbeef" * 8
        self._insert_entry("e1", "agent-1", "action", "{}", fake_prev)

        success, message, stats = verify_audit_trail(self.db_path)
        self.assertFalse(success)
        self.assertEqual(stats["orphan_parents"], 1)
        self.assertIn("orphan parents", message)

    def test_strict_continuity_break_detection(self):
        """Edge Case Path: Forked chain with valid hashes fails strict continuity."""
        genesis_prev = "0" * 64
        h1, _ = self._insert_entry("e1", "agent-1", "step-1", "{}", genesis_prev)
        h2a, _ = self._insert_entry("e2a", "agent-2", "fork-a", "{}", h1)
        # e2b also branches off h1 instead of h2a
        h2b, _ = self._insert_entry("e2b", "agent-3", "fork-b", "{}", h1)

        # Standard mode allows valid DAG forks
        success_dag, _, stats_dag = verify_audit_trail(self.db_path, strict_continuity=False)
        self.assertTrue(success_dag)

        # Strict mode detects continuity break between adjacent rows e2a and e2b
        success_strict, message, stats_strict = verify_audit_trail(self.db_path, strict_continuity=True)
        self.assertFalse(success_strict)
        self.assertEqual(stats_strict["continuity_breaks"], 1)


if __name__ == "__main__":
    unittest.main()
