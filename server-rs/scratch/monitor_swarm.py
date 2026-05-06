"""
@docs ARCHITECTURE:Core

### AI Assist Note
**Core technical resource for the Tadpole OS Sovereign infrastructure.**
Advanced agentic logic and tool orchestration for the Tadpole OS swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Script error, API failure, or logic drift in the 3-layer architecture.
- **Telemetry Link**: Search `[monitor_swarm]` in system logs.
"""

import requests
import time

headers = {"Authorization": "Bearer Tadpole-OS-2026"}
url = "http://localhost:8000/v1/agents/graph"

print("Starting live swarm monitoring...")
for i in range(15):
    try:
        r = requests.get(url, headers=headers)
        if r.status_code == 200:
            data = r.json()
            nodes = data.get("nodes", [])
            links = data.get("links", [])
            running = [n for n in nodes if n.get("status") == "Running"]
            print(f"Iteration {i}: Running Nodes={len(running)}, Total Links={len(links)}")
            if len(running) > 0:
                print(f"  IDs: {', '.join([n.get('id') for n in running])}")
        else:
            print(f"Error: {r.status_code}")
    except Exception as e:
        print(f"Exception: {e}")
    time.sleep(4)

# Metadata: [monitor_swarm]
