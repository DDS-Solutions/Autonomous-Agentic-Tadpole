"""
@docs ARCHITECTURE:Core

### AI Assist Note
**Core technical resource for the Tadpole OS Sovereign infrastructure.**
Advanced agentic logic and tool orchestration for the Tadpole OS swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Script error, API failure, or logic drift in the 3-layer architecture.
- **Telemetry Link**: Search `[check_db]` in system logs.
"""

import sqlite3

db_path = "../data/tadpole.db"
try:
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()
    cursor.execute("PRAGMA table_info(mission_history);")
    columns = cursor.fetchall()
    print("Columns in agent_directives:")
    for c in columns:
        print(f"  - {c[1]} ({c[2]})")
    conn.close()
except Exception as e:
    print(f"Error: {e}")

# Metadata: [check_db]
