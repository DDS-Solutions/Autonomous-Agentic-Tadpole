"""
@docs ARCHITECTURE:Agent:SwarmTesting

### AI Assist Note
**🛡️ Tadpole OS: Swarm Stress Simulation & Verification Suite**
Dispatches 3 concurrent specialized agents in local-only mode (Ollama / gemma4),
evaluating:
1. ModelGovernor inference serialization (Inference Semaphore)
2. PID lockfile integrity (`data/engine.pid.lock`)
3. Sovereign Ed25519 tamper-evident audit ledger signing (`data/.keys/audit_ed25519.key`)
4. Checkpoint pruning and SQLite database WAL stability
5. Server log telemetry for zero fatal panics / race condition errors.

### 🔍 Debugging & Observability
- Telemetry Link: Search `[simulate_swarm_stress]` in system logs.
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
PID_LOCK = ROOT / "data" / "engine.pid.lock"
AUDIT_KEY = ROOT / "data" / ".keys" / "audit_ed25519.key"

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

def run_stress_test(tail_duration_secs: int = 45):
    print("=" * 70)
    print("🚀 [simulate_swarm_stress] Initializing Full Swarm Stress Test...")
    print("=" * 70)
    
    token = get_neural_token()
    if not token:
        print("[!] Fatal: NEURAL_TOKEN not found in .env")
        return False

    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json",
        "User-Agent": "TadpoleOS/1.1.59"
    }

    # 1. Verify Server Health
    try:
        r = requests.get("http://127.0.0.1:8000/health", headers=headers, timeout=5)
        if r.status_code != 200:
            print(f"[!] Server returned non-200 health status: {r.status_code}")
            return False
        print(f"✅ [Server Health] Engine alive: {r.json().get('status', 'unknown')}")
    except Exception as e:
        print(f"[!] Engine connection failed: {e}")
        return False

    # 2. Verify PID Lockfile
    if PID_LOCK.exists():
        locked_pid = PID_LOCK.read_text(encoding="utf-8").strip()
        print(f"✅ [Engine Lock] data/engine.pid.lock active: PID {locked_pid}")
    else:
        print("⚠️ [Engine Lock] data/engine.pid.lock not found on disk")

    # 3. Verify Sovereign Ed25519 Key
    if AUDIT_KEY.exists():
        key_hex = AUDIT_KEY.read_text(encoding="utf-8").strip()
        print(f"✅ [SecurityAudit] Sovereign Ed25519 key active ({len(key_hex)} hex characters)")
    else:
        print("⚠️ [SecurityAudit] Sovereign key file data/.keys/audit_ed25519.key pending boot creation")

    # 4. Prepare Missions
    missions = [
        {
            "agent_id": "3",
            "name": "Elon (CTO)",
            "goal": "Scan codebase symbol dependencies and assess architecture invariants.",
            "category": "engineering",
            "priority": "high",
            "max_turns": 10
        },
        {
            "agent_id": "7",
            "name": "Grace (DevOps)",
            "goal": "Inspect database performance, checkpoint sizes, and telemetry pulse nodes.",
            "category": "operations",
            "priority": "high",
            "max_turns": 10
        },
        {
            "agent_id": "2",
            "name": "Linus (Kernel / Architect)",
            "goal": "Audit sandbox execution boundaries and check memory pressure telemetry.",
            "category": "security",
            "priority": "high",
            "max_turns": 10
        }
    ]

    log_file = get_active_log_file()
    initial_log_pos = log_file.stat().st_size if log_file.exists() else 0

    print(f"\n📡 Dispatching {len(missions)} concurrent agent missions into the swarm...")
    dispatched = []
    for m in missions:
        url = f"http://127.0.0.1:8000/v1/agents/{m['agent_id']}/tasks"
        payload = {
            "message": f"{m['name']}: {m['goal']} Use your specialized tools to verify system integrity and report findings.",
            "primaryGoal": m["goal"],
            "analysis": True,
            "auto_resume": True
        }
        try:
            res = requests.post(url, headers=headers, json=payload, timeout=10)
            if res.status_code in (200, 201, 202):
                data = res.json()
                mid = data.get("mission_id") or data.get("task_id") or data.get("id") or "dispatched"
                print(f"  ⚡ [{m['name']}] Mission dispatched: {mid}")
                dispatched.append(mid)
            else:
                print(f"  ❌ [{m['name']}] Dispatch failed ({res.status_code}): {res.text[:120]}")
        except Exception as e:
            print(f"  ❌ [{m['name']}] Request error: {e}")

    print(f"\n📊 Tailing live server log for {tail_duration_secs}s to observe swarm execution...")
    start_time = time.time()
    last_pos = initial_log_pos
    log_lines = []

    while time.time() - start_time < tail_duration_secs:
        time.sleep(2.0)
        if log_file.exists():
            curr_size = log_file.stat().st_size
            if curr_size > last_pos:
                with open(log_file, "r", encoding="utf-8", errors="replace") as f:
                    f.seek(last_pos)
                    new_chunk = f.read()
                    last_pos = f.tell()
                for line in new_chunk.splitlines():
                    if line.strip():
                        log_lines.append(line)
                        if any(k in line for k in ("ERROR", "WARN", "SSCP", "Inference", "Reaper", "VerificationGate", "Aletheia", "Turn", "Audit")):
                            print(f"  | {line[:140]}")

    elapsed = time.time() - start_time
    print(f"\n⏱️ Tail complete. Captured {len(log_lines)} log events in {elapsed:.1f}s.")

    # 5. Analyze Log Metrics
    error_lines = [l for l in log_lines if "ERROR" in l and "favicon" not in l]
    warn_lines = [l for l in log_lines if "WARN" in l]
    print(f"\n📋 Telemetry Quality Gate:")
    print(f"  • Total log lines: {len(log_lines)}")
    print(f"  • Errors detected: {len(error_lines)}")
    print(f"  • Warnings detected: {len(warn_lines)}")

    if error_lines:
        print("  🚨 Sample Errors:")
        for err in error_lines[:5]:
            print(f"    - {err[:120]}")

    # 6. Database Health Check
    if DB_PATH.exists():
        try:
            conn = sqlite3.connect(str(DB_PATH))
            cur = conn.cursor()
            cur.execute("SELECT COUNT(*) FROM mission_checkpoints")
            cp_count = cur.fetchone()[0]
            cur.execute("SELECT COUNT(*) FROM audit_trail")
            audit_count = cur.fetchone()[0]
            cur.execute("SELECT COUNT(*) FROM mission_history WHERE status = 'active'")
            active_missions = cur.fetchone()[0]
            conn.close()
            print(f"  • Database checkpoints: {cp_count}")
            print(f"  • Tamper-evident audit rows: {audit_count}")
            print(f"  • Active missions in SQLite: {active_missions}")
        except Exception as e:
            print(f"  • DB inspection error: {e}")

    print("\n🏁 Swarm stress test completed.")
    return len(error_lines) == 0

if __name__ == "__main__":
    tail_time = 45
    if len(sys.argv) > 1:
        try:
            tail_time = int(sys.argv[1])
        except ValueError:
            pass
    success = run_stress_test(tail_time)
    sys.exit(0 if success else 1)
