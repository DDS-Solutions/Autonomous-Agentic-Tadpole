#!/usr/bin/env python3
"""
@docs ARCHITECTURE:Infrastructure

### AI Assist Note
**🛡️ Tadpole OS: Pre-PR Quality Gate**
Quality verification gate for testing and code validation.

### 🔍 Debugging & Observability
- **Failure Path**: Fails if linting, tests, or check steps fail.
- **Telemetry Link**: Run audit log tracing.
"""
import sys
import os
import io
import subprocess
import json
from datetime import datetime
from pathlib import Path

# Force UTF-8 encoding on Windows for emoji support
if sys.platform == "win32":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
    sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8')

# Setup colors
class Colors:
    HEADER = '\033[95m'
    OK = '\033[92m'
    WARN = '\033[93m'
    FAIL = '\033[91m'
    END = '\033[0m'
    BOLD = '\033[1m'

def log_info(msg):
    print(f"[{Colors.HEADER}INFO{Colors.END}] {msg}")

def log_ok(msg):
    print(f"[{Colors.OK}OK{Colors.END}] {msg}")

def log_fail(msg):
    print(f"[{Colors.FAIL}FAIL{Colors.END}] {msg}")

def log_warn(msg):
    print(f"[{Colors.WARN}WARN{Colors.END}] {msg}")

def run_command(cmd, cwd=None):
    try:
        res = subprocess.run(cmd, shell=True, capture_output=True, text=True, encoding='utf-8', errors='replace', cwd=cwd)
        stdout = res.stdout if res.stdout is not None else ""
        stderr = res.stderr if res.stderr is not None else ""
        return res.returncode == 0, stdout, stderr
    except Exception as e:
        return False, "", str(e)

def main():
    workspace = Path(os.getcwd()).resolve()
    log_info(f"Starting Pre-PR Quality Gate at: {workspace}")
    
    start_time = datetime.now()
    report = {
        "timestamp": start_time.isoformat(),
        "checks": {},
        "verdict": "FAILED"
    }
    
    # 1. Backend Cargo Check
    log_info("Running Cargo Check (server-rs)...")
    cargo_ok, stdout, stderr = run_command("cargo check", cwd=str(workspace / "server-rs"))
    report["checks"]["cargo_check"] = {
        "passed": cargo_ok,
        "error": stderr if not cargo_ok else ""
    }
    if cargo_ok:
        log_ok("Cargo Check passed.")
    else:
        log_fail("Cargo Check failed. Details:")
        print(stderr[:500])

    # 2. Backend Cargo Clippy
    log_info("Running Cargo Clippy (server-rs)...")
    clippy_ok, stdout, stderr = run_command("cargo clippy --all-targets", cwd=str(workspace / "server-rs"))
    report["checks"]["cargo_clippy"] = {
        "passed": clippy_ok,
        "error": stderr if not clippy_ok else ""
    }
    if clippy_ok:
        log_ok("Cargo Clippy passed.")
    else:
        log_fail("Cargo Clippy failed. Details:")
        print(stderr[:500])
        print(stdout[:500])

    # 3. Frontend Linting
    log_info("Running Frontend Linting...")
    lint_ok, stdout, stderr = run_command("npm run lint", cwd=str(workspace))
    report["checks"]["frontend_lint"] = {
        "passed": lint_ok,
        "error": stderr if not lint_ok else ""
    }
    if lint_ok:
        log_ok("Frontend Linting passed.")
    else:
        log_fail("Frontend Linting failed. Details:")
        print(stderr[:500])
        print(stdout[:500])

    # 4. Frontend Vitest Runs
    log_info("Running Frontend Tests...")
    test_ok, stdout, stderr = run_command("npm run test", cwd=str(workspace))
    report["checks"]["frontend_tests"] = {
        "passed": test_ok,
        "error": stderr if not test_ok else ""
    }
    if test_ok:
        log_ok("Frontend Tests passed.")
    else:
        log_fail("Frontend Tests failed. Details:")
        print(stderr[:500])
        print(stdout[:500])
        
    # 5. Documentation & Version Parity Check
    log_info("Running Parity Guard...")
    parity_ok, stdout, stderr = run_command("python execution/parity_guard.py")
    report["checks"]["parity_guard"] = {
        "passed": parity_ok,
        "error": stderr if not parity_ok else ""
    }
    if parity_ok:
        log_ok("Parity Guard passed.")
    else:
        log_fail("Parity Guard failed. Details:")
        print(stdout[:500])

    # Final Summary
    all_passed = cargo_ok and clippy_ok and lint_ok and test_ok and parity_ok
    report["verdict"] = "PASSED" if all_passed else "FAILED"
    
    # Ensure reports dir exists
    reports_dir = workspace / "reports"
    reports_dir.mkdir(exist_ok=True)
    
    with open(reports_dir / "pre_pr_report.json", "w") as f:
        json.dump(report, f, indent=2)
        
    duration = (datetime.now() - start_time).total_seconds()
    log_info(f"Verification completed in {duration:.2f}s")
    
    if all_passed:
        log_ok("✨ PRE-PR GATE PASSED - Ready for review! ✨")
        sys.exit(0)
    else:
        log_fail("🚨 PRE-PR GATE FAILED - Fix warning/compile errors before committing. 🚨")
        sys.exit(1)

if __name__ == "__main__":
    main()
