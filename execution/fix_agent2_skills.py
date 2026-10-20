"""
@docs ARCHITECTURE:Agent:Skills

### AI Assist Note
**🛡️ Tadpole OS: Agent 2 Skills Sanitizer & Circuit Breaker Reset**
Cleans legacy/casing-drifted skill subscriptions from SQLite and resets the circuit breaker.

### 🔍 Debugging & Observability
- Telemetry Link: Search `[fix_agent2_skills]` in system logs.
"""

import sqlite3
import json
import requests
import sys
from pathlib import Path

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass

print("[fix_agent2_skills] Initializing agent 2 skill remediation...")

ROOT = Path(__file__).resolve().parent.parent
DB_PATH = ROOT / "data" / "tadpole.db"
ENV_PATH = ROOT / ".env"

def get_neural_token() -> str:
    if not ENV_PATH.exists():
        return ""
    for line in ENV_PATH.read_text(encoding="utf-8").splitlines():
        if line.startswith("NEURAL_TOKEN="):
            return line.split("=", 1)[1].strip().strip('"').strip("'")
    return ""

token = get_neural_token()
headers = {
    "Authorization": f"Bearer {token}",
    "User-Agent": "TadpoleOS/1.1.58",
    "Content-Type": "application/json"
}

# 1. Update SQLite database
conn = sqlite3.connect(DB_PATH)
cur = conn.cursor()
cur.execute("DELETE FROM skill_subscriptions WHERE agent_id = '2' AND skill_id = 'Deep Research'")
clean_skills = ["schedule_meeting", "Resource Check", "parity_guard", "security_scan"]
cur.execute("UPDATE agents SET skills = ? WHERE id = '2'", (json.dumps(clean_skills),))
conn.commit()
conn.close()
print("[fix_agent2_skills] SQLite: Deleted pending 'Deep Research' subscription and cleaned skills array.")

# 2. Update Live Axum server memory
r = requests.put("http://127.0.0.1:8000/v1/agents/2", headers=headers, json={"skills": clean_skills})
print(f"[fix_agent2_skills] Live REST API PUT /v1/agents/2: HTTP {r.status_code}")
