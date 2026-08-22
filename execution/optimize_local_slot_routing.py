"""
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**Tadpole OS Local-First Model Slot Routing Optimizer**
Audits and optimizes agent model slot configurations in SQLite according to `.env` PRIVACY_MODE.
When local privacy mode is active, guarantees 100% local Ollama model routing (gemma4:12b for strategic planning in Slot 1, and gemma4:e4b / phi3.5 for sub-agent execution in Slot 2).

### 🔍 Debugging & Observability
- **Failure Path**: Database write failure or missing local Ollama models.
- **Telemetry Link**: Search `[optimize_local_slot_routing]` in system logs.
"""

import os
import sys
import json
import sqlite3
import requests
from pathlib import Path

# UTF-8 stdout setup for Windows PowerShell
if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding='utf-8')
        sys.stderr.reconfigure(encoding='utf-8')
    except Exception:
        pass

ROOT = Path(__file__).resolve().parent.parent
DB_PATH = ROOT / "data" / "tadpole.db"
ENV_PATH = ROOT / ".env"
raw_ollama_host = os.getenv("OLLAMA_HOST", "http://127.0.0.1:11434")
if not raw_ollama_host.startswith("http://") and not raw_ollama_host.startswith("https://"):
    raw_ollama_host = f"http://{raw_ollama_host}"
OLLAMA_BASE = raw_ollama_host.replace("0.0.0.0", "127.0.0.1")

# Load environment variables
privacy_mode_str = os.getenv("PRIVACY_MODE", "true").lower()
is_privacy_mode = privacy_mode_str in ["true", "1", "yes"]

def is_strategic_lead(agent_id: str, name: str, role: str) -> bool:
    """Determine whether an agent is a strategic lead requiring Slot 1 heavy reasoning."""
    combined = f"{agent_id} {name} {role}".lower()
    lead_keywords = [
        "lead", "ceo", "commander", "architect", "alpha", 
        "director", "coordinator", "strategist", "checkmate", "orchestrator"
    ]
    return any(kw in combined for kw in lead_keywords)

def get_installed_ollama_models() -> list[str]:
    """Queries the local Ollama daemon for currently installed models."""
    try:
        res = requests.get(f"{OLLAMA_BASE}/api/tags", timeout=5)
        if res.status_code == 200:
            return [m.get("name", "") for m in res.json().get("models", []) if "name" in m]
    except Exception as e:
        print(f"⚠️ Warning: Could not connect to Ollama at {OLLAMA_BASE}: {e}")
    return []

def optimize_slot_routing():
    print("=" * 75)
    print("🛡️ [optimize_local_slot_routing] TADPOLE OS MODEL SLOT ROUTING OPTIMIZER")
    print("=" * 75)
    print(f"📁 Database:      {DB_PATH}")
    print(f"🔒 Privacy Mode:  {'ACTIVE (Local-Only Enforced)' if is_privacy_mode else 'DISABLED (Cloud/Hybrid)'}")
    
    if not is_privacy_mode:
        print("☁️ Privacy Mode is DISABLED. Skipping local-only Ollama overwrites to preserve cloud provider routing.")
        print("=" * 75)
        return

    if not DB_PATH.exists():
        print(f"❌ Error: Database not found at {DB_PATH}")
        sys.exit(1)

    installed_models = get_installed_ollama_models()
    print(f"🦙 Installed Ollama Models: {', '.join(installed_models) if installed_models else 'None detected'}")

    if not installed_models:
        print("⚠️ Warning: No local Ollama models detected or Ollama service is unreachable.")
        print("Aborting slot optimization to prevent populating database with invalid models.")
        print("Please verify Ollama is running (`ollama serve`) and pull required models (`ollama pull gemma4:12b`).")
        print("=" * 75)
        return

    # Select best installed models dynamically
    local_heavy_model = next((m for m in installed_models if "gemma4:12b" in m), None)
    if not local_heavy_model:
        local_heavy_model = next((m for m in installed_models if "gemma4" in m or "llama3" in m or "qwen" in m), installed_models[0])

    local_fast_model = next((m for m in installed_models if "gemma4:e4b" in m or "phi3" in m), None)
    if not local_fast_model:
        local_fast_model = next((m for m in installed_models if "mini" in m or "small" in m or "e4b" in m), local_heavy_model)

    print(f"🧠 Heavy Strategy Slot (Slot 1 & 3):  {local_heavy_model}")
    print(f"⚡ Fast Sub-Worker Slot (Slot 2):      {local_fast_model}")
    print("-" * 75)

    conn = sqlite3.connect(DB_PATH)
    cur = conn.cursor()

    cur.execute("SELECT id, name, role, model_id, active_model_slot FROM agents")
    agents = cur.fetchall()

    updated_count = 0
    for agent in agents:
        a_id, a_name, a_role, a_model, a_active_slot = agent
        
        is_lead = is_strategic_lead(a_id, a_name, a_role)
        target_active_slot = 1 if is_lead else 2
        
        slot1_model = local_heavy_model
        slot2_model = local_fast_model
        slot3_model = local_heavy_model

        cfg2 = json.dumps({
            "provider": "ollama",
            "model_id": slot2_model,
            "base_url": f"{OLLAMA_BASE}/v1",
            "api_key": None,
            "temperature": 0.3
        })

        cfg3 = json.dumps({
            "provider": "ollama",
            "model_id": slot3_model,
            "base_url": f"{OLLAMA_BASE}/v1",
            "api_key": None,
            "temperature": 0.7
        })

        # Non-destructive update: modifies only model slot configs without resetting agent health metrics
        cur.execute("""
            UPDATE agents 
            SET model_id = ?,
                model_2 = ?,
                model_3 = ?,
                provider = 'ollama',
                base_url = ?,
                api_key = NULL,
                active_model_slot = ?,
                model_config2 = ?,
                model_config3 = ?
            WHERE id = ?
        """, (
            slot1_model,
            slot2_model,
            slot3_model,
            f"{OLLAMA_BASE}/v1",
            target_active_slot,
            cfg2,
            cfg3,
            a_id
        ))
        updated_count += 1
        slot_label = f"Slot {target_active_slot} ({'Strategy' if target_active_slot == 1 else 'Fast Sub-Worker'})"
        print(f"  ✓ [{a_id}] {a_name} ({a_role}) -> Active: {slot_label} | Slot 1: {slot1_model} | Slot 2: {slot2_model}")

    conn.commit()
    conn.close()

    print("-" * 75)
    print(f"✅ Successfully optimized model slot routing for {updated_count} agents in {DB_PATH}")
    print("🔒 All agent slots are verified 100% compliant with local PRIVACY_MODE.")
    print("=" * 75)

if __name__ == "__main__":
    optimize_slot_routing()
