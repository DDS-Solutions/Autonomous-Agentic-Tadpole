"""
@docs ARCHITECTURE:Continuity:Workflows

### AI Assist Note
**⚙️ Tadpole OS: Seed Continuity Workflows**
Seeds standard multi-step agent workflows (including maintenance and audit pipelines)
into the SQLite workflows and workflow_steps tables in tadpole.db.

### 🔍 Debugging & Observability
- **Failure Path**: Missing agent_id foreign key, locked database, or duplicate step_order constraint.
- **Telemetry Link**: Search `[seed_continuity_workflows]` in audit logs.
"""

import json
import sqlite3
import sys
from datetime import datetime, timezone
from pathlib import Path

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass

ROOT = Path(__file__).resolve().parent.parent
DB_PATH = ROOT / "data" / "tadpole.db"

WORKFLOWS = [
    {
        "id": "wf-parity-audit",
        "name": "System & Code Parity Audit",
        "description": "Automated architecture, security drift, and code integrity audit across system surfaces.",
        "category": "audit",
        "steps": [
            {
                "id": "step-parity-1",
                "agent_id": "12",  # Sec-1 (Security Auditor)
                "step_order": 1,
                "name": "Security Surface Scan",
                "prompt_template": "Scan codebase endpoints, secret patterns, and boundary invariants."
            },
            {
                "id": "step-parity-2",
                "agent_id": "26",  # Checkmate (Quality Auditor)
                "step_order": 2,
                "name": "Parity Guard Verification",
                "prompt_template": "Execute parity guard checks against docs/architecture and report deviations."
            },
            {
                "id": "step-parity-3",
                "agent_id": "6",   # Ada (Product Lead)
                "step_order": 3,
                "name": "Audit Report Synthesis",
                "prompt_template": "Synthesize audit findings into an actionable executive remediation briefing."
            }
        ]
    },
    {
        "id": "wf-db-maintenance",
        "name": "Database & Storage Maintenance",
        "description": "Scheduled database optimization, checkpoint consolidation, and storage cleanup.",
        "category": "maintenance",
        "steps": [
            {
                "id": "step-db-1",
                "agent_id": "13",  # Ops-1 (DevOps)
                "step_order": 1,
                "name": "WAL Checkpoint & Vacuum",
                "prompt_template": "Consolidate SQLite WAL checkpoints and execute vacuum defragmentation."
            },
            {
                "id": "step-db-2",
                "agent_id": "1",   # Agent of Nine (Performance Architect)
                "step_order": 2,
                "name": "Storage & Index Optimization",
                "prompt_template": "Analyze table index utilization, query latency, and data file integrity."
            },
            {
                "id": "step-db-3",
                "agent_id": "7",   # Grace (DevOps)
                "step_order": 3,
                "name": "System Health Verification",
                "prompt_template": "Verify SQLite connection pool latency and publish maintenance telemetry pulse."
            }
        ]
    },
    {
        "id": "wf-swarm-sec-sweep",
        "name": "Swarm Security Sweep",
        "description": "Multi-agent credential exposure, permission policy, and endpoint penetration scan.",
        "category": "audit",
        "steps": [
            {
                "id": "step-sec-1",
                "agent_id": "12",  # Sec-1 (Security Auditor)
                "step_order": 1,
                "name": "Secret & Credential Scan",
                "prompt_template": "Audit active configuration files and memory logs for unredacted secrets or keys."
            },
            {
                "id": "step-sec-2",
                "agent_id": "99",  # QA-99 (Quality Auditor)
                "step_order": 2,
                "name": "Permission Boundary Audit",
                "prompt_template": "Verify agent permission policies and endpoint authorization limits for privilege escalation."
            },
            {
                "id": "step-sec-3",
                "agent_id": "alpha",  # alpha (Swarm Mission Commander)
                "step_order": 3,
                "name": "Security Clearance Sign-Off",
                "prompt_template": "Review swarm audit logs, update threat score, and certify system security posture."
            }
        ]
    },
    {
        "id": "wf-ci-pipeline",
        "name": "Continuous Integration & Parity Pipeline",
        "description": "Automated verification pipeline for type safety, invariant gates, and unit test coverage.",
        "category": "maintenance",
        "steps": [
            {
                "id": "step-ci-1",
                "agent_id": "8",   # Linus (Backend Dev)
                "step_order": 1,
                "name": "Static Type & Compilation Gate",
                "prompt_template": "Run cargo check and tsc type verification across rust and frontend packages."
            },
            {
                "id": "step-ci-2",
                "agent_id": "26",  # Checkmate (Quality Auditor)
                "step_order": 2,
                "name": "Invariant Test Suite Run",
                "prompt_template": "Execute system invariant test suites and assert zero functional regressions."
            },
            {
                "id": "step-ci-3",
                "agent_id": "3",   # Elon (CTO)
                "step_order": 3,
                "name": "Release Readiness Certification",
                "prompt_template": "Evaluate test telemetry and certify release readiness for production deployment."
            }
        ]
    },
    {
        "id": "wf-memory-prune",
        "name": "Daily Context & Memory Pruning",
        "description": "Routine cleanup of stale cache artifacts, session memory compression, and scratch rotation.",
        "category": "maintenance",
        "steps": [
            {
                "id": "step-mem-1",
                "agent_id": "1",   # Agent of Nine (Performance Architect)
                "step_order": 1,
                "name": "Stale Artifact Inspection",
                "prompt_template": "Identify orphaned temporary scratch files, stale memory caches, and large log files."
            },
            {
                "id": "step-mem-2",
                "agent_id": "13",  # Ops-1 (DevOps)
                "step_order": 2,
                "name": "Scratch File Cleanup",
                "prompt_template": "Safely purge temporary files older than retention policy and rotate debug logs."
            },
            {
                "id": "step-mem-3",
                "agent_id": "6",   # Ada (Product Lead)
                "step_order": 3,
                "name": "Maintenance Log Summary",
                "prompt_template": "Publish storage reclamation metrics and update system operational dashboard."
            }
        ]
    }
]

