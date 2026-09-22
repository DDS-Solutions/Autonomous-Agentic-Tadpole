"""
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**Core technical resource for the Tadpole OS Sovereign infrastructure.**
Advanced agentic logic and tool orchestration for the Tadpole OS swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Script error, API failure, or logic drift in the 3-layer architecture.
- **Telemetry Link**: Search `[audit_manual]` in system logs.
"""

import os
import json
import re
from pathlib import Path

def audit_manual():
    script_dir = Path(__file__).parent.absolute()
    workspace_root = script_dir.parent
    manual_path = workspace_root / "docs/OPERATIONS_MANUAL.md"
    
    if not manual_path.exists():
        print(f"Error: Manual not found at {manual_path}")
        return

    manual_content = manual_path.read_text(encoding="utf-8")
    
    # 1. Discover Skills
    skills = []
    candidate_native_dirs = [
        workspace_root / "data/skills",
        workspace_root / "execution/skills",
        workspace_root / "server-rs/data/skills",
    ]
    for native_dir in candidate_native_dirs:
        if native_dir.exists():
            for d in native_dir.iterdir():
                if d.is_dir() and (d / "skill.json").exists():
                    skills.append(d.name)
    
    # Script manifests in execution/*.json
    execution_dir = workspace_root / "execution"
    if execution_dir.exists():
        for f in execution_dir.glob("*.json"):
            try:
                data = json.loads(f.read_text(encoding="utf-8"))
                if isinstance(data, dict) and "execution_command" in data:
                    skills.append(data.get("name", f.stem))
            except Exception:
                pass
        
    # 2. Check for Skill mentions in manual
    missing_skills = []
    for skill in set(skills):
        if skill not in manual_content:
            missing_skills.append(skill)
            
    # 3. Check for active operational sections in docs/OPERATIONS_MANUAL.md
    sections = [
        "Local Startup",
        "Engine Lifecycle",
        "Authentication",
        "Dashboard Operations",
        "Agent And Swarm Management",
        "Oversight And Governance",
        "Model And Provider Management",
        "Skills, Hybrid RAG, And Execution",
        "Continuity Jobs",
        "Observability",
        "Code Intelligence & Blast Radius Operations",
        "Database Operations",
        "Rollback & Recovery Automation",
    ]
    missing_sections = [s for s in sections if f"## {s}" not in manual_content]
    
    # 4. Generate Report
    report = [
        "> [!IMPORTANT]",
        "> **AI Assist Note (Knowledge Heritage)**:",
        "> - **@docs OPERATIONS_MANUAL:Runbooks**",
        "> - **Failure Path**: Documentation drift or missing operational runbooks.",
        "> - **Telemetry Link**: Search `[DOC_DRIFT]` in audit logs.",
        ">",
        "> ### AI Assist Note",
        "> Operational drift assessment for Tadpole OS manuals.",
        ">",
        "> ### 🔍 Debugging & Observability",
        "> Traceability via `execution/audit_manual.py`.",
        "",
        "# Documentation Drift Report",
        "",
        "## Summary",
        f"Audited: `OPERATIONS_MANUAL.md`",
        f"Date: 2026-09-22",
        "",
        "## Gaps Found",
        "",
        "### Missing Skill Documentation",
        "The following active skills are registered in the OS but NOT mentioned in the manual:"
    ]
    
    if missing_skills:
        for s in missing_skills:
            report.append(f"- [ ] `{s}`")
    else:
        report.append("- No missing skills found.")
        
    report.append("")
    report.append("### Missing Architectural Sections")
    if missing_sections:
        for s in missing_sections:
            report.append(f"- [ ] `## {s}`")
    else:
        report.append("- All core sections present.")
        
    report.append("")
    report.append("[//]: # (Metadata: [DOC_DRIFT])")
        
    report_path = workspace_root / "docs/documentation_drift_report.md"
    report_path.write_text("\n".join(report), encoding="utf-8")
    
    print(f"Report generated at {report_path}")

if __name__ == "__main__":
    audit_manual()

# Metadata: [audit_manual]
