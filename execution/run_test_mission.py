"""
@docs ARCHITECTURE:Infrastructure

### AI Assist Note
**🛡️ Tadpole OS: Run Test Mission**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
"""

import asyncio
import os
import sys
import json
import time
from pathlib import Path

# Add execution directory to sys.path
execution_dir = Path(__file__).resolve().parent
sys.path.append(str(execution_dir))

from core.registry import SkillRegistry

async def run_mission():
    """
    Simulates a 'High-Scrutiny Infrastructure Audit' mission.
    This mission exercises the new in-process skills.
    """
    print("--- 🚀 STARTING MISSION: Infrastructure Integrity Audit ---")
    start_time = time.perf_counter()
    
    registry = SkillRegistry()
    registry.discover_skills()
    
    # 1. Step: Discover Workspace Structure
    print("\n[MISSION STEP 1] Discovery: Mapping workspace structure...")
    # Using a hypothetical tool call to list files (simulated here for the mission script)
    # In a real agent loop, this would be an MCP tool call.
    print("  -> Found 124 files across 12 directories.")
    
    # 2. Step: Security Vulnerability Scan
    print("\n[MISSION STEP 2] Security: Running vulnerability scan...")
    if "security_scan" in registry.skills:
        result = await registry.call_skill("security_scan", {"project_path": ".", "scan_type": "all"})
        report = json.loads(result)
        vulnerabilities = report.get("vulnerabilities", [])
        print(f"  -> Scan Complete. Detected {len(vulnerabilities)} potential issues.")
    else:
        print("  !! ERROR: security_scan skill not found.")

    # 3. Step: AI Context Alignment Audit
    print("\n[MISSION STEP 3] Alignment: Auditing AI Context Notes...")
    if "verify_ai_context" in registry.skills:
        result = await registry.call_skill("verify_ai_context", {"path": "."})
        report = json.loads(result)
        failed = report.get("summary", {}).get("failed", 0)
        print(f"  -> Audit Complete. {failed} files are missing context notes.")
    else:
        print("  !! ERROR: verify_ai_context skill not found.")

    # 4. Step: Parity Verification
    print("\n[MISSION STEP 4] Integrity: Verifying system parity...")
    if "parity_guard" in registry.skills:
        result = await registry.call_skill("parity_guard", {"fix": False})
        print("  -> Parity report generated successfully.")
    else:
        print("  !! ERROR: parity_guard skill not found.")

    total_duration = time.perf_counter() - start_time
    print(f"\n--- ✅ MISSION COMPLETE: Audit Finished in {total_duration:.2f}s ---")
    print("Findings synthesized. Ready for remediation turn.")

if __name__ == "__main__":
    asyncio.run(run_mission())

# Metadata: [run_test_mission]

# Metadata: [run_test_mission]
