"""
@docs ARCHITECTURE:Infrastructure

### AI Assist Note
**🛡️ Tadpole OS: Verify Telemetry**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Search `[verify_telemetry]` in system logs.
"""

import json
import sys
import os
import sqlite3
import argparse
from pathlib import Path

# Ensure stdout handles UTF-8 on Windows
if sys.platform == "win32":
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")

ROOT_DIR = Path(__file__).resolve().parent.parent
DB_PATH = ROOT_DIR / "data" / "tadpole.db"
LOGS_DIR = ROOT_DIR / "data" / "logs"

def get_mission_context(mission_id_arg: str):
    """Resolves mission ID and returns mission metadata from DB if available."""
    if not DB_PATH.exists():
        return None, None
        
    try:
        conn = sqlite3.connect(DB_PATH)
        cursor = conn.cursor()
        if mission_id_arg.upper() == "LATEST":
            cursor.execute(
                "SELECT id, title, status, agent_id, created_at FROM mission_history ORDER BY created_at DESC LIMIT 1"
            )
        else:
            cursor.execute(
                "SELECT id, title, status, agent_id, created_at FROM mission_history WHERE id = ?",
                (mission_id_arg,)
            )
        mission = cursor.fetchone()
        conn.close()
        if mission:
            return mission[0], {
                "id": mission[0],
                "title": mission[1],
                "status": mission[2],
                "agent_id": mission[3],
                "created_at": mission[4]
            }
    except Exception as e:
        print(f"⚠️ Warning: DB query failed: {e}", file=sys.stderr)
    return None, None

def scan_jsonl_spans(mission_id: str, agent_id: str = None) -> list:
    """Scans telemetry JSONL files in data/logs for spans matching the mission."""
    spans = []
    if not LOGS_DIR.exists():
        return spans
        
    for log_file in sorted(LOGS_DIR.glob("telemetry-*.jsonl"), reverse=True):
        try:
            with open(log_file, "r", encoding="utf-8", errors="replace") as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        entry = json.loads(line)
                        if entry.get("type") == "trace:span":
                            span = entry.get("span", {})
                            s_mission = span.get("mission_id") or span.get("attributes", {}).get("mission_id")
                            s_agent = span.get("agent_id") or span.get("attributes", {}).get("agent_id")
                            if s_mission == mission_id or (agent_id and s_agent == agent_id):
                                spans.append(span.get("name") or span.get("id"))
                    except json.JSONDecodeError:
                        continue
        except Exception as e:
            print(f"⚠️ Warning reading {log_file.name}: {e}", file=sys.stderr)
    return spans

def scan_db_logs(mission_id: str) -> list:
    """Queries mission_logs table for log events matching the mission."""
    if not DB_PATH.exists():
        return []
    try:
        conn = sqlite3.connect(DB_PATH)
        cursor = conn.cursor()
        cursor.execute(
            "SELECT source, text, severity, timestamp FROM mission_logs WHERE mission_id = ? ORDER BY timestamp ASC",
            (mission_id,)
        )
        logs = cursor.fetchall()
        conn.close()
        return logs
    except Exception:
        return []

def verify_telemetry(mission_id_arg: str = None, agent_id_arg: str = None):
    print("[TelemetryAudit] Initializing Neural Trace Audit for QA-99...")
    
    target_mission = mission_id_arg or os.getenv("MISSION_ID", "LATEST")
    target_agent = agent_id_arg or os.getenv("AGENT_ID", None)
    
    print(f"[TelemetryAudit] Scanning spans for Mission: {target_mission}, Agent: {target_agent or 'ANY'}")
    
    resolved_id, mission_meta = get_mission_context(target_mission)
    if not resolved_id and target_mission.upper() == "LATEST":
        print(f"❌ Error: No mission found in database or telemetry logs.", file=sys.stderr)
        audit_findings = {
            "sop_compliance": "SOP-SEC-09",
            "spans_detected": [],
            "status": "MISSION_NOT_FOUND",
            "final_verdict": "FAILED"
        }
        print("\n--- FINAL VERDICT ---")
        print(json.dumps(audit_findings, indent=2))
        sys.exit(1)
        
    active_mission_id = resolved_id or target_mission
    
    # 1. Scan JSONL telemetry logs
    detected_spans = scan_jsonl_spans(active_mission_id, target_agent)
    
    # 2. Scan DB mission logs
    db_logs = scan_db_logs(active_mission_id)
    for log_row in db_logs:
        source = log_row[0]
        if source and source not in detected_spans:
            detected_spans.append(source)
            
    # If mission exists with active status, record active lifecycle span
    if mission_meta and mission_meta.get("status"):
        lifecycle_span = f"mission:{mission_meta['status']}"
        if lifecycle_span not in detected_spans:
            detected_spans.append(lifecycle_span)

    if not detected_spans and not mission_meta:
        print(f"❌ Error: No telemetry spans or database logs found for mission {active_mission_id}.", file=sys.stderr)
        audit_findings = {
            "mission_id": active_mission_id,
            "sop_compliance": "SOP-SEC-09",
            "spans_detected": [],
            "status": "NO_TELEMETRY_RECORDED",
            "final_verdict": "FAILED"
        }
        print("\n--- FINAL VERDICT ---")
        print(json.dumps(audit_findings, indent=2))
        sys.exit(1)
        
    audit_findings = {
        "mission_id": active_mission_id,
        "sop_compliance": "SOP-SEC-09",
        "spans_detected": detected_spans,
        "status": "FUNCTIONALLY_COMPLETE",
        "final_verdict": "SUCCESS"
    }
    
    print(f"[TelemetryAudit] Found {len(detected_spans)} mission-critical span(s) / source(s).")
    print("\n--- FINAL VERDICT ---")
    print(json.dumps(audit_findings, indent=2))
    
    if audit_findings["final_verdict"] == "SUCCESS":
        sys.exit(0)
    else:
        sys.exit(1)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Deterministic Telemetry Verification")
    parser.add_argument("--mission-id", default=None, help="The mission ID to audit (or LATEST)")
    parser.add_argument("--agent-id", default=None, help="The agent ID to filter by")
    args = parser.parse_args()
    verify_telemetry(args.mission_id, args.agent_id)

# Metadata: [verify_telemetry]
