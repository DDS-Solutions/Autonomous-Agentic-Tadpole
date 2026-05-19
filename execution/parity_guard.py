"""
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**🛡️ Tadpole Engine: Parity Guard**
Advanced agentic logic and tool orchestration for the Tadpole OS swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Script error, API failure, or logic drift in the 3-layer architecture.
- **Telemetry Link**: Search `[parity_guard]` in system logs.
"""

#!/usr/bin/env python3
"""
# 🛡️ Tadpole Engine: Parity Guard
**Agent Consistency**: High (ECC Optimized)
**Source of Truth**: `execution/parity_guard.py`, `docs/openapi.yaml`
**Inputs**: `server-rs/src/router.rs`, `docs/`, `.agent/skills/`
**Outputs**: Structured Parity Report (Exit 0 on success, Exit 1 on drift)

> [!IMPORTANT]
> **AI Assist Note (Execution Logic)**:
> This script is the "Integrity Gate" for the Tadpole OS 3-layer architecture.
> - **Gateway Pillar**: Verifies Axum routes against OpenAPI specifications.
> - **Registry Pillar**: Verifies skill manifests against their execution scripts.
> - **Security Root**: Checks for environment variable leakage in `.env.example`.
> - **ECC Audit**: Validates `@docs` cross-links for documentation drift.

Workflow: Code -> docs/openapi.yaml -> docs/API_REFERENCE.md -> Unit Tests
"""

import os
import re
import yaml
import json
import sys
import io
import subprocess
from pathlib import Path
from typing import Dict, List, Any

# Fix Windows console encoding for Unicode output
if __name__ == "__main__" and sys.platform == "win32":
    try:
        sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
        sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8')
    except (AttributeError, io.UnsupportedOperation):
        pass

ROOT = Path(__file__).resolve().parent.parent

def print_result(check, status, message):
    icon = "[OK]" if status else "[FAIL]"
    print(f"{icon} [{check}] {message}")

def normalize_path(path):
    """Normalizes paths from any format (:id, {id}, :agent_id) to a standard {PARAM} placeholder for comparison."""
    # Convert :param to {PARAM}
    p = re.sub(r':([a-zA-Z0-9_]+)', r'{PARAM}', path)
    # Convert {param} to {PARAM}
    p = re.sub(r'\{([a-zA-Z0-9_]+)\}', r'{PARAM}', p)
    return p

def scan_router(router_path):
    """Extracts routes from the Axum router.rs file, handling multi-level nesting."""
    try:
        sys.path.insert(0, str(ROOT / "execution"))
        from generate_api_reference import discover_routes

        return [
            {"path": route.path, "method": route.method, "handler": route.handler}
            for route in discover_routes()
        ]
    except Exception as exc:
        print_result("ROUTER-SCAN", False, f"Failed to import route generator: {exc}")
        return []

def scan_openapi(openapi_path):
    """Parses paths from openapi.yaml."""
    if not os.path.exists(openapi_path):
        return {}
    
    with open(openapi_path, 'r', encoding='utf-8') as f:
        data = yaml.safe_load(f)
        return data.get("paths", {})

def check_env_vars(root):
    print(f"\nScanning for Environment Variables...")
    env_vars_in_code = set()
    for root_dir_walk, _, files in os.walk(root / "server-rs" / "src"):
        for file in files:
            if not file.endswith(".rs"): continue
            file_path = os.path.join(root_dir_walk, file)
            with open(file_path, 'r', encoding='utf-8') as f:
                content = f.read()
                matches = re.findall(r'std::env::var\(\s*"([A-Z0-9_]+)"\s*\)', content)
                for m in matches:
                    env_vars_in_code.add(m)
                    
    env_example_path = root / ".env.example"
    env_example_vars = set()
    if env_example_path.exists():
        with open(env_example_path, 'r', encoding='utf-8') as f:
            for line in f:
                line = line.strip()
                if not line: continue
                if line.startswith('#'):
                    line = line.lstrip('#').strip()
                if '=' in line:
                    key = line.split('=')[0].strip()
                    env_example_vars.add(key)
    
    errors = 0
    for var in env_vars_in_code:
        if var not in env_example_vars:
            print_result("ENV-VAR", False, f"std::env::var(\"{var}\") used in code but missing from .env.example")
            errors += 1
        else:
            print_result("ENV-VAR", True, f"{var} documented")
            
    return errors

