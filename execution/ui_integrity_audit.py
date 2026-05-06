"""
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**Core technical resource for the Tadpole OS Sovereign infrastructure.**
Advanced agentic logic and tool orchestration for the Tadpole OS swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Script error, API failure, or logic drift in the 3-layer architecture.
- **Telemetry Link**: Search `[ui_integrity_audit]` in system logs.
"""

import sys
import os
import json
import io
from pathlib import Path

# Ensure stdout handles UTF-8 on Windows
if sys.platform == "win32":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
    sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8')

def check_ui_integrity(root_path):
    print(f"🔍 Analyzing Frontend Integrity for: {root_path}")
    
    # 1. Verify index.html exists
    index_path = Path(root_path) / "index.html"
    if not index_path.exists():
        print("❌ CRITICAL: index.html not found.")
        return False
        
    # 2. Check for core UI components
    src_path = Path(root_path) / "src"
    essential_pages = ["App.tsx", "pages/Settings.tsx", "pages/Ops_Dashboard.tsx"]
    
    for page in essential_pages:
        p = src_path / page
        if not p.exists():
            print(f"❌ ERROR: Missing essential UI component: {page}")
            return False
        else:
            print(f"✅ Verified: {page}")

    # 3. Verify Settings Store integration
    settings_store = src_path / "stores" / "settings_store.ts"
    if settings_store.exists():
        with open(settings_store, 'r', encoding='utf-8') as f:
            content = f.read()
            if "enable_neural_handoff" in content and "sentinel_mode" in content:
                print("✅ Verified: Neural Handoff parameters in SettingsStore.")
            else:
                print("⚠️ WARNING: SettingsStore missing Neural Handoff fields.")
    
    print("\n🟢 Frontend Integrity Audit PASSED.")
    return True

if __name__ == "__main__":
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    success = check_ui_integrity(root)
    sys.exit(0 if success else 1)

# Metadata: [ui_integrity_audit]
