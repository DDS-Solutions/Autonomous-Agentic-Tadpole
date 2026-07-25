#!/usr/bin/env python3
"""
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**Sovereign Swarm Suspend Script (v1.0.0)**
Determining and transitioning idle swarm agents to a suspended status in the primary database.
Prevents idle agents from consuming resources during autonomous swarm runs.

### 🔍 Debugging & Observability
- **Failure Path**: Database query or update failure, file access permissions.
- **Telemetry Link**: Search `[suspend_idle_agents]` in system logs.
"""

import sqlite3
import os
import sys
from pathlib import Path

WORKSPACE_ROOT = Path(__file__).resolve().parent.parent
DB_PATH = Path(os.getenv("DATABASE_URL", str(WORKSPACE_ROOT / "server-rs" / "tadpole.db")).replace("sqlite:", ""))

def main():
    if not DB_PATH.exists():
        print(f"Error: Database not found at {DB_PATH.absolute()}")
        return

    print(f"Connecting to database at {DB_PATH}...")
    conn = sqlite3.connect(DB_PATH)
    cursor = conn.cursor()

    # Core agents that must NEVER be suspended
    protected_agents = ('1', '2', 'alpha', 'general')

    # Find idle agents to suspend
    cursor.execute("""
        SELECT id, name, role, tokens_used, status 
        FROM agents 
        WHERE status = 'idle' 
          AND (tokens_used = 0 OR tokens_used IS NULL)
          AND id NOT IN (?, ?, ?, ?)
    """, protected_agents)

    idle_agents = cursor.fetchall()
    if not idle_agents:
        print("No idle, non-protected agents found for suspension.")
        conn.close()
        return

    print(f"Found {len(idle_agents)} idle agents to suspend:")
    for agent in idle_agents:
        print(f" - ID: {agent[0]}, Name: {agent[1]}, Role: {agent[2]} (Tokens: {agent[3]})")

    # Perform update
    agent_ids = [agent[0] for agent in idle_agents]
    placeholders = ",".join("?" * len(agent_ids))
    
    cursor.execute(f"""
        UPDATE agents 
        SET status = 'suspended' 
        WHERE id IN ({placeholders})
    """, agent_ids)
    
    conn.commit()
    print(f"\n[suspend_idle_agents] Successfully suspended {cursor.rowcount} agents in the database.")
    
    conn.close()

if __name__ == "__main__":
    main()

# Metadata: [suspend_idle_agents]
