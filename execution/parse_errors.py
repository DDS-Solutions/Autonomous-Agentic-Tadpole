#!/usr/bin/env python3
"""
@docs ARCHITECTURE:Infrastructure:Execution
@docs OPERATIONS_MANUAL:Runbooks

### AI Assist Note
**Compiler Error Parser**: Runs `cargo check` and re-formats Rust compiler
errors and warnings into a structured, machine-readable JSON format, enabling
autonomous agents to execute self-healing code repair loops.

### 🔍 Debugging & Observability
- **Failure Path**: Command failures or unexpected rustc diagnostics format changes.
- **Telemetry Link**: Search `[parse_errors]` in logs.
"""

import sys
import os
import json
import subprocess
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

def run_cargo_check() -> str:
    """Runs cargo check in the server-rs directory and returns its stderr."""
    server_dir = ROOT / "server-rs"
    # Ensure cargo check outputs compiler messages in JSON format
    result = subprocess.run(
        ["cargo", "check", "--message-format=json"],
        cwd=server_dir,
        capture_output=True,
        text=True
    )
    return result.stdout

def parse_cargo_json(stdout_data: str) -> list:
    errors = []
    for line in stdout_data.splitlines():
        if not line.strip():
            continue
        try:
            msg = json.loads(line)
            # We are interested in compiler-message types
            if msg.get("reason") == "compiler-message":
                message = msg.get("message", {})
                level = message.get("level")
                # Filter for errors and warnings
                if level in ("error", "warning"):
                    spans = message.get("spans", [])
                    if spans:
                        primary_span = next((s for s in spans if s.get("is_primary")), spans[0])
                        file_path = primary_span.get("file_name")
                        line_num = primary_span.get("line_start")
                        col_num = primary_span.get("column_start")
                        code = message.get("code")
                        code_str = code.get("code") if code else "None"
                        
                        errors.append({
                            "file": file_path,
                            "line": line_num,
                            "column": col_num,
                            "level": level,
                            "code": code_str,
                            "message": message.get("message"),
                            "rendered": message.get("rendered")
                        })
        except json.JSONDecodeError:
            continue
    return errors

def main():
    print("🏗️ Running compiler check...")
    stdout_output = run_cargo_check()
    errors = parse_cargo_json(stdout_output)
    
    output_path = ROOT / ".tmp" / "compiler_errors.json"
    output_path.parent.mkdir(parents=True, exist_ok=True)
    
    report = {
        "summary": {
            "total_errors": len([e for e in errors if e["level"] == "error"]),
            "total_warnings": len([e for e in errors if e["level"] == "warning"]),
        },
        "diagnostics": errors
    }
    
    output_path.write_text(json.dumps(report, indent=2))
    print(f"✅ Compiler check complete. {report['summary']['total_errors']} errors, {report['summary']['total_warnings']} warnings.")
    print(f"📂 Structured report saved to {output_path.relative_to(ROOT)}")
    
    # Exit with code 1 if errors found, so it can fail CI or build pipelines
    sys.exit(1 if report["summary"]["total_errors"] > 0 else 0)

if __name__ == "__main__":
    main()

# Metadata: [parse_errors]
