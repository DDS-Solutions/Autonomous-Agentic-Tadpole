#!/usr/bin/env python3
"""
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**Core technical resource for the Tadpole OS Sovereign infrastructure.**
Deterministic Python tool for cross-platform 7-day rolling telemetry log retention.
Prevents disk exhaustion during long-running autonomous swarm operations.

### 🔍 Debugging & Observability
Traceability via `execution/parity_guard.py`.
"""

import sys
import os
import time
import argparse
from pathlib import Path

WORKSPACE_ROOT = Path(__file__).resolve().parent.parent
LOG_DIR = WORKSPACE_ROOT / "data" / "logs"

def prune_telemetry_logs(max_days: int = 7, dry_run: bool = False) -> dict:
    """Delete JSONL telemetry files older than max_days. Fails closed on invalid days or deletion errors."""
    if max_days < 1:
        raise ValueError(f"Retention days must be at least 1, received {max_days}")

    if not LOG_DIR.exists():
        print(f"[LOG_CLEANUP] Log directory {LOG_DIR} does not exist.")
        return {"deleted": 0, "kept": 0, "freed_bytes": 0, "errors": 0}

    cutoff_seconds = time.time() - (max_days * 86400)
    deleted_count = 0
    kept_count = 0
    freed_bytes = 0
    error_count = 0

    mode_label = "[DRY-RUN] " if dry_run else ""
    print(f"[LOG_CLEANUP] {mode_label}Scanning {LOG_DIR} for telemetry logs older than {max_days} days...")

    for log_file in LOG_DIR.glob("telemetry-*.jsonl"):
        try:
            mtime = log_file.stat().st_mtime
            size = log_file.stat().st_size
            if mtime < cutoff_seconds:
                if not dry_run:
                    log_file.unlink()
                deleted_count += 1
                freed_bytes += size
                action = "[WOULD_DELETE]" if dry_run else "[DELETED]"
                print(f"  {action} {log_file.name} ({size / (1024*1024):.2f} MB)")
            else:
                kept_count += 1
                print(f"  [KEPT] {log_file.name} ({size / (1024*1024):.2f} MB)")
        except Exception as e:
            error_count += 1
            print(f"  [ERROR] Failed processing {log_file.name}: {e}", file=sys.stderr)

    result = {
        "deleted": deleted_count,
        "kept": kept_count,
        "freed_bytes": freed_bytes,
        "freed_mb": round(freed_bytes / (1024 * 1024), 2),
        "errors": error_count
    }

    action_label = "Would delete" if dry_run else "Deleted"
    print(f"[LOG_CLEANUP] Complete. {action_label} {deleted_count} files ({result['freed_mb']} MB freed). Kept {kept_count} active logs. Errors: {error_count}.")
    return result

def main():
    parser = argparse.ArgumentParser(description="Tadpole OS Telemetry Log Retention Tool")
    parser.add_argument("--days", type=int, default=7, help="Number of days of logs to retain (minimum 1, default: 7)")
    parser.add_argument("--dry-run", action="store_true", help="Simulate pruning without deleting files")
    args = parser.parse_args()

    if args.days < 1:
        print(f"❌ Error: --days must be an integer >= 1 (received {args.days})", file=sys.stderr)
        sys.exit(1)

    result = prune_telemetry_logs(args.days, dry_run=args.dry_run)
    if result["errors"] > 0:
        sys.exit(1)
    sys.exit(0)

if __name__ == "__main__":
    main()

# Metadata: [clean_telemetry_logs]
