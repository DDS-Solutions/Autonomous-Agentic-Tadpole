#!/usr/bin/env python3
"""
@docs ARCHITECTURE:Infrastructure:Execution
@docs OPERATIONS_MANUAL:Runbooks

### AI Assist Note
**sync_all_upstream_toolbelts**: Synchronizes the capabilities, skills, and workflows of all
upstream agents (Tadpole COO, Alpha, Elon, Grace, Linus, Sec-1, Checkmate, Steve, Res-1)
across data/tadpole.db and data/agents.json. Eliminates phantom skills and aligns with
engine manifests and directives.

### 🔍 Debugging & Observability
- **Failure Path**: Database write lock or JSON desynchronization.
- **Telemetry Link**: Search `[sync_all_upstream_toolbelts]` in audit logs.
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

UPSTREAM_ALIGNMENT = {
    "2": {
        "name": "Tadpole",
        "skills": [
            "spawn_subagent",
            "recruit",
            "complete_mission",
            "read_file",
            "write_file",
            "list_files",
            "grep_search",
            "get_current_time",
            "fetch_url",
            "search_web",
            "share_finding",
            "send_mission_directive",
            "search_mission_knowledge",
            "update_working_memory",
            "execution"
        ],
        "workflows": [
            "resource_allocation",
            "ops_review",
            "orchestrate",
            "ci_cd_pipeline",
            "sprint_planning",
            "codebase_health_mission"
        ]
    },
    "alpha": {
        "name": "alpha",
        "skills": [
            "spawn_subagent",
            "recruit",
            "complete_mission",
            "read_file",
            "write_file",
            "list_files",
            "delete_file",
            "grep_search",
            "get_symbol_context",
            "get_impacted_tests",
            "get_current_time",
            "fetch_url",
            "search_web",
            "share_finding",
            "send_mission_directive",
            "execution"
        ],
        "workflows": [
            "orchestrate",
            "codebase_health_mission",
            "quality_gate_review",
            "ci_cd_pipeline"
        ]
    },
    "3": {
        "name": "Elon",
        "skills": [
            "read_file",
            "write_file",
            "list_files",
            "grep_search",
            "get_symbol_context",
            "get_impacted_tests",
            "get_current_time",
            "share_finding",
            "parity_guard",
            "verify_ai_context",
            "cargo_fast_check",
            "execution"
        ],
        "workflows": [
            "system_architecture_review",
            "codebase_review",
            "incident_response",
            "refactor_microservice",
            "quality_gate_review"
        ]
    },
    "7": {
        "name": "Grace",
        "skills": [
            "read_file",
            "list_files",
            "grep_search",
            "get_agent_metrics",
            "get_current_time",
            "share_finding",
            "db_health_check",
            "server_tail",
            "clean_telemetry_logs",
            "cargo_fast_check",
            "execution"
        ],
        "workflows": [
            "pipeline_optimization",
            "database_migration",
            "server_tailing",
            "ci_cd_pipeline",
            "scale_cluster"
        ]
    },
    "8": {
        "name": "Linus",
        "skills": [
            "read_file",
            "write_file",
            "list_files",
            "grep_search",
            "get_symbol_context",
            "get_impacted_tests",
            "get_current_time",
            "share_finding",
            "fetch_url",
            "db_health_check",
            "backup_sqlite",
            "cargo_fast_check",
            "execution"
        ],
        "workflows": [
            "refactor_microservice",
            "api_documentation",
            "api_integrity_audit",
            "database_migration",
            "ci_cd_pipeline"
        ]
    },
    "12": {
        "name": "Sec-1",
        "skills": [
            "read_file",
            "list_files",
            "grep_search",
            "get_current_time",
            "share_finding",
            "security_scan",
            "mcp_audit",
            "parity_guard",
            "verify_ai_context",
            "execution"
        ],
        "workflows": [
            "security_audit",
            "compliance_check",
            "risk_assessment"
        ]
    },
    "26": {
        "name": "Checkmate",
        "skills": [
            "read_file",
            "list_files",
            "grep_search",
            "get_symbol_context",
            "get_impacted_tests",
            "get_current_time",
            "share_finding",
            "parity_guard",
            "pre_pr",
            "verify_ai_context",
            "comment_coverage",
            "execution"
        ],
        "workflows": [
            "compliance_check",
            "quality_gate_review",
            "codebase_review"
        ]
    },
    "9": {
        "name": "Steve",
        "skills": [
            "generate_image",
            "visual_inspect_ui",
            "read_file",
            "list_files",
            "get_current_time",
            "share_finding",
            "ui_integrity_audit",
            "execution"
        ],
        "workflows": [
            "design_system_update",
            "usability_testing"
        ]
    },
    "16": {
        "name": "Res-1",
        "skills": [
            "search_web",
            "fetch_url",
            "read_file",
            "list_files",
            "get_current_time",
            "search_global_vault",
            "archive_to_global_vault",
            "share_finding"
        ],
        "workflows": [
            "market_research",
            "user_feedback_analysis",
            "competitive_audit"
        ]
    }
}

def sync_database():
    db_path = ROOT / "data" / "tadpole.db"
    conn = sqlite3.connect(db_path)
    c = conn.cursor()
    
    for aid, config in UPSTREAM_ALIGNMENT.items():
        skills_json = json.dumps(config["skills"])
        workflows_json = json.dumps(config["workflows"])
        
        c.execute("""
            UPDATE agents
            SET skills = ?,
                workflows = ?
            WHERE id = ?
        """, (skills_json, workflows_json, aid))
        print(f"  [DB] Updated Agent {aid} ({config['name']}): {len(config['skills'])} skills, {len(config['workflows'])} workflows.")
        
    conn.commit()
    conn.close()
    print("✅ Successfully updated all target agents in data/tadpole.db.\n")

def sync_agents_json():
    json_path = ROOT / "data" / "agents.json"
    agents = json.loads(json_path.read_text(encoding="utf-8"))
    
    for a in agents:
        aid = str(a.get("identity", {}).get("id") or a.get("id"))
        if aid in UPSTREAM_ALIGNMENT:
            config = UPSTREAM_ALIGNMENT[aid]
            a["skills"] = config["skills"]
            a["workflows"] = config["workflows"]
            if "capabilities" in a and isinstance(a["capabilities"], dict):
                a["capabilities"]["skills"] = config["skills"]
                a["capabilities"]["workflows"] = config["workflows"]
            print(f"  [JSON] Synchronized Agent {aid} ({config['name']}).")
            
    json_path.write_text(json.dumps(agents, indent=2), encoding="utf-8")
    print("✅ Successfully updated all target agents in data/agents.json.\n")

def verify_all():
    db_path = ROOT / "data" / "tadpole.db"
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    c = conn.cursor()
    
    for aid, config in UPSTREAM_ALIGNMENT.items():
        c.execute("SELECT skills, workflows FROM agents WHERE id=?", (aid,))
        row = c.fetchone()
        assert row is not None, f"Agent {aid} not found in DB"
        db_skills = json.loads(row["skills"])
        db_wfs = json.loads(row["workflows"])
        assert db_skills == config["skills"], f"Agent {aid} skills mismatch in DB"
        assert db_wfs == config["workflows"], f"Agent {aid} workflows mismatch in DB"
    conn.close()
    
    json_path = ROOT / "data" / "agents.json"
    agents = json.loads(json_path.read_text(encoding="utf-8"))
    for aid, config in UPSTREAM_ALIGNMENT.items():
        a = next((x for x in agents if str(x.get("identity", {}).get("id") or x.get("id")) == aid), None)
        if a:
            assert a["skills"] == config["skills"], f"Agent {aid} skills mismatch in JSON"
            assert a["workflows"] == config["workflows"], f"Agent {aid} workflows mismatch in JSON"
            
    print("🎉 ALL 9 UPSTREAM AGENTS ARE 100% VERIFIED AND SYNCHRONIZED.")

if __name__ == "__main__":
    print("=== Aligning Upstream Swarm Toolbelts ===")
    sync_database()
    sync_agents_json()
    verify_all()

# [sync_all_upstream_toolbelts]