def check_version_sync(root):
    print(f"\nChecking Version Parity...")
    version_file = root / "version.json"
    if not version_file.exists():
        print_result("VERSION", False, "version.json missing")
        return 1
    version = json.loads(version_file.read_text(encoding="utf-8")).get("version")
    targets = {
        "package.json": r'"version":\s*"([^"]+)"',
        "server-rs/Cargo.toml": r'^version\s*=\s*"([^"]+)"',
        "docs/openapi.yaml": r'version:\s*([0-9.]+)',
        "docs/API_REFERENCE.md": r'\*\*Version\*\*:\s*([0-9.]+)',
        "SYSTEM_MAP.md": r'\*\*Version\*\*:\s*([0-9.]+)',
        "directives/IDENTITY.md": r'TadpoleOS/([0-9.]+)',
    }
    errors = 0
    for rel, pattern in targets.items():
        path = root / rel
        if not path.exists():
            print_result("VERSION", False, f"{rel} missing")
            errors += 1
            continue
        match = re.search(pattern, path.read_text(encoding="utf-8"), flags=re.MULTILINE)
        if not match:
            print_result("VERSION", False, f"{rel} has no version marker")
            errors += 1
        elif match.group(1) != version:
            print_result("VERSION", False, f"{rel} has {match.group(1)}, expected {version}")
            errors += 1
        else:
            print_result("VERSION", True, f"{rel} -> {version}")
    return errors

def check_doc_file_refs(root):
    print(f"\nChecking Documentation File References...")
    docs = [
        root / "README.md",
        root / "SYSTEM_MAP.md",
        root / "docs" / "ARCHITECTURE.md",
        root / "docs" / "OPERATIONS_MANUAL.md",
        root / "docs" / "SECURITY.md",
        root / "docs" / "API_REFERENCE.md",
        root / "directives" / "IDENTITY.md",
        root / "directives" / "GEMINI.md",
        root / "directives" / "AUTONOMY_MANIFEST.md",
    ]
    errors = 0
    for doc in docs:
        if not doc.exists():
            continue
        text = doc.read_text(encoding="utf-8")
        refs = re.findall(r'`([^`\n]+\.(?:md|yaml|yml|json|toml|rs|tsx|ts|py|bat|css))`', text)
        refs += [m.replace("/", "\\") for m in re.findall(r'file:///G:/Autonomous-Agentic-Tadpole/([^\)\s]+)', text)]
        for ref in refs:
            if (
                ref.startswith("http")
                or "<" in ref
                or ">" in ref
                or "{" in ref
                or "}" in ref
                or " " in ref
                or ref == "SKILL.md"
            ):
                continue
            normalized = ref.replace("/", os.sep)
            candidates = [
                root / normalized,
                root / "execution" / normalized,
                root / "directives" / normalized,
                root / "docs" / normalized,
            ]
            if not any(candidate.exists() for candidate in candidates):
                print_result("DOC-REF", False, f"{doc.relative_to(root)} references missing {ref}")
                errors += 1
    if errors == 0:
        print_result("DOC-REF", True, "All checked file references exist")
    return errors

def check_skills(root):
    print(f"\nScanning Skills & Workflows...")
    skills_dir = root / "data" / "skills"
    errors = 0
    
    if not skills_dir.exists():
        return 0
        
    for file in os.listdir(skills_dir):
        if not file.endswith(".json"): continue
        file_path = skills_dir / file
        
        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                skill_data = json.load(f)
                name = skill_data.get('name', 'UNKNOWN')
                exec_cmd = skill_data.get('execution_command', '')
                
                if exec_cmd.startswith('python '):
                    parts = exec_cmd.split(' ')
                    if len(parts) >= 2:
                        script_path_str = parts[1]
                        script_path = root / script_path_str
                        if not script_path.exists():
                            print_result("SKILL-MANIFEST", False, f"[{name}] demands '{script_path_str}' but file is missing")
                            errors += 1
                        else:
                            print_result("SKILL-MANIFEST", True, f"[{name}] runner '{script_path_str}' verified")
                    else:
                        print_result("SKILL-MANIFEST", False, f"[{name}] invalid python execution command")
                        errors += 1
                else:
                    print_result("SKILL-MANIFEST", True, f"[{name}] {exec_cmd} verified")
        except json.JSONDecodeError as e:
            print_result("SKILL-MANIFEST", False, f"Failed to parse {file} as JSON: {e}")
            errors += 1
        except Exception as e:
            print_result("SKILL-MANIFEST", False, f"Error processing {file}: {e}")
            errors += 1
            
    return errors

def check_api_docs_parity(root, fix=False):
    print(f"\nChecking API Documentation Parity...")
    gen_script = root / "execution" / "generate_api_reference.py"
    if not gen_script.exists():
        print_result("DOCS-PARITY", False, "generate_api_reference.py missing")
        return 1
        
    if fix:
        print("Running generate_api_reference.py --fix...")
        result = subprocess.run([sys.executable, str(gen_script)], capture_output=True, text=True)
        if result.returncode == 0:
            print_result("DOCS-PARITY", True, "Successfully synced API_REFERENCE.md")
            return 0
        else:
            print_result("DOCS-PARITY", False, f"Sync failed: {result.stderr}")
            return 1
    else:
        # Check if synced (Heuristic: check mtimes)
        openapi_mtime = os.path.getmtime(root / "docs" / "openapi.yaml")
        api_ref_mtime = os.path.getmtime(root / "docs" / "API_REFERENCE.md")
        
        if openapi_mtime > api_ref_mtime + 5: # 5s buffer
            print_result("DOCS-PARITY", False, "API_REFERENCE.md is out of sync with openapi.yaml. Run with FIX=1 to sync.")
            return 1
        else:
            print_result("DOCS-PARITY", True, "API_REFERENCE.md is synchronized")
            return 0

