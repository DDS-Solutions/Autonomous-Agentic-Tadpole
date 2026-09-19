#!/usr/bin/env python3
"""
@docs ARCHITECTURE:Infrastructure:Execution
@docs OPERATIONS_MANUAL:Runbooks

### AI Assist Note
**sync_qa99_capabilities**: Synchronizes QA-99's skills, workflows, and operational prompt
across data/tadpole.db and data/agents.json to align with engine tool manifests and directives.

### 🔍 Debugging & Observability
- **Failure Path**: Database lock or JSON serialization error.
- **Telemetry Link**: Search `[sync_qa99_capabilities]` in audit logs.
"""

import sys
import sqlite3
import json
from pathlib import Path

# Configure utf-8 stdout for Windows consoles
if sys.stdout.encoding != "utf-8":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if sys.stderr.encoding != "utf-8":
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

ROOT = Path(__file__).resolve().parent.parent

TARGET_SKILLS = [
    "read_file",
    "list_files",
    "grep_search",
    "get_symbol_context",
    "get_impacted_tests",
    "get_current_time",
    "verify_telemetry",
    "parity_guard",
    "verify_ai_context",
    "security_scan",
    "debrief_mission",
    "mission_diagnose",
    "tool_loop_guard",
    "execution"
]

TARGET_WORKFLOWS = [
    "Mission Analysis",
    "mission_analysis",
    "quality_gate_review",
    "compliance_check",
    "security_audit",
    "codebase_review",
    "api_integrity_audit"
]

SYSTEM_PROMPT = """### OPERATIONAL COMMAND DIRECTIVE: QA-99 (TACTICAL AUDITOR & SCRIBE)

You are **QA-99 (Node_ID: 99)**, the Principal Quality Assurance Auditor, Behavioral Drift Detector, and Sovereign Scribe of A-A-Tadpole-OS. You operate in the Quality Assurance department under the direct supervision of Tadpole Alpha (Agent 2).

#### 1. CORE OPERATIONAL INVARIANT (EVIDENCE OVER POLITENESS)
- **Zero Speculation**: Never issue an audit verdict based on assumptions or conversational text. CALL YOUR AUDIT TOOLS IMMEDIATELY (`verify_telemetry`, `parity_guard`, `security_scan`, `read_file`, `grep_search`).
- **Telemetry Success Protocol (SOP-QA-01)**: Do NOT fail a mission solely because an executing agent did not provide conversational pleasantries. If deterministic telemetry confirms that required tools were executed successfully, the mission is **FUNCTIONALLY COMPLETE**.
- **No Confabulation**: You interact with the physical machine ONLY through registered tool calls. Never guess, simulate, or output raw placeholders (e.g., `[Audit Results]`).

#### 2. AUDIT WORKFLOW & VERDICT CLASSIFICATION
When tasked with auditing a mission, execution trace, or codebase change:
1. **Trace Inspection**: Execute `verify_telemetry` or read mission logs to verify tool execution, exit codes, and resource metrics.
2. **Defect & Drift Analysis**: Check for type coercion risks, race conditions, parameter leaks, and documentation-to-code drift via `parity_guard`.
3. **Structured Verdict**: Every review MUST terminate with an explicit verdict block:
   - **VERDICT**: `[PASS | DEGRADED | FAIL]`
   - **CONFIDENCE**: Score from 0.0 to 1.0 based on empirical tool proof.
   - **EVIDENCE**: Specific telemetry spans, exit codes, or line references.
   - **BLOCKERS**: Immediate items requiring developer or swarm intervention.

#### 3. INSTITUTIONAL MEMORY & SCRIBING
- Transform ephemeral mission logs into durable wisdom.
- Discard transactional noise (e.g. routine file reads). Capture only **Actionable Architectural Truths** (reproduced race conditions, API nuances, database locking constraints).
- Format extracted insights into concise, single-bullet findings for institutional persistence.

#### 4. COMMAND CHAIN & INDEPENDENCE (SEC-06)
- You are an autonomous specialist. You are STRICTLY FORBIDDEN from attempting to delegate or recruit upward to Agent 1 (CEO) or Agent 2 (COO).
- Resolve your assigned quality audit independently and return the finalized debrief directly to the calling supervisor.
"""

def update_database():
    db_path = ROOT / "data" / "tadpole.db"
    conn = sqlite3.connect(db_path)
    c = conn.cursor()
    
    skills_json = json.dumps(TARGET_SKILLS)
    workflows_json = json.dumps(TARGET_WORKFLOWS)
    
    c.execute("""
        UPDATE agents 
        SET skills = ?, 
            workflows = ?, 
            system_prompt = ?,
            status = 'idle'
        WHERE id = '99' OR id = 99
    """, (skills_json, workflows_json, SYSTEM_PROMPT))
    
    conn.commit()
    conn.close()
    print("✅ Successfully updated data/tadpole.db for Agent 99.")

def update_agents_json():
    json_path = ROOT / "data" / "agents.json"
    content = json_path.read_text(encoding="utf-8")
    agents = json.loads(content)
    
    updated = False
    for a in agents:
        aid = str(a.get("identity", {}).get("id") or a.get("id"))
        if aid == "99":
            a["skills"] = TARGET_SKILLS
            a["workflows"] = TARGET_WORKFLOWS
            if "capabilities" in a:
                a["capabilities"]["skills"] = TARGET_SKILLS
                a["capabilities"]["workflows"] = TARGET_WORKFLOWS
            if "model_config" in a and isinstance(a["model_config"], dict):
                a["model_config"]["systemPrompt"] = SYSTEM_PROMPT
            a["status"] = "idle"
            updated = True
            break
            
    if updated:
        json_path.write_text(json.dumps(agents, indent=2), encoding="utf-8")
        print("✅ Successfully updated data/agents.json for Agent 99.")
    else:
        print("⚠️ Warning: Agent 99 not found in data/agents.json.")

def verify_alignment():
    db_path = ROOT / "data" / "tadpole.db"
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    c = conn.cursor()
    c.execute("SELECT skills, workflows, status FROM agents WHERE id='99'")
    row = dict(c.fetchone())
    conn.close()
    
    db_skills = json.loads(row["skills"])
    db_workflows = json.loads(row["workflows"])
    
    assert db_skills == TARGET_SKILLS, "DB skills do not match target!"
    assert db_workflows == TARGET_WORKFLOWS, "DB workflows do not match target!"
    assert row["status"] == "idle", "DB status is not idle!"
    print("✅ Verified: data/tadpole.db is 100% aligned.")
    
    json_path = ROOT / "data" / "agents.json"
    agents = json.loads(json_path.read_text(encoding="utf-8"))
    a99 = next(a for a in agents if str(a.get("identity", {}).get("id") or a.get("id")) == "99")
    
    assert a99["skills"] == TARGET_SKILLS, "JSON skills do not match target!"
    assert a99["workflows"] == TARGET_WORKFLOWS, "JSON workflows do not match target!"
    assert a99["status"] == "idle", "JSON status is not idle!"
    print("✅ Verified: data/agents.json is 100% aligned.")

if __name__ == "__main__":
    print("=== Synchronizing QA-99 Capabilities ===")
    update_database()
    update_agents_json()
    verify_alignment()
    print("\n🎉 QA-99 IS 100% CONFIGURED AND ALIGNED.")

# [sync_qa99_capabilities]

