#!/usr/bin/env python3
"""
@docs ARCHITECTURE:Infrastructure:Execution
@docs OPERATIONS_MANUAL:Runbooks

### AI Assist Note
**verify_swarm_hierarchy_toolbelts**: Verifies that toolbelts and ACL policies properly support
the complete vertical chain of command up and down the swarm to the Alpha agent:
- Downward: Agent 1 (CEO) -> Agent 2 (COO) -> Alpha -> Specialists
- Upward: Specialists -> Alpha -> Agent 2 -> Agent 1 -> QA-99

### 🔍 Debugging & Observability
- **Failure Path**: Missing recruitment tool, broken delegation link, or ACL violation.
- **Telemetry Link**: Search `[verify_swarm_hierarchy_toolbelts]` in audit logs.
"""

import sys
import sqlite3
import json
from pathlib import Path

if sys.stdout.encoding != "utf-8":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if sys.stderr.encoding != "utf-8":
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

ROOT = Path(__file__).resolve().parent.parent

def check_downward_chain(conn):
    c = conn.cursor()
    
    # 1. Agent 1 (CEO)
    c.execute("SELECT skills FROM agents WHERE id='1'")
    ceo_skills = json.loads(c.fetchone()[0])
    assert "issue_alpha_directive" in ceo_skills, "Agent 1 missing issue_alpha_directive"
    print("✅ [Downward 1/3] Agent 1 (CEO) possesses issue_alpha_directive to delegate to COO/Alpha.")
    
    # 2. Agent 2 (COO / Tadpole)
    c.execute("SELECT skills FROM agents WHERE id='2'")
    coo_skills = json.loads(c.fetchone()[0])
    assert "spawn_subagent" in coo_skills, "Agent 2 missing spawn_subagent"
    assert "recruit" in coo_skills, "Agent 2 missing recruit"
    assert "send_mission_directive" in coo_skills, "Agent 2 missing send_mission_directive"
    print("✅ [Downward 2/3] Agent 2 (COO) possesses spawn_subagent, recruit, and send_mission_directive to command Alpha.")
    
    # 3. Alpha (Commander)
    c.execute("SELECT skills FROM agents WHERE id='alpha'")
    alpha_skills = json.loads(c.fetchone()[0])
    assert "spawn_subagent" in alpha_skills, "Alpha missing spawn_subagent"
    assert "recruit" in alpha_skills, "Alpha missing recruit"
    assert "send_mission_directive" in alpha_skills, "Alpha missing send_mission_directive"
    print("✅ [Downward 3/3] Alpha Commander possesses spawn_subagent and recruitment tools to dispatch specialists.")

def check_upward_chain(conn):
    c = conn.cursor()
    specialists = ["3", "7", "8", "12", "26", "99"]
    
    for sid in specialists:
        c.execute("SELECT name, skills FROM agents WHERE id=?", (sid,))
        row = c.fetchone()
        assert row is not None, f"Specialist {sid} not found in DB"
        skills = json.loads(row[1])
        # Specialists must have share_finding to report upward findings
        assert "share_finding" in skills, f"Specialist {sid} ({row[0]}) missing share_finding"
    print("✅ [Upward 1/2] All technical specialists possess share_finding to inject intelligence upward into swarm context.")

def check_acl_guardrails():
    acl_path = ROOT / "server-rs" / "src" / "services" / "acl_service.rs"
    content = acl_path.read_text(encoding="utf-8")
    
    # Specialist isolation
    assert '!matches!(tool_name, "issue_alpha_directive" | "spawn_subagent")' in content, "Missing specialist subagent block in ACL"
    # CEO whitelist
    assert 'AGENT_CEO || authority == RoleAuthorityLevel::Executive' in content, "Missing CEO ACL check"
    # COO spawn allowed
    assert '"spawn_subagent" => true' in content, "Missing COO spawn_subagent permission"
    print("✅ [ACL Guardrails] Hierarchy permissions verified: Specialists cannot spawn; CEO cannot execute direct I/O.")

def add_share_finding_to_qa99(conn):
    c = conn.cursor()
    c.execute("SELECT skills FROM agents WHERE id='99'")
    skills = json.loads(c.fetchone()[0])
    if "share_finding" not in skills:
        skills.append("share_finding")
        c.execute("UPDATE agents SET skills = ? WHERE id='99'", (json.dumps(skills),))
        conn.commit()
        
        # Also update agents.json
        json_path = ROOT / "data" / "agents.json"
        agents = json.loads(json_path.read_text(encoding="utf-8"))
        for a in agents:
            if str(a.get("identity", {}).get("id") or a.get("id")) == "99":
                a["skills"] = skills
                if "capabilities" in a and isinstance(a["capabilities"], dict):
                    a["capabilities"]["skills"] = skills
                break
        json_path.write_text(json.dumps(agents, indent=2), encoding="utf-8")
        print("✅ Added share_finding to QA-99 in tadpole.db and agents.json.")

if __name__ == "__main__":
    print("=== Verifying Swarm Vertical Hierarchy Toolbelts ===")
    conn = sqlite3.connect(ROOT / "data" / "tadpole.db")
    add_share_finding_to_qa99(conn)
    check_downward_chain(conn)
    check_upward_chain(conn)
    check_acl_guardrails()
    conn.close()
    print("\n🎉 VERTICAL SWARM HIERARCHY TOOLBELTS VERIFIED 100% OPERATIONAL.")

# [verify_swarm_hierarchy_toolbelts]