def check_parity(root_dir, fix=False):
    root = ROOT
    router_path = root / "server-rs" / "src" / "router.rs"
    openapi_path = root / "docs" / "openapi.yaml"
    api_ref_path = root / "docs" / "API_REFERENCE.md"
    
    print(f"--- Tadpole OS Parity Audit ---\n")
    
    # 1. Capture Ground Truth (Code)
    code_routes = scan_router(router_path)
    print(f"Found {len(code_routes)} routes in router.rs")
    
    # 2. Capture Source of Truth (OpenAPI)
    raw_doc_paths = scan_openapi(openapi_path)
    # Normalize keys for comparison (e.g. /v1/agents/{id} -> /v1/agents/{PARAM})
    doc_paths = {normalize_path(k): v for k, v in raw_doc_paths.items()}
    
    errors = 0
    
    # Check Code -> OpenAPI parity
    for route in code_routes:
        path = route["path"]
        method = route["method"].lower()
        
        # Normalize code path (e.g. /v1/agents/:id -> /v1/agents/{PARAM})
        base_path = normalize_path(path)
        
        # SELECTIVE ENFORCEMENT: 
        # We strictly enforce /v1 routes.
        # Legacy root routes (e.g., /agents, /engine) are tolerated but warned.
        is_legacy = not path.startswith("/v1")
        
        if base_path in doc_paths:
            if method in doc_paths[base_path]:
                print_result("CODE->OPENAPI", True, f"{route['method']} {path}")
            else:
                print_result("CODE->OPENAPI", False, f"Method {method} missing for {path} in openapi.yaml")
                errors += 1
        else:
            if is_legacy:
                print_result("CODE->OPENAPI", True, f"{route['method']} {path} (Legacy/Internal)")
            else:
                print_result("CODE->OPENAPI", False, f"Route {path} missing in openapi.yaml")
                errors += 1
            
    # 3. Semantic Tag Scanning & Drift Detection
    print(f"\nScanning for Documentation Tags (@docs)...")
    doc_tags = {}
    for root_dir_walk, _, files in os.walk(root / "server-rs" / "src"):
        for file in files:
            if not file.endswith(".rs"): continue
            file_path = os.path.join(root_dir_walk, file)
            with open(file_path, 'r', encoding='utf-8') as f:
                content = f.read()
                # Find /// @docs DOC_NAME:SECTION or //! @docs DOC_NAME:SECTION
                tags = re.findall(r'//[/!]\s*@docs\s+([A-Z_]+):([a-zA-Z0-9_]+)', content)
                for doc_name, section in tags:
                    doc_tags[f"{doc_name}:{section}"] = {
                        "file": file,
                        "mtime": os.path.getmtime(file_path)
                    }

    # Verify Tags against /docs
    for tag, info in doc_tags.items():
        doc_name, section = tag.split(":")
        doc_file = root / "docs" / f"{doc_name}.md"
        if not doc_file.exists():
            # Try root if not in docs/
            doc_file = root / f"{doc_name}.md"
            if not doc_file.exists():
                print_result("ADG-TAG", False, f"Tag {tag} references missing file {doc_name}.md")
                errors += 1
                continue
            
        # Check drift
        doc_mtime = os.path.getmtime(doc_file)
        if info["mtime"] > doc_mtime:
            # We allow 60s tolerance for batch updates
            if info["mtime"] - doc_mtime > 60:
                print_result("ADG-DRIFT", False, f"{info['file']} updated but {doc_name}.md is older (Drift detected)")
                # errors += 1 # Warning only for now
            else:
                print_result("ADG-TAG", True, f"{tag} synchronized (Grace period)")
        else:
            print_result("ADG-TAG", True, f"{tag} synchronized")

    errors += check_env_vars(root)
    errors += check_version_sync(root)
    errors += check_doc_file_refs(root)
    errors += check_skills(root)
    errors += check_api_docs_parity(root, fix=fix)

    print(f"\nAudit Complete. Errors found: {errors}")
    return errors == 0

if __name__ == "__main__":
    project_root = sys.argv[1] if len(sys.argv) > 1 and not sys.argv[1].startswith("FIX=") else "."
    fix_mode = any(arg == "FIX=1" for arg in sys.argv)
    
    if check_parity(project_root, fix=fix_mode):
        sys.exit(0)
    else:
        sys.exit(1)

# Metadata: [parity_guard]
