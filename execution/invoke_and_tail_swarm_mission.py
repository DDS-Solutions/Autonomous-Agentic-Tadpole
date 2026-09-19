"""
@docs ARCHITECTURE:Agent:SwarmTesting

### AI Assist Note
**🛡️ Tadpole OS: Swarm Mission Invocation & Real-time Server Log Tail**
Dispatches a bidirectional swarm mission to Agent 1 (CEO / Router) ->
Agent 2 (COO / Alpha) -> Specialist (Linus/Elon) -> Agent 99 (QA Auditor),
while tailing server-rs daily log in real-time. Captures toolbelt usage,
latency, errors, and system events.

### 🔍 Debugging & Observability
- **Telemetry Link**: Search `[invoke_and_tail_swarm_mission]` in system logs.
"""

import os
import sys
import time
import json
import requests
from datetime import datetime
from pathlib import Path

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass

ROOT = Path(__file__).resolve().parent.parent
ENV_PATH = ROOT / ".env"
LOGS_DIR = ROOT / "logs"

def get_neural_token() -> str:
    if not ENV_PATH.exists():
        return ""
    for line in ENV_PATH.read_text(encoding="utf-8").splitlines():
        if line.startswith("NEURAL_TOKEN="):
            return line.split("=", 1)[1].strip().strip('"').strip("'")
    return ""

def get_active_log_file() -> Path:
    today_str = datetime.now().strftime("%Y-%m-%d")
    log_file = LOGS_DIR / f"server.log.{today_str}"
    if not log_file.exists():
        # Fallback to newest server.log.* in logs dir
        all_logs = sorted(LOGS_DIR.glob("server.log.*"), key=lambda p: p.stat().st_mtime, reverse=True)
        if all_logs:
            return all_logs[0]
    return log_file

def run_swarm_mission_and_tail(agent_id: str = "2", timeout_secs: int = 75):
    token = get_neural_token()
    if not token:
        print("[!] Error: NEURAL_TOKEN not found in .env")
        return

    headers = {
        "User-Agent": "TadpoleOS/1.1.58",
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json",
        "x-tadpole-role": "overlord"
    }

    # Verify server health first
    try:
        hr = requests.get("http://127.0.0.1:8000/v1/engine/health", headers=headers, timeout=3)
        print(f"[*] Engine Health: HTTP {hr.status_code} - {hr.text.strip()}")
    except Exception as e:
        print(f"[!] Server unreachable at localhost:8000: {e}")
        return

    log_path = get_active_log_file()
    print(f"[*] Active server log path: {log_path}")
    if not log_path.exists():
        print(f"[!] Log file does not exist: {log_path}")
        return

    # Seek to end of log file
    log_file = open(log_path, "r", encoding="utf-8", errors="replace")
    log_file.seek(0, os.SEEK_END)
    initial_offset = log_file.tell()
    print(f"[*] Tailing from offset: {initial_offset} bytes...")

    # Formulate Mission Payload
    payload = {
        "message": (
            "Conduct a high-scrutiny codebase inspection mission focused on the access control logic within the file 'server-rs/src/services/acl_service.rs'. "
            "Use tool 'read_codebase_file' with path 'server-rs/src/services/acl_service.rs' to load the source, examine security policies and error handling, "
            "and share findings via 'share_finding' before completing the mission."
        ),
        "primaryGoal": "Audit server-rs/src/services/acl_service.rs",
        "analysis": True,
        "auto_resume": True
    }

    print("\n" + "="*80)
    print(f"🚀 DISPATCHING MISSION TO AGENT {agent_id} via POST /v1/agents/{agent_id}/tasks")
    print(f"Goal: {payload['primaryGoal']}")
    print("="*80 + "\n")

    dispatch_start = time.time()
    try:
        r = requests.post(f"http://127.0.0.1:8000/v1/agents/{agent_id}/tasks", headers=headers, json=payload, timeout=10)
        print(f"[+] Task Dispatch HTTP Status: {r.status_code}")
        print(f"[+] Response: {r.text}")
    except Exception as e:
        print(f"[!] Failed to dispatch task: {e}")
        log_file.close()
        return

    print("\n[*] Tailing live server logs. Watching for toolbelt invocations and hierarchical flow...\n")

    captured_events = []
    mission_completed = False
    start_tail = time.time()

    while time.time() - start_tail < timeout_secs:
        line = log_file.readline()
        if not line:
            time.sleep(0.3)
            continue

        raw_line = line.strip()
        if not raw_line:
            continue

        captured_events.append(raw_line)

        # Categorize and print highlighted log events
        upper = raw_line.upper()
        if "AGENT OF NINE" in upper or "TADPOLE ALPHA" in upper or "ISSUE_ALPHA_DIRECTIVE" in upper:
            print(f"  👑 [CEO/ROUTER]  {raw_line}")
        elif "SPAWN_SUBAGENT" in upper or "SUB-AGENT" in upper or "RECRUIT" in upper:
            print(f"  🐝 [SWARM-ALPHA] {raw_line}")
        elif "READ_FILE" in upper or "SHARE_FINDING" in upper or "EXECUTING TOOL" in upper:
            print(f"  🛠️ [TOOLBELT]   {raw_line}")
        elif "ANALYSIS" in upper or "AGENT 99" in upper or "QA AUDITOR" in upper:
            print(f"  🔍 [QA-99 AUDIT] {raw_line}")
        elif "ERROR" in upper or "FAILED" in upper or "PANIC" in upper or "WARN" in upper:
            print(f"  ⚠️ [ALERT]       {raw_line}")
        elif "TASK DISPATCHED" in upper or "MISSION" in upper or "COMPLETE" in upper:
            print(f"  📡 [ENGINE]      {raw_line}")
        else:
            # Print brief context lines if relevant
            if any(k in raw_line.lower() for k in ["prompt", "ollama", "gemma", "turn", "token"]):
                print(f"  🧠 [MODEL-LLM]  {raw_line}")

        if "MISSION REPORT GENERATED" in upper or "MISSION REPORT SAVED" in upper:
            mission_completed = True
            print("\n[+] Detected Mission Completion and QA Report Generation!")
            break

    log_file.close()
    elapsed = time.time() - start_tail
    print("\n" + "="*80)
    print(f"🏁 TAIL MONITORING FINISHED (Elapsed: {elapsed:.2f}s | Captured Events: {len(captured_events)})")
    print("="*80 + "\n")

    return captured_events

if __name__ == "__main__":
    target = sys.argv[1] if len(sys.argv) > 1 else "2"
    run_swarm_mission_and_tail(agent_id=target, timeout_secs=100)

# [invoke_and_tail_swarm_mission]

