import requests
import json
import os

def dispatch_mission():
    url = "http://127.0.0.1:8000/v1/agents/2/tasks"
    headers = {
        "Content-Type": "application/json",
        "Authorization": "Bearer Tadpole-OS-2026"
    }
    
    payload = {
        "message": "Execute a high-scrutiny infrastructure audit. Use 'security_scan' to check for vulnerabilities, 'verify_ai_context' to check alignment, and 'parity_guard' to verify system parity. Report all findings in a structured summary. DO NOT ASK FOR PERMISSION. JUST CALL THE TOOLS."
    }
    
    print(f"[Dispatcher] Sending mission to Agent 2 (Tadpole)...")
    try:
        response = requests.post(url, headers=headers, json=payload)
        if response.status_code == 200:
            print(f"[Dispatcher] Mission dispatched successfully!")
            print(f"Response: {json.dumps(response.json(), indent=2)}")
        else:
            print(f"[Dispatcher] Failed to dispatch mission: {response.status_code}")
            print(f"Error: {response.text}")
    except Exception as e:
        print(f"[Dispatcher] Error connecting to server: {e}")

if __name__ == "__main__":
    dispatch_mission()
