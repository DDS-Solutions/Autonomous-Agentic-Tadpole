"""
@docs ARCHITECTURE:Security:Permissions

### AI Assist Note
**🛡️ Tadpole OS: Seed Safe Permission Policies**
Ensures all safe read-only tools and system utilities (list_files, read_codebase_file,
get_current_time, calculate, search_global_vault) are explicitly set to 'allow'
in the permission_policies table to enable auto-confirm pass-through.

### 🔍 Debugging & Observability
- **Failure Path**: Missing SQL columns, locked database, or schema constraint violation.
- **Telemetry Link**: Search `[seed_safe_permission_policies]` in audit logs.
"""

import sqlite3
import sys
from pathlib import Path

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass

ROOT = Path(__file__).resolve().parent.parent
db_path = ROOT / "data" / "tadpole.db"

def seed_safe_policies():
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()

    safe_tools = [
        "list_files",
        "read_codebase_file",
        "get_current_time",
        "calculate",
        "search_global_vault",
    ]

    print("=== Seeding Safe Tools in permission_policies ===")
    for tool in safe_tools:
        cursor.execute("""
            INSERT INTO permission_policies (tool_name, agent_id, mode)
            VALUES (?, NULL, 'allow')
            ON CONFLICT(tool_name, agent_id) DO UPDATE SET mode = excluded.mode
        """, (tool,))
        print(f"  [✓] {tool} -> allow")

    conn.commit()

    print("\n=== Current permission_policies in tadpole.db ===")
    cursor.execute("SELECT tool_name, agent_id, mode FROM permission_policies WHERE mode = 'allow'")
    rows = cursor.fetchall()
    for r in rows:
        print(f"  - {r[0]} (agent: {r[1]}): {r[2]}")

    conn.close()

if __name__ == "__main__":
    seed_safe_policies()

# [seed_safe_permission_policies]
