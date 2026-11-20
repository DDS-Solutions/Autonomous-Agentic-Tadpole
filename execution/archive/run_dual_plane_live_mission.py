"""
@docs ARCHITECTURE:Agent:SwarmTesting

### AI Assist Note
**🛡️ Tadpole OS: Dual-Plane Live Mission Deployer & 30s Server Tail**
Dispatches live missions testing Plane 1 (EngineMaintenance) and Plane 2 (SmbDigitalTwin),
tails the server logs for 30+ seconds, and audits runtime behavior, security boundaries (FR-11),
turn compaction, and loop detection under local models only (Ollama / gemma4:e4b).

### 🔍 Debugging & Observability
- Telemetry Link: Search `[dual_plane_mission]` in system logs.
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

def deploy_and_tail(tail_duration_secs: int = 35):
    print("=" * 80)
    print("🛡️  TADPOLE OS DUAL-PLANE SOVEREIGN MISSION DEPLOYMENT (v1.1.59) [dual_plane_mission]")
    print(f"Target Endpoints: Backend (http://127.0.0.1:8000) | Frontend (http://127.0.0.1:5173)")
    print(f"Tail Duration: {tail_duration_secs} seconds | Engine Model: LOCAL ONLY (Ollama / gemma4:e4b)")
    print("=" * 80)

    token = get_neural_token()
    if not token:
        print("[!] Fatal: NEURAL_TOKEN not found in .env")
        sys.exit(1)

    headers = {
        "User-Agent": "TadpoleOS/1.1.59",
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json",
        "x-tadpole-role": "overlord"
    }

    # Step 1: Health Pre-checks
    print("\n[Step 1/5] Verifying Live Engine & UI Services...")
    try:
        r_fe = requests.get("http://127.0.0.1:5173", timeout=4)
        print(f"  [Frontend 5173] HTTP {r_fe.status_code} - React/Vite Dev Server Online")
    except Exception as e:
        print(f"  [Frontend 5173] [!] Warning: Failed to reach Vite dev server: {e}")

    try:
        r_be = requests.get("http://127.0.0.1:8000/health", headers=headers, timeout=4)
        health_data = r_be.json()
        print(f"  [Backend  8000] HTTP {r_be.status_code} - Rust Engine Online (v{health_data.get('version', 'unknown')})")
        print(f"                  Active Agents: {health_data.get('active_agents')} | DB: {health_data.get('database', {}).get('status')}")
    except Exception as e:
        print(f"  [Backend  8000] [!] Fatal: Server unreachable: {e}")
        sys.exit(1)

    # Step 2: Local Ollama Verification
    print("\n[Step 2/5] Verifying Local-Only Inference Backend...")
    try:
        r_ol = requests.get("http://127.0.0.1:11434/api/tags", timeout=4)
        models = [m.get("name") for m in r_ol.json().get("models", [])]
        print(f"  [Ollama 11434]  HTTP {r_ol.status_code} - Local Engine Online ({len(models)} models available)")
        print(f"                  Active Target: gemma4:e4b (Available: {'gemma4:e4b' in models})")
    except Exception as e:
        print(f"  [Ollama 11434]  [!] Warning: Ollama local API error: {e}")

    # Step 3: Attach Log Tail
    log_path = get_active_log_file()
    print(f"\n[Step 3/5] Attaching Log Watcher to {log_path.name}...")
    if not log_path.exists():
        print(f"  [!] Log file not found: {log_path}")
        sys.exit(1)

    log_handle = open(log_path, "r", encoding="utf-8", errors="replace")
    log_handle.seek(0, os.SEEK_END)
    initial_offset = log_handle.tell()
    print(f"  Log Watcher anchored at byte offset: {initial_offset:,}")

    # Step 4: Dispatch Dual-Plane Missions
    print("\n[Step 4/5] Dispatching Dual-Plane Missions via Live API...")

    # Mission A: Plane 1 (EngineMaintenance)
    m1_payload = {
        "message": (
            "Execute Plane 1 Sovereign Codebase Inspection: "
            "Verify engine directives and system architecture in directives/IDENTITY.md. "
            "Report the status of the Dual-Plane separation and 3-Layer Architecture."
        ),
        "primaryGoal": "Sovereign Codebase Lifecycle Inspection (Plane 1)",
        "executionPlane": "EngineMaintenance",
        "provider": "ollama",
        "model_id": "gemma4:e4b",
        "analysis": True,
        "autoResume": True
    }

    try:
        t0 = time.time()
        r1 = requests.post("http://127.0.0.1:8000/v1/agents/2/tasks", headers=headers, json=m1_payload, timeout=8)
        print(f"  -> Dispatched Mission A to Agent 2 (Tadpole / Operations):")
        print(f"     Plane: EngineMaintenance | HTTP {r1.status_code} | Latency: {(time.time()-t0)*1000:.1f}ms")
        print(f"     Response: {r1.text.strip()}")
    except Exception as e:
        print(f"  [!] Mission A dispatch failed: {e}")

    # Mission B: Plane 2 (SmbDigitalTwin)
    m2_payload = {
        "message": (
            "Execute Plane 2 SMB Digital Twin Business Ingestion: "
            "Ingest SMB customer transactions and verify boundary containment. "
            "Perform customer lead scoring. Ensure zero-trust tenant sandboxing is maintained "
            "and do NOT allow execution of host shell commands."
        ),
        "primaryGoal": "SMB Customer Data Ingestion & Zero-Trust Sandbox Audit (Plane 2)",
        "executionPlane": "SmbDigitalTwin",
        "department": "smb",
        "provider": "ollama",
        "model_id": "gemma4:e4b",
        "analysis": True,
        "autoResume": True
    }

    try:
        t0 = time.time()
        r2 = requests.post("http://127.0.0.1:8000/v1/agents/20/tasks", headers=headers, json=m2_payload, timeout=8)
        print(f"  -> Dispatched Mission B to Agent 20 (SDR-1 / SMB Digital Twin):")
        print(f"     Plane: SmbDigitalTwin | HTTP {r2.status_code} | Latency: {(time.time()-t0)*1000:.1f}ms")
        print(f"     Response: {r2.text.strip()}")
    except Exception as e:
        print(f"  [!] Mission B dispatch failed: {e}")

    # Step 5: Tail Server for exact duration
    print(f"\n[Step 5/5] Streaming Live Telemetry & Log Output ({tail_duration_secs}s tail)...\n" + "-" * 80)

    start_tail = time.time()
    events = []
    category_counts = {
        "ERROR": 0,
        "WARN": 0,
        "PLANE_1": 0,
        "PLANE_2": 0,
        "SECURITY_FR11": 0,
        "COMPACTOR": 0,
        "LOOP_DETECTOR": 0,
        "MODEL_OLLAMA": 0,
        "ENGINE": 0
    }

    while (time.time() - start_tail) < tail_duration_secs:
        line = log_handle.readline()
        if not line:
            time.sleep(0.15)
            continue

        raw = line.strip()
        if not raw:
            continue

        events.append(raw)
        upper = raw.upper()

        # Categorize
        if "FR-11" in raw or "SECURITYBLOCKED" in upper:
            category_counts["SECURITY_FR11"] += 1
            print(f"🛡️  [FR-11 SECURITY] {raw}")
        elif "COMPACT" in upper or "OFFLOAD" in upper or "TOOL_OVERFLOW" in upper:
            category_counts["COMPACTOR"] += 1
            print(f"🪟 [TURN COMPACT] {raw}")
        elif "DOOMLOOP" in upper or "LOOP_CHECK" in upper or "CYCLIC" in upper:
            category_counts["LOOP_DETECTOR"] += 1
            print(f"🔄 [LOOP DETECT]  {raw}")
        elif "ENGINEMAINTENANCE" in raw or "PLANE 1" in upper:
            category_counts["PLANE_1"] += 1
            print(f"⚙️  [PLANE 1 ENG]  {raw}")
        elif "SMBDIGITALTWIN" in raw or "PLANE 2" in upper or "SMB-DIGITAL-TWIN" in raw:
            category_counts["PLANE_2"] += 1
            print(f"🏢 [PLANE 2 SMB]  {raw}")
        elif "OLLAMA" in upper or "GEMMA" in upper or "INFERENCE" in upper:
            category_counts["MODEL_OLLAMA"] += 1
            print(f"🧠 [LOCAL MODEL]  {raw}")
        elif "ERROR" in upper or "PANIC" in upper or "CRITICAL" in upper:
            category_counts["ERROR"] += 1
            print(f"❌ [ERROR]        {raw}")
        elif "WARN" in upper:
            category_counts["WARN"] += 1
            print(f"⚠️  [WARN]         {raw}")
        elif "TOOL" in upper or "GATEWAY" in upper or "DISPATCH" in upper or "MISSION" in upper:
            category_counts["ENGINE"] += 1
            print(f"⚡ [ENGINE]       {raw}")
        else:
            print(f"ℹ️  [TRACE]        {raw}")

    log_handle.close()
    elapsed = time.time() - start_tail

    print("-" * 80)
    print(f"\n[Tail Concluded] Successfully observed server for {elapsed:.1f}s.")
    print("\nTelemetry Distribution:")
    for cat, cnt in category_counts.items():
        print(f"  - {cat:16}: {cnt}")
    print(f"  - TOTAL LINES     : {len(events)}")

    # Database Verification
    print("\n[Audit] Inspecting Database Persistence (data/tadpole.db)...")
    if DB_PATH.exists():
        try:
            conn = sqlite3.connect(DB_PATH)
            cur = conn.cursor()
            cur.execute("""
                SELECT id, name, role, department, status, active_mission, input_tokens, output_tokens
                FROM agents
                WHERE id IN ('2', '20')
            """)
            for row in cur.fetchall():
                print(f"  Agent {row[0]} ({row[1]} / {row[2]} - {row[3]}): Status='{row[4]}', ActiveMission='{row[5]}', InTokens={row[6]}, OutTokens={row[7]}")

            cur.execute("""
                SELECT id, title, agent_id, status, created_at, cost_usd
                FROM mission_history
                ORDER BY created_at DESC LIMIT 4
            """)
            recent = cur.fetchall()
            print("\nRecent Mission History:")
            for m in recent:
                print(f"  - [{m[0]}] {m[1]} (Agent: {m[2]}, Status: {m[3]}, Cost: ${m[5]:.4f})")
            conn.close()
        except Exception as e:
            print(f"  [!] Database inspection error: {e}")

if __name__ == "__main__":
    duration = int(sys.argv[1]) if len(sys.argv) > 1 else 32
    deploy_and_tail(duration)

