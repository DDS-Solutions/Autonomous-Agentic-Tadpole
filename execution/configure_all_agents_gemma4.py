"""
@docs ARCHITECTURE:Agent:Configuration

### AI Assist Note
**🛡️ Tadpole OS: Configure All Agents to Local Gemma 4:e4b**
Updates all swarm agents in SQLite database, agents.json, and the live
Axum server memory (via REST PUT /v1/agents/{id}) to use the local Ollama
instance running 'gemma4:e4b'.

### 🔍 Debugging & Observability
- **Failure Path**: REST 400/500 on update, SQLite lock contention.
- **Telemetry Link**: Search `[configure_all_agents_gemma4]` in system logs.
"""

import json
import os
import sys
import sqlite3
import requests
from pathlib import Path

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass


ROOT = Path(__file__).resolve().parent.parent
DB_PATH = ROOT / "data" / "tadpole.db"
AGENTS_JSON = ROOT / "data" / "agents.json"
ENV_PATH = ROOT / ".env"

def get_neural_token() -> str:
    if not ENV_PATH.exists():
        return ""
    for line in ENV_PATH.read_text(encoding="utf-8").splitlines():
        if line.startswith("NEURAL_TOKEN="):
            return line.split("=", 1)[1].strip().strip('"').strip("'")
    return ""

def configure_agents():
    token = get_neural_token()
    headers = {
        "User-Agent": "TadpoleOS/1.1.58",
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json"
    }

    # 1. Update SQLite DB directly first
    conn = sqlite3.connect(DB_PATH)
    cursor = conn.cursor()
    cursor.execute("SELECT id, name FROM agents")
    all_agents = cursor.fetchall()
    print(f"[*] Found {len(all_agents)} agents in SQLite database.")

    cursor.execute("""
        UPDATE agents 
        SET provider = 'ollama',
            model_id = 'gemma4:e4b',
            base_url = 'http://127.0.0.1:11434/v1',
            model_2 = NULL,
            model_3 = NULL
    """)
    conn.commit()
    print("[+] Updated all agents in SQLite database to provider='ollama', model_id='gemma4:e4b'.")

    # 2. Update Live Running Server via REST API
    success_count = 0
    fail_count = 0
    for aid, name in all_agents:
        payload = {
            "provider": "ollama",
            "modelId": "gemma4:e4b",
            "baseUrl": "http://127.0.0.1:11434/v1"
        }
        try:
            r = requests.put(f"http://127.0.0.1:8000/v1/agents/{aid}", headers=headers, json=payload, timeout=5)
            if r.status_code == 200:
                success_count += 1
            else:
                print(f"  [!] Failed to update agent {aid} ({name}) via REST: {r.status_code} - {r.text}")
                fail_count += 1
        except Exception as e:
            print(f"  [!] REST request error for agent {aid}: {e}")
            fail_count += 1

    print(f"[+] Live Server Update: {success_count} succeeded, {fail_count} failed.")

    # 3. Update data/agents.json
    if AGENTS_JSON.exists():
        with open(AGENTS_JSON, "r", encoding="utf-8") as f:
            json_agents = json.load(f)

        existing_ids = set()
        for a in json_agents:
            aid = a.get("id") or a.get("identity", {}).get("id")
            existing_ids.add(str(aid))
            a["provider"] = "ollama"
            a["model_id"] = "gemma4:e4b"
            a["base_url"] = "http://127.0.0.1:11434/v1"
            if "model" in a:
                a["model"] = "gemma4:e4b"
            if "models" in a and isinstance(a["models"], dict):
                a["models"]["model_id"] = "gemma4:e4b"
                if "model" in a["models"] and isinstance(a["models"]["model"], dict):
                    a["models"]["model"]["provider"] = "ollama"
                    a["models"]["model"]["model_id"] = "gemma4:e4b"
                    a["models"]["model"]["base_url"] = "http://127.0.0.1:11434/v1"

        # If alpha was missing from json, add it from DB
        if "alpha" not in existing_ids:
            cursor.execute("SELECT id, name, role, department, description, skills, workflows FROM agents WHERE id = 'alpha'")
            alpha_row = cursor.fetchone()
            if alpha_row:
                skills_val = json.loads(alpha_row[5]) if alpha_row[5] else []
                wfs_val = json.loads(alpha_row[6]) if alpha_row[6] else []
                alpha_entry = {
                    "id": "alpha",
                    "name": alpha_row[1],
                    "role": alpha_row[2],
                    "department": alpha_row[3],
                    "description": alpha_row[4],
                    "provider": "ollama",
                    "model_id": "gemma4:e4b",
                    "base_url": "http://127.0.0.1:11434/v1",
                    "skills": skills_val,
                    "workflows": wfs_val,
                    "identity": {
                        "id": "alpha",
                        "name": alpha_row[1],
                        "role": alpha_row[2],
                        "department": alpha_row[3],
                        "description": alpha_row[4]
                    },
                    "models": {
                        "model_id": "gemma4:e4b",
                        "model": {
                            "provider": "ollama",
                            "model_id": "gemma4:e4b",
                            "base_url": "http://127.0.0.1:11434/v1"
                        }
                    },
                    "capabilities": {
                        "skills": skills_val,
                        "workflows": wfs_val
                    }
                }
                json_agents.append(alpha_entry)
                print("[+] Appended 'alpha' agent to data/agents.json.")

        with open(AGENTS_JSON, "w", encoding="utf-8") as f:
            json.dump(json_agents, f, indent=2)
        print(f"[+] Synchronized {len(json_agents)} agents in data/agents.json.")

    conn.close()

    # 4. Verify Live Memory via GET /v1/agents?per_page=50
    try:
        r = requests.get("http://127.0.0.1:8000/v1/agents?page=1&per_page=50", headers=headers, timeout=5)
        if r.status_code == 200:
            data = r.json()
            items = data.get("data", data.get("items", [])) if isinstance(data, dict) else data
            print(f"\n=== Verification of Live Agents ({len(items)}) ===")
            all_match = True
            for it in items:
                aid = it.get("id") or it.get("identity", {}).get("id")
                name = it.get("name") or it.get("identity", {}).get("name")
                provider = it.get("provider") or it.get("models", {}).get("model", {}).get("provider")
                model_id = it.get("model") or it.get("model_id") or it.get("models", {}).get("model_id")
                if str(provider).lower() != "ollama" or str(model_id).lower() != "gemma4:e4b":
                    print(f"  [X] MISMATCH: Agent {aid:5} ({name}) -> Provider: {provider}, Model: {model_id}")
                    all_match = False
                else:
                    print(f"  [✓] MATCH: Agent {aid:5} ({name:15}) -> {provider} / {model_id}")
            if all_match:
                print("\n[SUCCESS] 100% of live agents are verified on local gemma4:e4b!")
            else:
                print("\n[WARN] Some agents did not match gemma4:e4b.")
    except Exception as e:
        print("[!] Verification request failed:", e)

if __name__ == "__main__":
    configure_agents()

# [configure_all_agents_gemma4]