def seed_workflows():
    if not DB_PATH.exists():
        print(f"🚨 [Error] Database not found at {DB_PATH}")
        sys.exit(1)

    conn = sqlite3.connect(DB_PATH)
    conn.execute("PRAGMA foreign_keys = ON")
    cursor = conn.cursor()

    now_iso = datetime.now(timezone.utc).isoformat()

    print("=== Seeding Continuity Workflows into tadpole.db ===")
    for wf in WORKFLOWS:
        # Upsert workflow
        cursor.execute("""
            INSERT INTO workflows (id, name, description, enabled, created_at, updated_at, metadata, category)
            VALUES (?, ?, ?, 1, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                updated_at = excluded.updated_at,
                category = excluded.category
        """, (
            wf["id"],
            wf["name"],
            wf["description"],
            now_iso,
            now_iso,
            json.dumps({"seeded": True, "category": wf["category"]}),
            wf["category"]
        ))
        print(f"  [✓] Workflow: {wf['name']} ({wf['id']}) [{wf['category'].upper()}]")

        # Upsert steps
        for step in wf["steps"]:
            cursor.execute("""
                INSERT INTO workflow_steps (id, workflow_id, agent_id, step_order, name, prompt_template, config)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    agent_id = excluded.agent_id,
                    step_order = excluded.step_order,
                    name = excluded.name,
                    prompt_template = excluded.prompt_template
            """, (
                step["id"],
                wf["id"],
                step["agent_id"],
                step["step_order"],
                step["name"],
                step["prompt_template"],
                json.dumps({})
            ))
            print(f"       -> Step {step['step_order']}: {step['name']} (Agent: {step['agent_id']})")

    conn.commit()

    print("\n=== Summary of Workflows in tadpole.db ===")
    cursor.execute("""
        SELECT w.id, w.name, w.category, COUNT(s.id) as step_count
        FROM workflows w
        LEFT JOIN workflow_steps s ON w.id = s.workflow_id
        GROUP BY w.id
        ORDER BY w.name ASC
    """)
    rows = cursor.fetchall()
    for r in rows:
        print(f"  - [{r[2]}] {r[1]} ({r[0]}): {r[3]} steps configured")

    conn.close()
    print("\n[✓] Seeding completed successfully.")

if __name__ == "__main__":
    seed_workflows()

# [seed_continuity_workflows]
