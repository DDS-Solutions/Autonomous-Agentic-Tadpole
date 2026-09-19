"""
@docs ARCHITECTURE:Agent:Tasks

### AI Assist Note
**Live Verification: Section 4 Improvements**
Validates unified health semantics, operator auto-resume on dispatch,
and agent suspension state transitions against the compiled server-rs binary.

### 🔍 Debugging & Observability
- **Failure Path**: Connection refused if server-rs fails to start, 401 Unauthorized if token invalid.
- **Telemetry Link**: Search `[live_verify_improvements]` in system logs.
"""

import os
import sys
import time
import subprocess
import requests
from pathlib import Path

# Ensure stdout handles UTF-8 on Windows
if sys.platform == "win32":
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding='utf-8', errors='replace')
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding='utf-8', errors='replace')

WORKSPACE_ROOT = Path(__file__).resolve().parent.parent
SERVER_BINARY = WORKSPACE_ROOT / "server-rs" / "target" / "debug" / "server-rs.exe"

def load_neural_token():
    env_file = WORKSPACE_ROOT / ".env"
    if env_file.exists():
        for line in env_file.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line.startswith("NEURAL_TOKEN=") or line.startswith("NEURAL_ENGINE_ACCESS_TOKEN="):
                parts = line.split("=", 1)
                if len(parts) == 2 and parts[1].strip():
                    return parts[1].strip().strip('"').strip("'")
    return os.getenv("NEURAL_TOKEN", "")

def extract_agents(res_json):
    if isinstance(res_json, dict):
        return res_json.get("data", res_json.get("items", []))
    if isinstance(res_json, list):
        return res_json
    return []

