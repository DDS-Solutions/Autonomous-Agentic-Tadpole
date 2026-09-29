"""
@docs ARCHITECTURE:Core

### AI Assist Note
**Audit Chain Verifier**: Deterministic cryptographic verification tool for the Tadpole OS
tamper-evident SHA-256 Merkle audit trail. Validates payload hash integrity and chain linkage.

### 🔍 Debugging & Observability
- **Failure Path**: Hash chain corruption, broken prev_hash link, tampered action payload.
- **Telemetry Link**: Search `[audit_chain]` in system logs.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import sqlite3
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple


def compute_audit_hash(
    prev_hash: str,
    agent_id: str,
    mission_id: Optional[str],
    user_id: Optional[str],
    action: str,
    params: str,
    timestamp: str,
) -> str:
    """Compute the expected SHA-256 hash for an audit_trail entry byte-for-byte with server-rs."""
    hasher = hashlib.sha256()
    hasher.update(prev_hash.encode("utf-8"))
    hasher.update(agent_id.encode("utf-8"))
    if mission_id:
        hasher.update(mission_id.encode("utf-8"))
    if user_id:
        hasher.update(user_id.encode("utf-8"))
    hasher.update(action.encode("utf-8"))
    hasher.update(params.encode("utf-8"))
    hasher.update(timestamp.encode("utf-8"))
    return hasher.hexdigest()


def verify_audit_trail(
    db_path: str,
    limit: Optional[int] = None,
    strict_continuity: bool = False,
) -> Tuple[bool, str, Dict[str, Any]]:
    """
    Verify the cryptographic integrity of the audit_trail table.

    Returns:
        (success: bool, summary_message: str, stats: dict)
    """
    path = Path(db_path)
    if not path.is_file():
        return False, f"Database file does not exist: {db_path}", {"error": "file_not_found"}

    try:
        conn = sqlite3.connect(str(path))
        conn.row_factory = sqlite3.Row
        cursor = conn.cursor()

        # 1. Verify table exists
        table_check = cursor.execute(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='audit_trail'"
        ).fetchone()
        if not table_check:
            conn.close()
            return False, "Table 'audit_trail' does not exist in database.", {"error": "missing_table"}

        # 2. Collect all current_hashes for DAG parent validity
        all_hashes = set(r[0] for r in cursor.execute("SELECT current_hash FROM audit_trail").fetchall())
        all_hashes.add("0" * 64)

        # 3. Fetch rows for verification
        if limit is not None and limit > 0:
            query = "SELECT * FROM audit_trail ORDER BY timestamp DESC, rowid DESC LIMIT ?"
            rows = cursor.execute(query, (limit,)).fetchall()
            rows.reverse()
        else:
            query = "SELECT * FROM audit_trail ORDER BY timestamp ASC, rowid ASC"
            rows = cursor.execute(query).fetchall()

        conn.close()
    except Exception as exc:
        return False, f"Database access error: {exc}", {"error": str(exc)}

    total_rows = len(rows)
    if total_rows == 0:
        return True, "Audit trail is empty (0 entries, genesis ready).", {"total": 0, "verified": 0}

    hash_mismatches: List[str] = []
    orphan_parents: List[str] = []
    continuity_breaks: List[str] = []
    prev_entry: Optional[sqlite3.Row] = None

    for idx, row in enumerate(rows):
        row_id = row["id"]
        prev_hash = row["prev_hash"]
        current_hash = row["current_hash"]
        agent_id = row["agent_id"]
        mission_id = row["mission_id"]
        user_id = row["user_id"]
        action = row["action"]
        params = row["params"] or ""
        timestamp = row["timestamp"]

        # A. Verify payload hash integrity
        expected_hash = compute_audit_hash(
            prev_hash=prev_hash,
            agent_id=agent_id,
            mission_id=mission_id,
            user_id=user_id,
            action=action,
            params=params,
            timestamp=timestamp,
        )
        if expected_hash != current_hash:
            hash_mismatches.append(
                f"Row {idx} ({row_id}): expected hash {expected_hash}, found {current_hash} (action={action!r})"
            )

        # B. Verify DAG parent existence
        if prev_hash not in all_hashes:
            orphan_parents.append(
                f"Row {idx} ({row_id}): prev_hash {prev_hash} not found in audit ledger (broken ancestor link)"
            )

        # C. Verify sequential continuity if requested or between adjacent entries
        if strict_continuity and prev_entry is not None:
            if prev_hash != prev_entry["current_hash"]:
                continuity_breaks.append(
                    f"Row {idx} ({row_id}): prev_hash {prev_hash} != previous entry's current_hash {prev_entry['current_hash']}"
                )

        prev_entry = row

    stats = {
        "total_checked": total_rows,
        "hash_mismatches": len(hash_mismatches),
        "orphan_parents": len(orphan_parents),
        "continuity_breaks": len(continuity_breaks),
    }

    if hash_mismatches or orphan_parents or (strict_continuity and continuity_breaks):
        errors = hash_mismatches + orphan_parents + continuity_breaks
        summary = (
            f"Audit chain verification FAILED ({len(errors)} violation(s)): "
            f"{len(hash_mismatches)} hash mismatches, "
            f"{len(orphan_parents)} orphan parents, "
            f"{len(continuity_breaks)} continuity breaks."
        )
        for err in errors[:10]:
            summary += f"\n  - {err}"
        if len(errors) > 10:
            summary += f"\n  ... and {len(errors) - 10} more"
        return False, summary, stats

    return True, f"[audit_chain] Audit chain verified successfully ({total_rows} entries checked, unbroken cryptographic integrity).", stats


def main() -> int:
    parser = argparse.ArgumentParser(description="Tadpole OS Audit Trail Cryptographic Verifier")
    parser.add_argument(
        "project_root",
        nargs="?",
        default=".",
        help="Root directory of the project (default: current directory)",
    )
    parser.add_argument(
        "--db",
        default=None,
        help="Path to SQLite database file (default: <project_root>/data/tadpole.db)",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=None,
        help="Limit verification to the last N entries (default: all entries)",
    )
    parser.add_argument(
        "--strict-continuity",
        action="store_true",
        help="Require strict 1:1 linear continuity without branching across checked entries",
    )
    args = parser.parse_args()

    db_path = args.db or os.path.join(args.project_root, "data", "tadpole.db")

    success, message, stats = verify_audit_trail(
        db_path=db_path,
        limit=args.limit,
        strict_continuity=args.strict_continuity,
    )

    if success:
        print(f"[OK] {message}")
        print(f"     [audit_chain] Total Checked: {stats.get('total_checked', 0)}")
        return 0
    else:
        print(f"[FAIL] [audit_chain] {message}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

# Metadata: [audit_chain]
