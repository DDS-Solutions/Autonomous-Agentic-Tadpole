"""
@docs ARCHITECTURE:Agent:SwarmTesting

### AI Assist Note
**🛡️ Tadpole OS: Specialized Dual Mission Dispatcher & Longtail Server Telemetry**
Dispatches 2 specialized missions (Agent 3: Code Graph & Blast Radius Intelligence;
Agent 7: DevOps Database & Telemetry Health) in local-only mode (Ollama / gemma4),
and tails the live server log for 60 seconds to evaluate multi-turn autonomous execution.

### 🔍 Debugging & Observability
- Telemetry Link: Search `[run_specialized_missions_longtail]` in system logs.
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

print("[run_specialized_missions_longtail] Initializing specialized dual mission test...")

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

def run_longtail_test(tail_duration_secs: int = 60):
    print(f"[run_specialized_missions_longtail] Starting dispatch and {tail_duration_secs}s tail...")
    token = get_neural_token()
    if not token:
        print("[run_specialized_missions_longtail] [!] Error: NEURAL_TOKEN not found in .env")
        return

    headers = {
        "User-Agent": "TadpoleOS/1.1.59",
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json",
        "x-tadpole-role": "overlord"
    }

    # 1. Check Server Health
    print("[1/5] Checking engine health at http://127.0.0.1:8000/health...")
    try:
        r = requests.get("http://127.0.0.1:8000/health", headers=headers, timeout=5)
        print(f"      Status: HTTP {r.status_code} | Health: {r.text.strip()}")
    except Exception as e:
        print(f"[!] Server unreachable: {e}")
        return

    # 2. Check Local Ollama
    print("[2/5] Checking local Ollama health at http://127.0.0.1:11434/api/tags...")
    try:
        r_ol = requests.get("http://127.0.0.1:11434/api/tags", timeout=5)
        model_names = [m.get("name") for m in r_ol.json().get("models", [])]
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

    # 4. Dispatch Mission A -> Agent 3 (Elon / CTO - Code Graph Intelligence & Blast Radius)
    print("\n[4/5] Dispatching Mission A -> Agent 3 (Elon / CTO)...")
    m_a_payload = {
        "message": (
            "Analyze the architectural blast radius and dependent symbols for 'server-rs/src/services/acl_service.rs'. "
            "Use your intelligence and code inspection tools to evaluate upstream callers and downstream test impacts. "
            "Share your architectural summary finding."
        ),
        "primaryGoal": "Blast Radius & Context Resolution for acl_service.rs",
        "analysis": True,
        "auto_resume": True
    }
    try:
        r_a = requests.post("http://127.0.0.1:8000/v1/agents/3/tasks", headers=headers, json=m_a_payload, timeout=8)
        print(f"      Mission A Response: HTTP {r_a.status_code} - {r_a.text.strip()}")
    except Exception as e:
        print(f"      [!] Failed to dispatch Mission A: {e}")

    # Dispatch Mission B -> Agent 7 (Grace / DevOps - Database & Telemetry Health)
    print("\n[4/5] Dispatching Mission B -> Agent 7 (Grace / DevOps)...")
    m_b_payload = {
        "message": (
            "Perform a comprehensive system telemetry and database health check. "
            "Inspect system performance metrics, pool status, and active agent resource consumption. "
            "Summarize the operational readiness of the platform."
        ),
        "primaryGoal": "System Telemetry & Database Health Audit",
        "analysis": True,
        "auto_resume": True
    }
    try:
        r_b = requests.post("http://127.0.0.1:8000/v1/agents/7/tasks", headers=headers, json=m_b_payload, timeout=8)
        print(f"      Mission B Response: HTTP {r_b.status_code} - {r_b.text.strip()}")
    except Exception as e:
        print(f"      [!] Failed to dispatch Mission B: {e}")

    # 5. Tail Server Logs for exact duration
    print(f"\n[5/5] Tailing server logs for {tail_duration_secs} seconds...\n" + "="*80)
    
    start_time = time.time()
    events = []
    error_count = 0
    warn_count = 0
    tool_count = 0
    llm_count = 0

    while time.time() - start_time < tail_duration_secs:
        line = log_file.readline()
        if not line:
            time.sleep(0.1)
            continue

        events.append(line)
        line_clean = line.strip()

        # Categorize output
        if "ERROR" in line:
            error_count += 1
            print(f"ALERT [ERROR/FAIL] {line_clean}")
        elif "WARN" in line:
            warn_count += 1
            print(f"WARN  [WARNING]    {line_clean}")
        elif "ToolExecution" in line or "Bridge" in line or "Aletheia" in line or "VerificationGate" in line:
            tool_count += 1
            print(f"ENGIN [ENGINE]     {line_clean}")
        elif "Provider" in line or "ollama" in line or "Polyglot" in line or "parser" in line:
            llm_count += 1
            print(f"MODEL [LLM-LOCAL]  {line_clean}")
        elif "Telemetry" in line or "Pulse" in line:
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

    # Check SQLite Mission Records
    print("\nChecking SQLite Mission Records (data/tadpole.db)...")
    try:
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute(
            "SELECT id, title, agent_id, status, cost_usd FROM mission_history "
            "ORDER BY rowid DESC LIMIT 6"
        )
        for row in cur.fetchall():
            m_id, title, a_id, status, cost = row
            print(f"  - Mission ID: {m_id} | Title: {(title or '')[:50]}... | Agent: {a_id} | Status: {status} | Cost: ${cost:.4f}")
        conn.close()
    except Exception as e:
        print(f"[!] SQLite query failed: {e}")

if __name__ == "__main__":
    duration = 60
    if len(sys.argv) > 1:
        try:
            duration = int(sys.argv[1])
        except ValueError:
            pass
    run_longtail_test(duration)
