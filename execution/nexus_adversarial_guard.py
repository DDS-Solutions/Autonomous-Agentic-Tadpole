"""
@docs ARCHITECTURE:Core

### AI Assist Note
**Nexus Adversarial Invariant Guard**
Static AST & pattern analyzer enforcing the Mandatory Dual-Pass Nexus Protocol (SOP-NEXUS-01).
Verifies scheduler invariants, relational cascade ordering, concurrency locks, and trust boundaries.

### 🔍 Debugging & Observability
- **Failure Path**: Antipattern detected in server-rs source code.
- **Telemetry Link**: Search `[nexus_adversarial_guard]` in audit logs.
"""

import sys
import re
import os
from pathlib import Path

# Ensure UTF-8 output on Windows
if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8")

def check_c1_version_clobber(persistence_code: str) -> list[str]:
    errors = []
    # Check if save_agent_db_in_tx reconciles by assigning version from db
    if "reconciled.version = db_ver" in persistence_code:
        errors.append("[C1-FAIL] Found optimistic locking clobber: 'reconciled.version = db_ver' bypasses OCC.")
    return errors

def check_c2_cascade_ordering(persistence_code: str) -> list[str]:
    errors = []
    # In delete_agent_cascade, DELETE FROM agents must come AFTER dependent mission/log deletes
    match = re.search(r"pub async fn delete_agent_cascade.*?\{(?P<body>.*?)\n\}", persistence_code, re.DOTALL)
    if match:
        body = match.group("body")
        agents_idx = body.find("DELETE FROM agents")
        mission_idx = body.find("DELETE FROM mission_history")
        logs_idx = body.find("DELETE FROM mission_logs")

        if agents_idx != -1:
            if mission_idx == -1:
                errors.append("[C2-FAIL] delete_agent_cascade does not clean 'mission_history'.")
            elif agents_idx < mission_idx:
                errors.append("[C2-FAIL] delete_agent_cascade deletes from 'agents' BEFORE 'mission_history'. Violates PRAGMA foreign_keys = ON.")
            
            if logs_idx == -1:
                errors.append("[C2-FAIL] delete_agent_cascade does not clean 'mission_logs'.")
            elif agents_idx < logs_idx:
                errors.append("[C2-FAIL] delete_agent_cascade deletes from 'agents' BEFORE 'mission_logs'.")
    return errors

def check_c4_spawn_handle_race(agent_routes_code: str) -> list[str]:
    errors = []
    # Check spawn_agent_runner for task cleanup race
    # Ensure active_runners is not inserted after tokio::spawn without synchronization
    match = re.search(r"pub\(crate\) fn spawn_agent_runner.*?\{(?P<body>.*?)\n\}", agent_routes_code, re.DOTALL)
    if match:
        body = match.group("body")
        # Flag if spawn occurs before insert without any sync barrier or pre-registration
        if "tokio::spawn" in body and "state.comms.active_runners.insert(" in body:
            spawn_idx = body.find("let join_handle = tokio::spawn")
            insert_idx = body.find("state.comms.active_runners.insert")
            if spawn_idx != -1 and insert_idx != -1 and spawn_idx < insert_idx:
                # If there is no pre-registration or yield barrier, flag warning/error
                if "yield_now" not in body and "channel" not in body and "notify" not in body and "pre_insert" not in body:
                    errors.append("[C4-WARN] spawn_agent_runner spawns task before active_runners.insert without sync barrier. Can leak dead handles if subtask exits immediately.")
    return errors

def check_c5_untrusted_auth_bypass(agent_routes_code: str) -> list[str]:
    errors = []
    # Search for payload.auto_resume or payload.user_id in authorization decision
    if re.search(r"is_authorized_operator.*?=.*?payload\.auto_resume", agent_routes_code):
        errors.append("[C5-FAIL] Found authorization bypass: 'is_authorized_operator' trusts unauthenticated 'payload.auto_resume'.")
    if re.search(r"is_authorized_operator.*?=.*?payload\.user_id", agent_routes_code):
        errors.append("[C5-FAIL] Found authorization bypass: 'is_authorized_operator' trusts unauthenticated 'payload.user_id'.")
    return errors

def check_h1_mission_validation(mission_code: str) -> list[str]:
    errors = []
    # In update_mission, check if cost_usd is verified and rows_affected is checked
    match = re.search(r"pub async fn update_mission.*?\{(?P<body>.*?)\n\}", mission_code, re.DOTALL)
    if match:
        body = match.group("body")
        if "cost_usd.is_finite()" not in body and "cost_usd >=" not in body:
            errors.append("[H1-FAIL] update_mission does not validate that cost_usd is finite and non-negative.")
        if "rows_affected" not in body and "NotFound" not in body:
            errors.append("[H1-FAIL] update_mission does not check rows_affected() or return NotFound for missing missions.")
    return errors

def main():
    root = Path(__file__).resolve().parent.parent
    server_rs = root / "server-rs" / "src"

    persistence_rs = server_rs / "agent" / "persistence.rs"
    mission_rs = server_rs / "agent" / "mission.rs"
    routes_agent_rs = server_rs / "routes" / "agent.rs"

    all_errors = []

    print("[NEXUS-GUARD] [nexus_adversarial_guard] Scanning codebase for Dual-Pass Invariant Violations...")

    if persistence_rs.exists():
        code = persistence_rs.read_text(encoding="utf-8")
        all_errors.extend(check_c1_version_clobber(code))
        all_errors.extend(check_c2_cascade_ordering(code))
    else:
        all_errors.append(f"Missing file: {persistence_rs}")

    if routes_agent_rs.exists():
        code = routes_agent_rs.read_text(encoding="utf-8")
        all_errors.extend(check_c4_spawn_handle_race(code))
        all_errors.extend(check_c5_untrusted_auth_bypass(code))
    else:
        all_errors.append(f"Missing file: {routes_agent_rs}")

    if mission_rs.exists():
        code = mission_rs.read_text(encoding="utf-8")
        all_errors.extend(check_h1_mission_validation(code))
    else:
        all_errors.append(f"Missing file: {mission_rs}")

    print(f"[NEXUS-GUARD] Completed scan. Found {len(all_errors)} invariant violation(s).")
    for err in all_errors:
        print(f"  ❌ {err}")

    if all_errors:
        return 1
    else:
        print("  ✅ All Dual-Pass Nexus Invariants satisfied.")
        return 0

if __name__ == "__main__":
    sys.exit(main())
