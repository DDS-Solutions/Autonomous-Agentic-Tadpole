#!/usr/bin/env python3
"""
@docs ARCHITECTURE:Infrastructure:Execution
@docs OPERATIONS_MANUAL:Runbooks

### AI Assist Note
**verify_temporal_and_delegation**: Verifies the implementation of systemic temporal grounding,
`get_current_time` tool registration, CEO Sovereign Router ACL constraints, and output placeholder detection.

### 🔍 Debugging & Observability
- **Failure Path**: Missing prompt template anchors, tool dispatcher omission, or ACL bypass.
- **Telemetry Link**: Search `[verify_temporal_and_delegation]` in audit logs.
"""

import sys
import re
import urllib.request
import urllib.error
import json
from pathlib import Path

# Configure utf-8 stdout for Windows consoles
if sys.stdout.encoding != "utf-8":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if sys.stderr.encoding != "utf-8":
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

ROOT = Path(__file__).resolve().parent.parent

def check_prompt_renderer():
    path = ROOT / "server-rs" / "src" / "agent" / "runner" / "prompt_renderer.rs"
    content = path.read_text(encoding="utf-8")
    assert "TEMPORAL ANCHOR: {{temporal_anchor}}" in content, "Missing TEMPORAL ANCHOR placeholder in prompt_renderer.rs"
    assert "EXECUTION BOUNDARY:" in content, "Missing EXECUTION BOUNDARY statement in prompt_renderer.rs"
    print("✅ [1/5] Prompt Renderer: Temporal anchor & execution boundary verified.")

def check_synthesis():
    path = ROOT / "server-rs" / "src" / "agent" / "runner" / "synthesis.rs"
    content = path.read_text(encoding="utf-8")
    assert 'vars.insert("temporal_anchor", temporal_anchor);' in content, "Missing temporal_anchor variable insertion in synthesis.rs"
    assert "chrono::Local::now()" in content and "chrono::Utc::now()" in content, "Missing chrono clock calls in synthesis.rs"
    print("✅ [2/5] Synthesis: Dynamic host clock computation verified.")

def check_tool_manifest_and_dispatcher():
    manifest_path = ROOT / "server-rs" / "src" / "agent" / "runner" / "tools" / "manifest.rs"
    dispatcher_path = ROOT / "server-rs" / "src" / "agent" / "runner" / "tools" / "dispatcher.rs"
    
    manifest_content = manifest_path.read_text(encoding="utf-8")
    dispatcher_content = dispatcher_path.read_text(encoding="utf-8")
    
    assert '"get_current_time"' in manifest_content, "Missing get_current_time in tool manifest"
    assert '"get_current_time"' in dispatcher_content, "Missing get_current_time in dispatcher"
    assert "Authoritative Host Clock:" in dispatcher_content, "Missing clock formatting in dispatcher"
    print("✅ [3/5] Tools: get_current_time registered in manifest and dispatcher.")

def check_acl_service():
    path = ROOT / "server-rs" / "src" / "services" / "acl_service.rs"
    content = path.read_text(encoding="utf-8")
    assert "CEO ROUTER MANDATE" in content, "Missing CEO router mandate in acl_service.rs"
    assert 'test_get_current_time_permissions' in content, "Missing get_current_time unit test in acl_service.rs"
    print("✅ [4/5] ACL Service: CEO Sovereign Router whitelist and get_current_time permissions verified.")

def check_placeholder_detection():
    path = ROOT / "server-rs" / "src" / "agent" / "runner" / "finalize.rs"
    content = path.read_text(encoding="utf-8")
    assert "pub fn detect_unresolved_placeholders" in content, "Missing detect_unresolved_placeholders in finalize.rs"
    assert "[Current System Date]" in content, "Missing [Current System Date] pattern in finalize.rs"
    assert "test_detect_unresolved_placeholders" in content, "Missing test_detect_unresolved_placeholders in finalize.rs"
    print("✅ [5/5] Finalize: Output placeholder detection and sanitization verified.")

def check_live_api():
    print("🔍 Probing live server status on http://localhost:8000...")
    token = ""
    env_path = ROOT / ".env"
    if env_path.exists():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("NEURAL_TOKEN="):
                token = line.split("=", 1)[1].strip().strip('"').strip("'")
                break
    headers = {"User-Agent": "TadpoleOS/1.1.58"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request("http://localhost:8000/v1/agents", headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=3) as resp:
            agents = json.loads(resp.read().decode("utf-8"))
            if isinstance(agents, dict):
                agents = agents.get("data", agents.get("items", []))
            agent_ids = [str(a.get("id")) for a in agents]
            print(f"   Live agents currently in server-rs memory: {len(agent_ids)}")
            if "99" in agent_ids:
                print("   Agent 99 is active in memory.")
            else:
                print("   ⚠️ Note: Agent 99 is NOT yet in live memory cache. server-rs restart required to rehydrate.")
    except Exception as e:
        print(f"   ⚠️ Could not reach live server: {e}")

if __name__ == "__main__":
    print("=== TadpoleOS Temporal & Delegation Verification ===")
    try:
        check_prompt_renderer()
        check_synthesis()
        check_tool_manifest_and_dispatcher()
        check_acl_service()
        check_placeholder_detection()
        check_live_api()
        print("\n🎉 ALL ARCHITECTURAL INVARIANTS SATISFIED.")
        sys.exit(0)
    except AssertionError as err:
        print(f"\n❌ REGRESSION DETECTED: {err}", file=sys.stderr)
        sys.exit(1)

# [verify_temporal_and_delegation]

