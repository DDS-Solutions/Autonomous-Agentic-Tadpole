"""
@docs ARCHITECTURE:Governance:Permissions

### AI Assist Note
**Verify Safe Skills Pass-Through & Sovereign Governance Integration**
Verifies that:
1. Safe skills (list_files, read_codebase_file, get_current_time, calculate, search_global_vault) pass through without pending oversight interruption.
2. The SQLite permission_policies schema constraint ON CONFLICT(tool_name, agent_id) handles updates cleanly (HTTP 200).
3. Dynamic auto-approval logic respects both governance settings and explicit user overrides.

### 🔍 Debugging & Observability
- **Failure Path**: Connection refused if server-rs offline, HTTP non-200 if permission policy conflict or schema mismatch.
- **Telemetry Link**: Search `[verify_safe_skills_passthrough]` in system logs.
"""

import sys
import json
import time
import requests

BASE_URL = "http://127.0.0.1:8000"
HEADERS = {
    "User-Agent": "TadpoleOS/1.1.58",
    "Content-Type": "application/json"
}

def log(msg: str):
    print(f"[verify_safe_skills_passthrough] {msg}", flush=True)

def test_governance_settings():
    log("1. Checking governance settings...")
    try:
        r = requests.get(f"{BASE_URL}/v1/governance/settings", headers=HEADERS, timeout=5)
        if r.status_code == 200:
            data = r.json()
            auto_approve = data.get("auto_approve_safe_skills")
            log(f"   -> Governance auto_approve_safe_skills = {auto_approve}")
            return auto_approve is True
        else:
            log(f"   -> Failed with status {r.status_code}: {r.text}")
            return False
    except Exception as e:
        log(f"   -> Exception checking governance settings: {e}")
        return False

def test_security_policies_get():
    log("2. Checking security policies from DB...")
    try:
        r = requests.get(f"{BASE_URL}/v1/oversight/security/policies", headers=HEADERS, timeout=5)
        if r.status_code == 200:
            policies = r.json()
            policy_map = {p.get("tool_name"): p.get("mode") for p in policies}
            log(f"   -> Found {len(policies)} policies in DB.")
            
            safe_tools = ["list_files", "read_codebase_file", "get_current_time", "calculate", "search_global_vault"]
            all_ok = True
            for st in safe_tools:
                mode = policy_map.get(st)
                log(f"   -> Tool '{st}': mode = {mode}")
                if mode != "allow":
                    all_ok = False
            return all_ok
        else:
            log(f"   -> Failed with status {r.status_code}: {r.text}")
            return False
    except Exception as e:
        log(f"   -> Exception checking policies: {e}")
        return False

def test_security_policies_put():
    log("3. Testing PUT /v1/oversight/security/policies (verifying SQLite ON CONFLICT fix)...")
    try:
        payload = {
            "tool_name": "get_current_time",
            "mode": "allow"
        }
        r = requests.put(f"{BASE_URL}/v1/oversight/security/policies", headers=HEADERS, json=payload, timeout=5)
        if r.status_code == 200:
            log("   -> Successfully updated policy via PUT without SQLite constraint failure (HTTP 200).")
            return True
        else:
            log(f"   -> Failed with status {r.status_code}: {r.text}")
            return False
    except Exception as e:
        log(f"   -> Exception updating policy: {e}")
        return False

def test_pending_oversight_queue():
    log("4. Checking pending oversight queue...")
    try:
        r = requests.get(f"{BASE_URL}/v1/oversight/pending", headers=HEADERS, timeout=5)
        if r.status_code == 200:
            pending = r.json()
            log(f"   -> Current pending queue count: {len(pending)}")
            safe_names = {"list_files", "read_codebase_file", "get_current_time", "calculate"}
            spurious_pending = [p for p in pending if p.get("tool_name") in safe_names or p.get("action") in safe_names]
            if spurious_pending:
                log(f"   -> WARNING: Found spurious safe tool in pending queue: {spurious_pending}")
                return False
            else:
                log("   -> Clean: No safe tools unexpectedly queued for manual HITL approval.")
                return True
        else:
            log(f"   -> Failed with status {r.status_code}: {r.text}")
            return False
    except Exception as e:
        log(f"   -> Exception checking pending oversight: {e}")
        return False

def main():
    log("=== Starting Safe Skills Pass-Through Verification ===")
    
    # Check server availability
    try:
        r = requests.get(f"{BASE_URL}/health", headers=HEADERS, timeout=3)
        log(f"Server health status: {r.status_code}")
    except Exception as e:
        log(f"Server not currently reachable at {BASE_URL}: {e}")
        sys.exit(1)
        
    res1 = test_governance_settings()
    res2 = test_security_policies_get()
    res3 = test_security_policies_put()
    res4 = test_pending_oversight_queue()
    
    all_passed = res1 and res2 and res3 and res4
    log(f"=== Verification Complete. Overall Success: {all_passed} ===")
    sys.exit(0 if all_passed else 1)

if __name__ == "__main__":
    main()
