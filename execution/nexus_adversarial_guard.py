"""
@docs ARCHITECTURE:Core

### AI Assist Note
**Nexus Adversarial Invariant Guard**
Pattern and semantic invariant analyzer enforcing the Mandatory Dual-Pass Nexus Protocol (SOP-NEXUS-01).
Verifies scheduler invariants, relational cascade ordering, concurrency locks, and trust boundaries.

### 🔍 Debugging & Observability
- **Failure Path**: Antipattern detected in server-rs source code.
- **Telemetry Link**: Search `[nexus_adversarial_guard]` in audit logs.
"""

import sys
import re
from pathlib import Path

# Ensure UTF-8 output on Windows
if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8")

def check_c1_version_clobber(persistence_code: str) -> list[str]:
    errors = []
    # Verify save_agent function exists
    if "execute_save_agent" not in persistence_code and "save_agent" not in persistence_code:
        errors.append("[C1-UNVERIFIED] could not locate `execute_save_agent` in agent/persistence.rs — invariant UNVERIFIED")
        return errors
        
    # Check if save_agent reconciles by assigning version from db without comparison
    if re.search(r"reconciled\.version\s*=\s*db_ver", persistence_code):
        errors.append("[C1-FAIL] Found optimistic locking clobber: 'reconciled.version = db_ver' bypasses OCC.")
    return errors

def check_c2_cascade_ordering(persistence_code: str) -> list[str]:
    errors = []
    # In delete_agent_cascade, DELETE FROM agents must come AFTER dependent mission/log deletes
    match = re.search(r"pub async fn delete_agent_cascade.*?\{(?P<body>.*?)\n\}", persistence_code, re.DOTALL)
    if not match:
        errors.append("[C2-UNVERIFIED] could not locate `delete_agent_cascade` in agent/persistence.rs — invariant UNVERIFIED")
        return errors

    body = match.group("body")
    agents_idx = body.find("DELETE FROM agents")
    mission_idx = body.find("DELETE FROM mission_history")
    logs_idx = body.find("DELETE FROM mission_logs")

    if agents_idx == -1:
        errors.append("[C2-FAIL] delete_agent_cascade does not delete from 'agents' table.")
    else:
        if mission_idx == -1:
            errors.append("[C2-FAIL] delete_agent_cascade does not clean 'mission_history'.")
        elif agents_idx < mission_idx:
            errors.append("[C2-FAIL] delete_agent_cascade deletes from 'agents' BEFORE 'mission_history'. Violates PRAGMA foreign_keys = ON.")
        
        if logs_idx == -1:
            errors.append("[C2-FAIL] delete_agent_cascade does not clean 'mission_logs'.")
        elif agents_idx < logs_idx:
            errors.append("[C2-FAIL] delete_agent_cascade deletes from 'agents' BEFORE 'mission_logs'.")
    return errors

def check_c3_retry_occ_clobber(persistence_code: str) -> list[str]:
    errors = []
    # Ensure no retry loop catches Conflict and overwrites version to force Last-Write-Wins
    if re.search(r"AppError::Conflict.*?version\s*=\s*.*?\+.*?retry", persistence_code, re.DOTALL | re.IGNORECASE):
        errors.append("[C3-FAIL] Detected retry loop converting OCC conflict into untracked Last-Write-Wins.")
    return errors

def check_c4_spawn_handle_race(agent_routes_code: str) -> list[str]:
    errors = []
    # Check spawn_agent_runner for task cleanup race
    # Ensure active_runners is not inserted after tokio::spawn without synchronization
    match = re.search(r"pub\(crate\) fn spawn_agent_runner.*?\{(?P<body>.*?)\n\}", agent_routes_code, re.DOTALL)
    if not match:
        errors.append("[C4-UNVERIFIED] could not locate `spawn_agent_runner` in routes/agent.rs — invariant UNVERIFIED")
        return errors

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
    # Search for payload.auto_resume or payload.user_id in authorization decision across newlines (re.DOTALL)
    if re.search(r"is_authorized_operator.*?=.*?payload\.auto_resume", agent_routes_code, re.DOTALL):
        errors.append("[C5-FAIL] Found authorization bypass: 'is_authorized_operator' trusts unauthenticated 'payload.auto_resume'.")
    if re.search(r"is_authorized_operator.*?=.*?payload\.user_id", agent_routes_code, re.DOTALL):
        errors.append("[C5-FAIL] Found authorization bypass: 'is_authorized_operator' trusts unauthenticated 'payload.user_id'.")
    return errors

def check_c6_graph_path_validation(intelligence_service_code: str) -> list[str]:
    errors = []
    match = re.search(r"pub async fn list_graph.*?\{(?P<body>.*?)\n\}", intelligence_service_code, re.DOTALL)
    if not match:
        errors.append("[C6-UNVERIFIED] could not locate `list_graph` in intelligence/service.rs — invariant UNVERIFIED")
    else:
        body = match.group("body")
        if "validate_path" not in body and "contains(\"..\"" not in body:
            errors.append("[C6-FAIL] `list_graph` in intelligence/service.rs does not validate path_prefix boundaries.")
    return errors

def check_h1_mission_validation(mission_code: str) -> list[str]:
    errors = []
    # In update_mission, check if cost_usd is verified and rows_affected is checked
    match = re.search(r"pub async fn update_mission.*?\{(?P<body>.*?)\n\}", mission_code, re.DOTALL)
    if not match:
        errors.append("[H1-UNVERIFIED] could not locate `update_mission` in agent/mission.rs — invariant UNVERIFIED")
        return errors

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
    intelligence_service_rs = server_rs / "intelligence" / "service.rs"

    all_errors = []

    print("[NEXUS-GUARD] [nexus_adversarial_guard] Scanning codebase for Dual-Pass Invariant Violations...")

    if persistence_rs.exists():
        code = persistence_rs.read_text(encoding="utf-8")
        all_errors.extend(check_c1_version_clobber(code))
        all_errors.extend(check_c2_cascade_ordering(code))
        all_errors.extend(check_c3_retry_occ_clobber(code))
    else:
        all_errors.append(f"Missing file: {persistence_rs}")

    if routes_agent_rs.exists():
        code = routes_agent_rs.read_text(encoding="utf-8")
        all_errors.extend(check_c4_spawn_handle_race(code))
        all_errors.extend(check_c5_untrusted_auth_bypass(code))
    else:
        all_errors.append(f"Missing file: {routes_agent_rs}")

    if intelligence_service_rs.exists():
        code = intelligence_service_rs.read_text(encoding="utf-8")
        all_errors.extend(check_c6_graph_path_validation(code))
    else:
        all_errors.append(f"Missing file: {intelligence_service_rs}")

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
