"""
@docs ARCHITECTURE:Agent:SwarmTesting

### AI Assist Note
**🛡️ Tadpole OS: Dual Mission Dispatcher & Real-Time Server Tail**
Configures and launches 2 autonomous missions in local-only mode (Ollama / gemma4),
tails the server log for 40 seconds, and analyzes errors, gaps, and improvements.

### 🔍 Debugging & Observability
- Telemetry Link: Search `[run_dual_missions_tail]` in system logs.
"""

import os
import sys
import time
import json
import sqlite3
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
DB_PATH = ROOT / "data" / "tadpole.db"

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
        all_logs = sorted(LOGS_DIR.glob("server.log.*"), key=lambda p: p.stat().st_mtime, reverse=True)
        if all_logs:
            return all_logs[0]
    return log_file

def run_dual_missions_and_tail(tail_duration_secs: int = 40):
    print(f"[run_dual_missions_tail] Starting dual mission dispatch and {tail_duration_secs}s tail...")
    token = get_neural_token()
    if not token:
        print("[run_dual_missions_tail] [!] Error: NEURAL_TOKEN not found in .env")
        return

    headers = {
        "User-Agent": "TadpoleOS/1.1.58",
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json",
        "x-tadpole-role": "overlord"
    }

    # 1. Verify Server Health
    print("[1/5] Checking engine health at http://127.0.0.1:8000/health...")
    try:
        r = requests.get("http://127.0.0.1:8000/health", headers=headers, timeout=5)
        print(f"      Status: HTTP {r.status_code} | Health: {r.text.strip()}")
    except Exception as e:
        print(f"[!] Server unreachable: {e}")
        return

    # 2. Verify Local Ollama
    print("[2/5] Checking local Ollama health at http://127.0.0.1:11434/api/tags...")
    try:
        r = requests.get("http://127.0.0.1:11434/api/tags", timeout=5)
        tags = r.json().get("models", [])
        model_names = [m.get("name") for m in tags]
        print(f"      Ollama Online! Available Models: {', '.join(model_names[:5])}")
    except Exception as e:
        print(f"[!] Ollama not reachable on 11434: {e}")

    # 3. Position Log Tail to EOF
    log_path = get_active_log_file()
    print(f"[3/5] Attaching to active log: {log_path.name}")
    if not log_path.exists():
        print(f"[!] Log path not found: {log_path}")
        return

    log_file = open(log_path, "r", encoding="utf-8", errors="replace")
    log_file.seek(0, os.SEEK_END)
    initial_offset = log_file.tell()
    print(f"      Initial log offset: {initial_offset} bytes")

    # 4. Dispatch Mission 1: Agent 2 (Tadpole - Codebase Security Audit)
    print("\n[4/5] Dispatching Mission 1 -> Agent 2 (Tadpole / Operations)...")
    m1_payload = {
        "message": (
            "Perform a security and access control audit of 'server-rs/src/services/acl_service.rs'. "
            "Examine the permission policies, role hierarchy enforcement, and potential bypass routes. "
            "Report key findings in a clear, structured summary."
        ),
        "primaryGoal": "Security Audit of server-rs/src/services/acl_service.rs",
        "analysis": True,
        "auto_resume": True
    }
    try:
        r1 = requests.post("http://127.0.0.1:8000/v1/agents/2/tasks", headers=headers, json=m1_payload, timeout=8)
        print(f"      Mission 1 Response: HTTP {r1.status_code} - {r1.text.strip()}")
    except Exception as e:
        print(f"      [!] Failed to dispatch Mission 1: {e}")

    # Dispatch Mission 2: Agent 1 (Agent of Nine - Swarm Architecture & Cluster Audit)
    print("\n[4/5] Dispatching Mission 2 -> Agent 1 (Agent of Nine / Routing)...")
    m2_payload = {
        "message": (
            "Examine active swarm cluster status and infrastructure health. "
            "Verify all registered bunker nodes and report overall swarm cluster readiness."
        ),
        "primaryGoal": "Swarm Infrastructure & Cluster Readiness Audit",
        "analysis": True,
        "auto_resume": True
    }
    try:
        r2 = requests.post("http://127.0.0.1:8000/v1/agents/1/tasks", headers=headers, json=m2_payload, timeout=8)
        print(f"      Mission 2 Response: HTTP {r2.status_code} - {r2.text.strip()}")
    except Exception as e:
        print(f"      [!] Failed to dispatch Mission 2: {e}")

    # 5. Tail Server Logs for exact duration
    print(f"\n[5/5] Tailing server logs for {tail_duration_secs} seconds...\n" + "="*80)
    
    start_time = time.time()
    events = []
    error_count = 0
    warn_count = 0
    tool_count = 0
    llm_count = 0

    while (time.time() - start_time) < tail_duration_secs:
        line = log_file.readline()
        if not line:
            time.sleep(0.2)
            continue
        
        line_clean = line.strip()
        if not line_clean:
            continue

        events.append(line_clean)
        upper = line_clean.upper()

        # Categorize output
        if "ERROR" in upper or "PANIC" in upper or "CRITICAL" in upper or "FAIL" in upper:
            error_count += 1
            print(f"ALERT [ERROR/FAIL] {line_clean}")
        elif "WARN" in upper:
            warn_count += 1
            print(f"WARN  [WARNING]    {line_clean}")
        elif "OLLAMA" in upper or "GEMMA" in upper or "INFERENCE" in upper or "TOKEN" in upper:
            llm_count += 1
            print(f"MODEL [LLM-LOCAL]  {line_clean}")
        elif "TOOL" in upper or "DISPATCH" in upper or "AGENT" in upper or "TASK" in upper:
            tool_count += 1
            print(f"ENGIN [ENGINE]     {line_clean}")
        elif "PULSE" in upper or "TELEMETRY" in upper:
            print(f"TELEM [TELEMETRY]  {line_clean}")
        else:
            print(f"INFO  [SERVER]     {line_clean}")

    log_file.close()
    elapsed = time.time() - start_time
    print("="*80)
    print(f"\nLog Tail Complete in {elapsed:.1f}s!")
    print(f"Total Lines Captured: {len(events)}")
    print(f"Errors Detected: {error_count}")
    print(f"Warnings Detected: {warn_count}")
    print(f"LLM/Model Events: {llm_count}")
    print(f"Engine/Tool Events: {tool_count}")

    # Check Database for mission records
    print("\nChecking SQLite Mission Records (data/tadpole.db)...")
    if DB_PATH.exists():
        try:
            conn = sqlite3.connect(DB_PATH)
            cur = conn.cursor()
            cur.execute("""
                SELECT id, title, agent_id, status, created_at, updated_at, cost_usd 
                FROM mission_history 
                ORDER BY created_at DESC LIMIT 5
            """)
            recent_missions = cur.fetchall()
            for m in recent_missions:
                print(f"  - Mission ID: {m[0]} | Title: {m[1]} | Agent: {m[2]} | Status: {m[3]} | Cost: ${m[6]:.4f}")
            conn.close()
        except Exception as e:
            print(f"  [!] SQLite query error: {e}")

if __name__ == "__main__":
    tail_secs = int(sys.argv[1]) if len(sys.argv) > 1 else 40
    run_dual_missions_and_tail(tail_secs)
