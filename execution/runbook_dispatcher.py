#!/usr/bin/env python3
"""
@docs ARCHITECTURE:Infrastructure:Execution
@docs OPERATIONS_MANUAL:Runbooks

### AI Assist Note
**Runbook Dispatcher**: Translates common operational symptoms into diagnostic
or resolution commands, assisting operators and autonomous models in triage.

### 🔍 Debugging & Observability
- **Failure Path**: Invalid symptom argument or outdated script maps.
- **Telemetry Link**: Search `[runbook_dispatcher]` in logs.
"""

import sys

RUNBOOKS = {
    "engine-down": "taskkill /F /IM server-rs.exe",
    "port-in-use": "taskkill /F /IM server-rs.exe",
    "token-missing": "python execution/rotate_token.py",
    "sqlite-locked": "python execution/db_health_check.py",
    "sqlite-corrupt": "python execution/restore_sqlite.py <backup_file>",
    "swarm-partition": "python execution/swarm_stress_test.py --partition",
    "budget-exhausted": "python execution/quick_run.py --audit-budget",
    "cost-spike": "python execution/parity_guard.py --check=budget",
    "data-corrupt": "python execution/restore_agents.py <source_json> <dest_db>",
    "token-compromised": "python execution/rotate_token.py",
    "context-drift": "python execution/verify_ai_context.py",
    "context-fix": "python execution/verify_ai_context.py --fix"
}

def main():
    if len(sys.argv) < 2:
        print("Usage: python execution/runbook_dispatcher.py <symptom>")
        print("Available symptoms:")
        for s in sorted(RUNBOOKS.keys()):
            print(f"  - {s}")
        sys.exit(1)
        
    symptom = sys.argv[1].lower()
    if symptom not in RUNBOOKS:
        print(f"❌ Unknown symptom: '{symptom}'")
        print("Run without arguments to list available symptoms.")
        sys.exit(1)
        
    print(f"📋 Remediation Command for '{symptom}':")
    print(f"   {RUNBOOKS[symptom]}")

if __name__ == "__main__":
    main()
