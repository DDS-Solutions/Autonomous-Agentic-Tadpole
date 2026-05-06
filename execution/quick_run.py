"""
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**Quick Run Utility**
Dispatches a baseline mission to the Alpha Agent to verify kernel integrity.

### 🔍 Debugging & Observability
- **Failure Path**: Connection timeout, 401 Unauthorized, or Agent busy.
- **Telemetry Link**: Search `[quick_run]` in system logs.
"""

import requests
import os

# Standard Tadpole OS configuration
NEURAL_TOKEN = os.getenv("NEURAL_TOKEN", "Tadpole-OS-2026")
AGENT_ID = "1"
BASE_URL = "http://127.0.0.1:8000/v1"

def run():
    print(f"[QuickRun] Awakening Alpha Agent (ID: {AGENT_ID})...")
    
    url = f"{BASE_URL}/agents/{AGENT_ID}/tasks"
    headers = {
        "Authorization": f"Bearer {NEURAL_TOKEN}",
        "Content-Type": "application/json"
    }
    
    payload = {
        "message": "Initiate a Sovereign Kernel self-diagnostic. Scan for blocked actors and verify bounded channel health.",
        "safe_mode": True
    }
    
    try:
        response = requests.post(url, headers=headers, json=payload, timeout=30)
        if response.status_code == 202:
            print("[QuickRun] Task Accepted. Alpha Agent is now active.")
            print(f"Tracking Mission via WebSocket: {BASE_URL}/engine/ws")
        else:
            print(f"[QuickRun] Dispatch failed with status {response.status_code}")
            print(f"Response: {response.text}")
    except Exception as e:
        print(f"[QuickRun] Critical connection error: {e}")

if __name__ == "__main__":
    run()

# Metadata: [quick_run]
