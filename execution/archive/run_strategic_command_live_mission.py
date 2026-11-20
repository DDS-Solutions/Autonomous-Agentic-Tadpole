"""
@docs ARCHITECTURE:Agent:SwarmTesting

### AI Assist Note
**🛡️ Tadpole OS: Strategic Command (cl-command) Live Mission & WebSocket Telemetry Stream**
Connects to ws://127.0.0.1:8000/v1/engine/ws with origin & bearer subprotocol authentication,
dispatches a live multi-agent operational mission to Strategic Command (cl-command) using local
Ollama gemma4:e4b, tails telemetry and logs for 30+ seconds through to terminal completion,
and extracts comprehensive mission debrief findings.

### 🔍 Debugging & Observability
- Telemetry Link: Search `[strategic_command_mission]` in system logs.
"""

import asyncio
import json
import os
import sys
import time
import sqlite3
import requests
import websockets
from datetime import datetime, timezone
from pathlib import Path

# Configure UTF-8 on Windows
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

async def live_mission_runner(tail_seconds: int = 32):
    print("=" * 80)
    print("🛡️  TADPOLE OS STRATEGIC COMMAND (cl-command) LIVE SWARM MISSION [strategic_command_mission]")
    print("Target Endpoints: Backend http://127.0.0.1:8000 | WebSocket ws://127.0.0.1:8000/v1/engine/ws")
    print("Inference Backend: LOCAL ONLY (Ollama / gemma4:e4b)")
    print(f"Tail Duration: {tail_seconds}s | Cluster: Strategic Command (cl-command)")
    print("=" * 80)

    token = get_neural_token()
    if not token:
        print("[!] Fatal: NEURAL_TOKEN not found in .env")
        return

    http_headers = {
        "User-Agent": "TadpoleOS/1.1.59",
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json",
        "x-tadpole-role": "overlord"
    }

    # Step 1: Pre-flight Verification
    print("\n[Step 1/5] Pre-flight Engine & Ollama Verifications...")
    try:
        r_health = requests.get("http://127.0.0.1:8000/health", headers=http_headers, timeout=4)
        print(f"  [Engine 8000] HTTP {r_health.status_code} - Online ({r_health.json().get('status')})")
    except Exception as e:
        print(f"  [Engine 8000] [!] Fatal: Rust engine unreachable: {e}")
        return

    try:
        r_ollama = requests.get("http://127.0.0.1:11434/api/tags", timeout=4)
        models = [m.get("name") for m in r_ollama.json().get("models", [])]
        print(f"  [Ollama 11434] HTTP {r_ollama.status_code} - gemma4:e4b Available: {'gemma4:e4b' in models}")
    except Exception as e:
        print(f"  [Ollama 11434] [!] Warning: Ollama unreachable: {e}")

    # Step 2: Establish Live Telemetry Stream
    print("\n[Step 2/5] Establishing WebSocket Stream on ws://127.0.0.1:8000/v1/engine/ws...")
    ws_uri = "ws://127.0.0.1:8000/v1/engine/ws"
    ws_headers = {
        "Origin": "http://127.0.0.1:8000",
        "User-Agent": "TadpoleOS/1.1.59",
        "Authorization": f"Bearer {token}"
    }
    subprotocols = [f"bearer.{token}", "tadpole-pulse-v1"]

    ws_messages = []
    stop_event = asyncio.Event()

    async def ws_listener(ws_conn):
        try:
            while not stop_event.is_set():
                try:
                    msg = await asyncio.wait_for(ws_conn.recv(), timeout=0.5)
                    ws_messages.append(msg)
                    try:
                        data = json.loads(msg)
                        msg_type = data.get("type", "unknown")
                        source = data.get("source", "")
                        text = data.get("text", "") or data.get("message", "")
                        if text:
                            print(f"  📡 [WS TELEMETRY] ({source} / {msg_type}): {text[:140]}")
                        elif msg_type == "agent:status" or msg_type == "agent:message":
                            print(f"  🤖 [SWARM EVENT] {msg_type}: {str(data)[:140]}")
                        elif msg_type in ["trace:span", "trace:event", "engine:health"]:
                            pass # High frequency tracing
                        else:
                            print(f"  📡 [WS EVENT] {msg_type}: {str(data)[:120]}")
                    except Exception:
                        pass
                except asyncio.TimeoutError:
                    continue
                except Exception as ex:
                    if not stop_event.is_set():
                        print(f"  [WS Error] {ex}")
                    break
        except Exception as e:
            print(f"  [WS Listener Failure] {e}")

    # Step 3: Anchor File Log Tailer
    log_path = get_active_log_file()
    print(f"\n[Step 3/5] Anchoring Log Watcher to {log_path.name}...")
    log_file = None
    if log_path.exists():
        log_file = open(log_path, "r", encoding="utf-8", errors="replace")
        log_file.seek(0, os.SEEK_END)
        print(f"  Log Watcher anchored at byte offset: {log_file.tell():,}")

    # Connect to WebSocket
    try:
        async with websockets.connect(
            ws_uri,
            additional_headers=ws_headers,
            subprotocols=subprotocols,
            ping_interval=20,
            ping_timeout=20
        ) as ws:
            print("  ✅ WebSocket connection authorized and live!")
            listener_task = asyncio.create_task(ws_listener(ws))

            # Step 4: Dispatch Strategic Command Mission (cl-command)
            print("\n[Step 4/5] Dispatching Strategic Command Swarm Mission (Cluster: cl-command)...")
            mission_payload = {
                "message": (
                    "Execute Strategic Command Mission for Cluster cl-command: "
                    "Conduct a full multi-agent swarm operational verification using local gemma4:e4b. "
                    "1. Use 'read_codebase_file' to inspect 'directives/IDENTITY.md' to verify Sovereign Tadpole OS identity. "
                    "2. Coordinate oversight inspection of system architecture and dual-plane boundaries. "
                    "3. Post verified telemetry findings via 'share_finding' and conclude cleanly with terminal completion status."
                ),
                "primary_goal": "Strategic Command Multi-Agent Swarm Operational Audit (cl-command)",
                "cluster_id": "cl-command",
                "execution_plane": "EngineMaintenance",
                "department": "Executive",
                "provider": "ollama",
                "model_id": "gemma4:e4b",
                "analysis": True,
                "auto_resume": True
            }

            dispatch_url = "http://127.0.0.1:8000/v1/agents/1/tasks"
            t0 = time.time()
            try:
                resp = requests.post(dispatch_url, headers=http_headers, json=mission_payload, timeout=8)
                print(f"  -> Dispatched to Agent 1 (Strategic Command Alpha / Agent of Nine):")
                print(f"     Status: HTTP {resp.status_code} | Latency: {(time.time()-t0)*1000:.1f}ms")
                print(f"     Response: {resp.text.strip()}")
            except Exception as e:
                print(f"  [!] Primary dispatch failed: {e}")

            # Step 5: Tail Server & Telemetry for Duration
            print(f"\n[Step 5/5] Streaming Live Telemetry & Log Output for {tail_seconds}s...")
            print("-" * 80)
            start_tail = time.time()
            log_events = []

            while (time.time() - start_tail) < tail_seconds:
                await asyncio.sleep(0.2)
                if log_file:
                    while True:
                        line = log_file.readline()
                        if not line:
                            break
                        raw = line.strip()
                        if not raw:
                            continue
                        log_events.append(raw)
                        upper = raw.upper()
                        if "CL-COMMAND" in upper or "STRATEGIC" in upper or "AGENT 1" in upper or "AGENT 2" in upper:
                            print(f"⭐ [COMMAND] {raw[:160]}")
                        elif "OLLAMA" in upper or "GEMMA" in upper:
                            print(f"🧠 [LOCAL MODEL] {raw[:160]}")
                        elif "TOOL" in upper or "DISPATCH" in upper or "RECEIPT" in upper or "RUNNER" in upper:
                            print(f"⚡ [RUNNER] {raw[:160]}")
                        elif "ERROR" in upper or "PANIC" in upper:
                            print(f"❌ [ERROR] {raw[:160]}")
                        elif "WARN" in upper:
                            print(f"⚠️  [WARN] {raw[:160]}")

            stop_event.set()
            await asyncio.sleep(0.5)
            listener_task.cancel()

    except Exception as e:
        print(f"[!] WebSocket session error: {e}")
    finally:
        if log_file:
            log_file.close()

    print("-" * 80)
    print(f"\n[Telemetry Stream Concluded] Observed {len(ws_messages)} WebSocket messages and {len(log_events)} log events.")

    # Step 6: Post-Mission Database Audit & Debrief
    print("\n[Audit] Querying Mission State & Database Artifacts...")
    if DB_PATH.exists():
        try:
            conn = sqlite3.connect(DB_PATH)
            cur = conn.cursor()
            cur.execute("""
                SELECT id, name, role, department, status, active_mission, current_task, input_tokens, output_tokens
                FROM agents
                WHERE id IN ('1', '2', '99')
            """)
            print("\nStrategic Command Agents:")
            for r in cur.fetchall():
                print(f"  - Agent {r[0]} ({r[1]} / {r[2]}): Status={r[4]}, Mission={r[5]}, Task={r[6]}, Tokens=In:{r[7]}/Out:{r[8]}")

            cur.execute("""
                SELECT id, agent_id, title, status, created_at, cost_usd
                FROM mission_history
                ORDER BY created_at DESC LIMIT 5
            """)
            print("\nRecent Mission History:")
            recent_missions = cur.fetchall()
            for m in recent_missions:
                print(f"  - Mission [{m[0]}] Agent:{m[1]} | Status:{m[3]} | Cost:${m[5]:.4f} | Title: {m[2][:60]}")

            if recent_missions:
                latest_id = recent_missions[0][0]
                cur.execute("""
                    SELECT source, text, severity, timestamp
                    FROM mission_logs
                    WHERE mission_id = ?
                    ORDER BY timestamp ASC LIMIT 15
                """, (latest_id,))
                mission_logs = cur.fetchall()
                print(f"\nTelemetry Logs for Mission [{latest_id}]:")
                for l in mission_logs:
                    print(f"    [{l[3]}] ({l[0]} / {l[2]}): {l[1][:140]}")

            conn.close()
        except Exception as e:
            print(f"  [!] Database audit failed: {e}")

if __name__ == "__main__":
    duration = int(sys.argv[1]) if len(sys.argv) > 1 else 32
    asyncio.run(live_mission_runner(duration))
