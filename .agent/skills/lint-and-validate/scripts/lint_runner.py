#!/usr/bin/env python3
"""
@docs ARCHITECTURE:SovereignArchitecture

### AI Assist Note
**🛡️ Unified Lint Runner**: Standardizes the linting and type-checking process across multi-language projects in the Tadpole OS swarm. 
Automatically detects Node.js (ESLint/TSC) or Python (Ruff/Mypy) environments and executes the appropriate compliance checks.

### 🔍 Debugging & Observability
- **Failure Path**: Linting rule violation or configuration mismatch in the sovereign environment.
- **Telemetry Link**: Search `[lint_runner]` in system logs.
"""

import subprocess
import sys
import json
import platform
import shutil
from pathlib import Path
from datetime import datetime

# Fix Windows console encoding
try:
    sys.stdout.reconfigure(encoding='utf-8', errors='replace')
except:
    pass


def detect_project_type(project_path: Path) -> dict:
    """Detect project type and available linters across Node, Python, and Rust."""
    result = {
        "type": "unknown",
        "ecosystems": [],
        "linters": [],
        "warnings": []
    }
    
    # Node.js project
    package_json = project_path / "package.json"
    if package_json.exists():
        try:
            pkg = json.loads(package_json.read_text(encoding='utf-8'))
            scripts = pkg.get("scripts", {})
            deps = {**pkg.get("dependencies", {}), **pkg.get("devDependencies", {})}
            
            result["ecosystems"].append("node")
            
            # Check for lint script
            if "lint" in scripts:
                result["linters"].append({"name": "npm lint", "cmd": ["npm", "run", "lint"]})
            elif "eslint" in deps:
                result["linters"].append({"name": "eslint", "cmd": ["npx", "eslint", "."]})
            
            # Check for TypeScript
            if "typescript" in deps or (project_path / "tsconfig.json").exists():
                result["linters"].append({"name": "tsc", "cmd": ["npx", "tsc", "--noEmit"]})
                
        except json.JSONDecodeError as e:
            result["warnings"].append(f"package.json JSONDecodeError: {e}")
        except Exception as e:
            result["warnings"].append(f"package.json read error: {e}")
    
    # Python project
    if (project_path / "pyproject.toml").exists() or (project_path / "requirements.txt").exists() or (project_path / "execution").exists():
        result["ecosystems"].append("python")
        
        # Check for ruff
        if shutil.which("ruff"):
            result["linters"].append({"name": "ruff", "cmd": ["ruff", "check", "."]})
        
        # Check for mypy
        if (project_path / "mypy.ini").exists() or (project_path / "pyproject.toml").exists():
            if shutil.which("mypy"):
                result["linters"].append({"name": "mypy", "cmd": ["mypy", "."]})

    # Rust project (Root workspace or server-rs)
    cargo_root = project_path / "Cargo.toml"
    server_cargo = project_path / "server-rs" / "Cargo.toml"
    if cargo_root.exists() or server_cargo.exists():
        result["ecosystems"].append("rust")
        if shutil.which("cargo"):
            if cargo_root.exists():
                result["linters"].append({
                    "name": "cargo check",
                    "cmd": ["cargo", "check", "--workspace"],
                    "cwd": str(project_path)
                })
            elif server_cargo.exists():
                result["linters"].append({
                    "name": "cargo check (server-rs)",
                    "cmd": ["cargo", "check"],
                    "cwd": str(project_path / "server-rs")
                })
    
    if not result["ecosystems"]:
        result["type"] = "unknown"
    elif len(result["ecosystems"]) == 1:
        result["type"] = result["ecosystems"][0]
    else:
        result["type"] = f"polyglot ({', '.join(result['ecosystems'])})"

    return result


def run_linter(linter: dict, cwd: Path) -> dict:
    """Run a single linter and return results."""
    result = {
        "name": linter["name"],
        "passed": False,
        "output": "",
        "error": ""
    }
    
    try:
        cmd = list(linter["cmd"])
        target_cwd = Path(linter.get("cwd", cwd))
        
        # Windows compatibility for npm/npx
        if platform.system() == "Windows":
            if cmd[0] in ["npm", "npx"]:
                # Force .cmd extension on Windows
                if not cmd[0].lower().endswith(".cmd"):
                    cmd[0] = f"{cmd[0]}.cmd"
        
        proc = subprocess.run(
            cmd,
            cwd=str(target_cwd),
            capture_output=True,
            text=True,
            encoding='utf-8',
            errors='replace',
            timeout=120,
            shell=platform.system() == "Windows" # Shell=True often helps with path resolution on Windows
        )
        
        result["output"] = proc.stdout[:2000] if proc.stdout else ""
        result["error"] = proc.stderr[:500] if proc.stderr else ""
        result["passed"] = proc.returncode == 0
        
    except FileNotFoundError:
        result["error"] = f"Command not found: {linter['cmd'][0]}"
    except subprocess.TimeoutExpired:
        result["error"] = "Timeout after 120s"
    except Exception as e:
        result["error"] = str(e)
    
    return result


def main():
    print("[lint_runner] Initializing sovereign linting audit...")
    if len(sys.argv) < 2:
        project_path = Path(".").resolve()
    else:
        project_path = Path(sys.argv[1]).resolve()
    
    print(f"\n{'='*60}")
    print(f"[LINT RUNNER] Unified Linting")
    print(f"{'='*60}")
    print(f"Project: {project_path}")
    print(f"Time: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    
    # Detect project type
    project_info = detect_project_type(project_path)
    print(f"Type: {project_info['type']}")
    print(f"Linters: {len(project_info['linters'])}")
    print("-"*60)
    
    if not project_info["linters"]:
        print("No linters found for this project type.")
        output = {
            "script": "lint_runner",
            "project": str(project_path),
            "type": project_info["type"],
            "checks": [],
            "passed": True,
            "message": "No linters configured"
        }
        print(json.dumps(output, indent=2))
        sys.exit(0)
    
    # Run each linter
    results = []
    all_passed = True
    
    for linter in project_info["linters"]:
        print(f"\nRunning: {linter['name']}...")
        result = run_linter(linter, project_path)
        results.append(result)
        
        if result["passed"]:
            print(f"  [PASS] {linter['name']}")
        else:
            print(f"  [FAIL] {linter['name']}")
            if result["error"]:
                print(f"  Error: {result['error'][:200]}")
            all_passed = False
    
    # Summary
    print("\n" + "="*60)
    print("SUMMARY")
    print("="*60)
    
    for r in results:
        icon = "[PASS]" if r["passed"] else "[FAIL]"
        print(f"{icon} {r['name']}")
    
    output = {
        "script": "lint_runner",
        "project": str(project_path),
        "type": project_info["type"],
        "checks": results,
        "passed": all_passed
    }
    
    print("\n" + json.dumps(output, indent=2))
    
    sys.exit(0 if all_passed else 1)


if __name__ == "__main__":
    main()