def test_live_improvements():
    token = load_neural_token()
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {token}",
        "User-Agent": "TadpoleOS/1.1.58"
    }
    base_url = "http://127.0.0.1:8000"

    print("======================================================================")
    print("      🛡️ LIVE VERIFICATION: SECTION 4 IMPROVEMENTS (v1.1.58)        ")
    print("      [live_verify_improvements] Telemetry linked                    ")
    print("======================================================================")

    # 1. Check if server-rs is running, launch if not
    server_process = None
    try:
        requests.get(f"{base_url}/health", timeout=1.0)
        print("[INIT] server-rs is already running.")
    except Exception:
        print("[INIT] Starting server-rs test instance...")
        server_process = subprocess.Popen(
            [str(SERVER_BINARY)],
            cwd=str(WORKSPACE_ROOT),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL
        )
        for _ in range(30):
            time.sleep(0.5)
            try:
                res = requests.get(f"{base_url}/health", timeout=1.0)
                if res.status_code == 200:
                    print("[INIT] server-rs boot complete and listening.")
                    break
            except Exception:
                pass
        else:
            print("❌ FATAL: Timed out waiting for server-rs to boot.")
            if server_process:
                server_process.terminate()
            return False

    all_passed = True
    try:
        # TEST 1: Retrieve Agent 1 registry retrieval...
        print("\n[TEST 1] Verifying Agent registry retrieval...")
        res = requests.get(f"{base_url}/v1/agents", headers=headers, timeout=5.0)
        assert res.status_code == 200, f"Expected 200, got {res.status_code}: {res.text}"
        agents = extract_agents(res.json())
        print(f"  Discovered agents in registry: {[a.get('id') for a in agents]}")
        agent1 = next((a for a in agents if str(a.get("id")) == "1"), None)
        if agent1 is None and len(agents) > 0:
            target_id = str(agents[0].get("id"))
            print(f"  Agent '1' not present; using first agent '{target_id}' as test subject.")
            agent1 = agents[0]
        else:
            target_id = "1"
        assert agent1 is not None, f"No agents found in registry: {res.text}"
        print(f"  Target Agent found: id='{target_id}', status='{agent1.get('status')}', isHealthy={agent1.get('isHealthy')}")

        # TEST 2: Suspend target agent (Quarantine)
        print(f"\n[TEST 2] Suspending Agent {target_id} via POST /v1/agents/{target_id}/pause...")
        res = requests.post(f"{base_url}/v1/agents/{target_id}/pause", headers=headers, timeout=5.0)
        assert res.status_code == 200, f"Failed to pause agent: {res.status_code}: {res.text}"
        print(f"  Agent {target_id} paused successfully.")

        # TEST 3: Verify Unified Health Semantics (isHealthy MUST be false when suspended)
        print("\n[TEST 3] Verifying Unified Health Semantics (isHealthy == false for suspended agent)...")
        res = requests.get(f"{base_url}/v1/agents", headers=headers, timeout=5.0)
        agents = extract_agents(res.json())
        agent1 = next((a for a in agents if str(a.get("id")) == target_id), None)
        assert agent1 is not None, f"Agent '{target_id}' not found"
        print(f"  Agent {target_id} status: '{agent1.get('status')}', isHealthy: {agent1.get('isHealthy')}")
        assert agent1.get("status") == "suspended", f"Expected 'suspended', got {agent1.get('status')}"
        assert agent1.get("isHealthy") is False, f"Expected isHealthy == False, got {agent1.get('isHealthy')}"
        print("  ✅ Unified Health Semantics confirmed: Suspended agent is flagged as unhealthy (isHealthy: false)!")

        # TEST 4: Dispatch task with non-operator (should be rejected with 400)
        print("\n[TEST 4] Testing standard user dispatch to suspended agent (Expect 400 Bad Request)...")
        unauthorized_payload = {
            "message": "ping standard user",
            "userId": "999",
            "autoResume": False
        }
        res = requests.post(f"{base_url}/v1/agents/{target_id}/tasks", headers=headers, json=unauthorized_payload, timeout=5.0)
        print(f"  Standard dispatch response: HTTP {res.status_code}")
        assert res.status_code == 400, f"Expected 400 for suspended agent standard user, got {res.status_code}: {res.text}"
        print("  ✅ Access control confirmed: Standard user cannot bypass suspension quarantine.")

        # TEST 5: Operator Auto-Awaken Dispatch (userId='0', autoResume=True)
        print("\n[TEST 5] Testing Overlord operator dispatch (userId='0', autoResume=True)...")
        operator_payload = {
            "message": "ping overlord operator",
            "userId": "0",
            "autoResume": True
        }
        res = requests.post(f"{base_url}/v1/agents/{target_id}/tasks", headers=headers, json=operator_payload, timeout=5.0)
        print(f"  Operator dispatch response: HTTP {res.status_code}")
        assert res.status_code in (200, 202), f"Expected 200 or 202 for operator dispatch, got {res.status_code}: {res.text}"
        print("  ✅ Overlord Auto-Awaken confirmed: Operator dispatch automatically awakened suspended agent!")

        # TEST 6: Verify agent state has transitioned to 'idle' / active and isHealthy is True
        print(f"\n[TEST 6] Verifying Agent {target_id} post-awaken status and health...")
        res = requests.get(f"{base_url}/v1/agents", headers=headers, timeout=5.0)
        agents = extract_agents(res.json())
        agent1 = next((a for a in agents if str(a.get("id")) == target_id), None)
        assert agent1 is not None, f"Agent '{target_id}' not found"
        print(f"  Agent {target_id} status: '{agent1.get('status')}', isHealthy: {agent1.get('isHealthy')}")
        assert agent1.get("status") in ("idle", "busy", "thinking", "working"), f"Expected operational status, got {agent1.get('status')}"
        assert agent1.get("isHealthy") is True, f"Expected isHealthy == True, got {agent1.get('isHealthy')}"
        print(f"  ✅ Agent status restored: Agent {target_id} returned to service with isHealthy: true!")

        # TEST 7: Test explicit Resume endpoint (/v1/agents/{target_id}/resume)
        print("\n[TEST 7] Testing explicit UI 'Resume Agent' endpoint...")
        # First suspend again
        requests.post(f"{base_url}/v1/agents/{target_id}/pause", headers=headers, timeout=5.0)
        # Call resume
        res = requests.post(f"{base_url}/v1/agents/{target_id}/resume", headers=headers, timeout=5.0)
        assert res.status_code == 200, f"Expected 200 from resume, got {res.status_code}"
        res = requests.get(f"{base_url}/v1/agents", headers=headers, timeout=5.0)
        agents = extract_agents(res.json())
        agent1 = next((a for a in agents if str(a.get("id")) == target_id), None)
        assert agent1.get("status") == "idle", f"Expected idle, got {agent1.get('status')}"
        print("  ✅ 1-Click UI Resume endpoint verified successfully!")

        print("\n======================================================================")
        print("       🎉 ALL 7 LIVE VERIFICATION SCENARIOS PASSED WITH 100% SUCCESS    ")
        print("======================================================================")

    except Exception as e:
        print(f"\n❌ Live verification failed: {e}")
        all_passed = False
    finally:
        if server_process:
            print("\n[CLEANUP] Gracefully shutting down test server instance...")
            server_process.terminate()
            try:
                server_process.wait(timeout=5)
            except Exception:
                server_process.kill()
            print("[CLEANUP] Test server instance terminated.")

    return all_passed

if __name__ == "__main__":
    success = test_live_improvements()
    sys.exit(0 if success else 1)

# Metadata: [verify_live_improvements]
