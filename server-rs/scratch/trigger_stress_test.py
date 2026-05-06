"""
@docs ARCHITECTURE:Quality:Verification

### AI Assist Note
**Verification and quality assurance for the Tadpole OS engine.**
Advanced agentic logic and tool orchestration for the Tadpole OS swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Script error, API failure, or logic drift in the 3-layer architecture.
- **Telemetry Link**: Search `[trigger_stress_test]` in system logs.
"""

import requests
import json
import time
import uuid

# Configuration
API_BASE = "http://localhost:8000/v1"
TOKEN = "Tadpole-OS-2026"
AGENT_ID = "2" # Tadpole (COO) - Has spawn_subagent authority

headers = {
    "Authorization": f"Bearer {TOKEN}",
    "Content-Type": "application/json"
}

def trigger_stress_test():
    print(f"[Stress Test] Initiating mass swarm recruitment via Agent {AGENT_ID}...")
    
    # Mission: Recruit a batch of agents to stress the orchestrator and pulse visualizer
    payload = {
        "message": "COMMAND: Initiate a high-load swarm audit. Recruit 10 specialized agents in parallel (auditor_1, auditor_2, auditor_3, auditor_4, auditor_5, dev_1, dev_2, dev_3, dev_4, dev_5) to scan the filesystem for 'PROTOCOL_VIOLATION' strings. Synthesize results and report completion.",
        "primary_goal": "Stress test the orchestrator and pulse telemetry with 10+ concurrent sub-agents.",
        "cluster_id": str(uuid.uuid4())
    }
    
    try:
        response = requests.post(
            f"{API_BASE}/agents/{AGENT_ID}/tasks",
            headers=headers,
            json=payload,
            timeout=10
        )
        
        if response.status_code == 202:
            data = response.json()
            print(f"[Accepted] Mission dispatched successfully.")
            print(f"   Mission ID (Cluster): {payload['cluster_id']}")
            print(f"   Agent ID: {data.get('agent_id')}")
            return payload['cluster_id']
        else:
            print(f"[Error] Failed to dispatch mission: {response.status_code}")
            print(response.text)
            return None
            
    except Exception as e:
        print(f"[Exception] API connection failed: {e}")
        return None

def monitor_pulse(mission_id):
    if not mission_id:
        return
        
    print(f"[Monitoring] Watching Swarm Pulse for Mission {mission_id}...")
    for i in range(10):
        try:
            # Check the graph endpoint
            response = requests.get(f"{API_BASE}/agents/graph", headers=headers)
            if response.status_code == 200:
                graph = response.json()
                nodes = graph.get("nodes", [])
                links = graph.get("links", [])
                active_nodes = [n for n in nodes if n.get("status") == "Running"]
                print(f"   [T+{i*3}s] Active Nodes: {len(active_nodes)} | Total Links: {len(links)}")
                if len(active_nodes) > 1:
                    print(f"   Nodes: {', '.join([n.get('id') for n in active_nodes])}")
            
            # Check for ghost missions in logs (if we could, but we can't easily tail here)
            # So we just wait and see if the mission stays active
            time.sleep(3)
        except Exception as e:
            print(f"⚠️ [Monitor] Error: {e}")
            time.sleep(3)

if __name__ == "__main__":
    mid = trigger_stress_test()
    if mid:
        monitor_pulse(mid)

# Metadata: [trigger_stress_test]
