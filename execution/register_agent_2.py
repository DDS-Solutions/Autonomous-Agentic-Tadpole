"""
@docs ARCHITECTURE:Agent:Setup

### AI Assist Note
**🛡️ Tadpole OS: Agent 2 SQLite Bootstrapper**
Registers or updates Agent 2 (Tadpole/COO) in the SQLite agents table.
Performs PRAGMA column checks to ensure backward/forward schema compatibility.

### 🔍 Debugging & Observability
- **Failure Path**: sqlite3.OperationalError due to database locking or missing data/ directory, column mismatch.
- **Telemetry Link**: Search for `[AgentBoot]` in database registration logs.
"""
import sqlite3
import json
from datetime import datetime, timezone

DB_PATH = r"G:\Autonomous-Agentic-Tadpole\data\tadpole.db"

now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")

agent = {
    "id": "2",
    "name": "Tadpole",
    "role": "COO (Operations Director)",
    "department": "Operations",
    "description": "Chief Operations Officer. Orchestrates multi-agent swarm missions, recruits specialists, and synthesizes operational results into final reports.",
    "model_id": "gemma4:e4b",
    "provider": "ollama",
    "base_url": "http://127.0.0.1:11434/v1",
    "api_key": None,
    "system_prompt": None,
    "temperature": None,
    "tokens_used": 0,
    "input_tokens": 0,
    "output_tokens": 0,
    "status": "idle",
    "current_task": None,
    "theme_color": "#4fd1c5",
    "budget_usd": 100.0,
    "cost_usd": 0.0,
    "category": "ai",
    "requires_oversight": False,
    "failure_count": 0,
    "last_failure_at": None,
    "created_at": now,
    "heartbeat_at": None,
    "active_mission": None,
    "working_memory": json.dumps({}),
    "skills": json.dumps([
        "complete_mission", "recruit", "spawn_subagent",
        "fetch_url", "read_file", "write_file", "list_files",
        "get_file_contents", "grep_search", "get_agent_metrics",
        "query_financial_logs", "search_mission_knowledge", "update_working_memory"
    ]),
    "workflows": json.dumps(["resource_allocation"]),
    "mcp_tools": json.dumps([]),
    "connector_configs": json.dumps([]),
    "metadata": json.dumps({}),
    "model_2": None,
    "model_3": None,
    "model_config2": None,
    "model_config3": None,
    "active_model_slot": None,
    "voice_id": None,
    "voice_engine": None,
    "version": 1,
}

conn = sqlite3.connect(DB_PATH)
cursor = conn.cursor()

# Get columns from the actual table
cursor.execute("PRAGMA table_info(agents)")
cols = [row[1] for row in cursor.fetchall()]
print(f"[AgentBoot] DB columns ({len(cols)}): {cols}")

# Only insert columns that exist in this DB version
insert_data = {k: v for k, v in agent.items() if k in cols}
missing = [k for k in agent if k not in cols]
if missing:
    print(f"⚠️ [AgentBoot] Skipping columns not in DB: {missing}")

col_names = ", ".join(insert_data.keys())
placeholders = ", ".join(["?" for _ in insert_data])
values = list(insert_data.values())

sql = f"INSERT OR REPLACE INTO agents ({col_names}) VALUES ({placeholders})"
cursor.execute(sql, values)
conn.commit()
conn.close()

print(f"✅ [AgentBoot] Agent 2 (Tadpole/COO) registered in tadpole.db with provider=ollama, model=gemma4:e4b, base_url={agent['base_url']}")

# Metadata: [register_agent_2]

# Metadata: [register_agent_2]
