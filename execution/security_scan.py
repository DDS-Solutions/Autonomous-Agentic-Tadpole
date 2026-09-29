"""
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**Core technical resource for the Tadpole OS Sovereign infrastructure.**
Advanced vulnerability and secret scanning engine enforcing OWASP Top 10 security standards.

### 🔍 Debugging & Observability
- **Failure Path**: Security vulnerability, leaked secret, or configuration flaw detected.
- **Telemetry Link**: Search `[security_scan]` in system logs.
"""

import sys
import os
import re
import io
import json
import logging
import argparse
from pathlib import Path
from typing import Dict, List, Any, Optional

# Ensure stdout/stderr handle UTF-8 on Windows
if sys.platform == "win32":
    if hasattr(sys.stdout, "buffer"):
        sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
    if hasattr(sys.stderr, "buffer"):
        sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8')

# Initialize sovereign logger
logging.basicConfig(level=logging.INFO, format="%(message)s")
logger = logging.getLogger("security_scan")

# ============================================================================
#  DETECTION PATTERNS
# ============================================================================

SECRET_PATTERNS = [
    # LLM / AI Providers
    (re.compile(r"sk-(?:ant-)?[a-zA-Z0-9_-]{20,}"), "OpenAI/Anthropic API Key", "critical"),
    (re.compile(r"AIza[0-9A-Za-z_-]{35}"), "Google Cloud / Gemini API Key", "critical"),
    (re.compile(r"gsk_[a-zA-Z0-9]{20,}"), "Groq API Key", "critical"),
    (re.compile(r"hf_[a-zA-Z0-9]{20,}"), "HuggingFace Access Token", "critical"),
    
    # Source Control & Cloud
    (re.compile(r"ghp_[a-zA-Z0-9]{36}"), "GitHub Personal Access Token", "critical"),
    (re.compile(r"github_pat_[a-zA-Z0-9_]{22,}"), "GitHub Fine-Grained Token", "critical"),
    (re.compile(r"AKIA[0-9A-Z]{16}"), "AWS Access Key ID", "critical"),
    (re.compile(r"aws[_-]?secret[_-]?access[_-]?key\s*[=:]\s*[\"']?[A-Za-z0-9/+=]{40}[\"']?", re.IGNORECASE), "AWS Secret Key", "critical"),
    
    # Bearer Tokens (requires 25+ chars or header syntax to prevent matching prose)
    (re.compile(r"(?i)\bAuthorization\s*:\s*Bearer\s+[a-zA-Z0-9_\-\.]{20,}"), "Bearer Token Header", "critical"),
    (re.compile(r"(?i)\bbearer\s+[a-zA-Z0-9_\-\.]{25,}"), "Raw Bearer Token", "high"),
    
    # Cryptographic Material
    (re.compile(r"-----BEGIN\s+(?:RSA|PRIVATE|EC|OPENSSH|DSA|ENCRYPTED)\s+KEY-----"), "Private Key Block", "critical"),
    (re.compile(r"ssh-rsa\s+[A-Za-z0-9+/]{40,}"), "SSH Public/Private Key Material", "high"),
    
    # Database Connections
    (re.compile(r"(?:mongodb|postgres|mysql|redis):\/\/[^\s\"'\`]+"), "Database Connection String", "critical"),
    
    # JWT
    (re.compile(r"eyJ[A-Za-z0-9-_]{10,}\.eyJ[A-Za-z0-9-_]{10,}\.[A-Za-z0-9-_]{10,}"), "JWT Token", "high"),
]

ENV_ASSIGNMENT_PATTERN = re.compile(
    r'(?im)^[ \t]*(?:export[ \t]+)?([A-Za-z0-9_]*(?:API[_-]?KEY|TOKEN|SECRET|PASSWORD)[A-Za-z0-9_]*)[ \t]*=[ \t]*(?![$<{\s\?])["\']?([^"\'\s#()${}<>\r\n]{10,})["\']?[ \t]*$'
)

DANGEROUS_PATTERNS = [
    # Code Injection
    (re.compile(r'\beval\s*\('), "eval() usage", "critical", "Code Injection risk"),
    (re.compile(r'(?<![\w\.\$])exec\s*\('), "exec() usage", "critical", "Code Injection risk"),
    (re.compile(r'\bnew\s+Function\s*\('), "Function constructor", "high", "Code Injection risk"),
    (re.compile(r'\bchild_process\.exec\s*\('), "child_process.exec", "high", "Command Injection risk"),
    (re.compile(r'\bsubprocess\.call\s*\([^)]*shell\s*=\s*True'), "subprocess with shell=True", "high", "Command Injection risk"),
    
    # XSS Risks
    (re.compile(r'\bdangerouslySetInnerHTML'), "dangerouslySetInnerHTML", "high", "XSS risk"),
    (re.compile(r'\.innerHTML\s*='), "innerHTML assignment", "medium", "XSS risk"),
    (re.compile(r'\bdocument\.write\s*\('), "document.write", "medium", "XSS risk"),
    
    # SQL Injection
    (re.compile(r'["\'][^"\']*\+\s*[a-zA-Z_]+\s*\+\s*["\'].*(?-i:SELECT|INSERT|UPDATE|DELETE)'), "SQL String Concat", "critical", "SQL Injection risk"),
    (re.compile(r'f"[^"]*(?-i:SELECT|INSERT|UPDATE|DELETE|DROP\s+TABLE|UNION\s+SELECT)[^"]*\{'), "SQL f-string", "critical", "SQL Injection risk"),
    
    # Insecure Configuration & Deserialization
    (re.compile(r'\bverify\s*=\s*False\b'), "SSL Verify Disabled", "high", "MITM risk"),
    (re.compile(r'\bpickle\.loads?\s*\('), "pickle usage", "high", "Deserialization risk"),
    (re.compile(r'\byaml\.load\s*\([^)]*\)(?!\s*,\s*Loader)'), "Unsafe YAML load", "high", "Deserialization risk"),
]

PLACEHOLDER_SUBSTRINGS = {
    'dummy', 'test', 'placeholder', 'example', 'your_', 'sample', 'fake',
    'invalid', 'wrong', 'current', 'initial', 'change_me', 'secret',
    'default', '<password>', '<token>', 'mock', 'none'
}

SKIP_DIRS = {
    'node_modules', '.git', 'dist', 'build', '__pycache__', '.venv', 'venv',
    '.next', '.tmp', 'target', 'coverage', 'htmlcov', 'scratch'
}

CODE_EXTENSIONS = {'.js', '.ts', '.jsx', '.tsx', '.py', '.rs', '.go', '.java'}
CONFIG_EXTENSIONS = {'.json', '.yaml', '.yml', '.toml', '.env'}

def is_test_path(path: Path) -> bool:
    """Accurately identify test files without false positives on words like 'latest.rs'."""
    # Check directory parts
    if any(part in {'tests', 'test', '__tests__', 'fixtures'} for part in path.parts):
        return True
    # Check specific filename conventions
    name = path.name.lower()
    if name.endswith(('_test.py', '.test.ts', '.test.tsx', '.test.js', '.spec.ts', '.spec.js')):
        return True
    if name.startswith(('test_', 'spec_')):
        return True
    return False

def run_security_scan(root_path: str, scan_type: str = "all", scan_env: bool = False) -> Dict[str, Any]:
    """Execute complete sovereign security scan across target path."""
    logger.info(f"🛡️ [security_scan] Running Sovereign Security Scan for: {root_path}")
    
    root = Path(root_path).resolve()
    findings: List[Dict[str, Any]] = []
    scanned_count = 0
    
    self_path = Path(__file__).resolve()
    
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        
        for f in filenames:
            p = (Path(dirpath) / f).resolve()
            
            # Skip self-scanning scanner files
            if p == self_path or p.name == 'security_scan.py':
                continue
                
            # Skip test suites to avoid false alarms on mock credentials
            if is_test_path(p):
                continue

            # In the 3-layer architecture, untracked local .env files store local developer credentials.
            # Only scan local .env if explicitly requested with scan_env=True.
            if p.name == '.env' and not scan_env:
                continue

            ext = p.suffix.lower()
            is_env_file = p.name == '.env' or p.name.startswith('.env.')
            
            if not (ext in CODE_EXTENSIONS or ext in CONFIG_EXTENSIONS or is_env_file):
                continue
                
            scanned_count += 1
            
            try:
                content = p.read_text(encoding='utf-8', errors='ignore')
            except Exception as e:
                logger.warning(f"⚠️ [security_scan] Unable to read {p}: {e}")
                continue
                
            # Respect explicit file-level suppression
            if "@security-scan:file-ignore" in content:
                continue

            lines = content.splitlines()
            cfg_test_idx = content.find("#[cfg(test)]") if ext == '.rs' else -1

            # 1. Secret Scanning
            if scan_type in {"all", "secrets"}:
                for pat, name, sev in SECRET_PATTERNS:
                    for match in pat.finditer(content):
                        if cfg_test_idx != -1 and match.start() >= cfg_test_idx:
                            continue
                            
                        matched_text = match.group(0)
                        if any(ph in matched_text.lower() for ph in PLACEHOLDER_SUBSTRINGS):
                            continue
                            
                        # Find line number and verify line doesn't have nosec
                        line_num = content[:match.start()].count('\n') + 1
                        line_content = lines[line_num - 1] if line_num <= len(lines) else ""
                        if "nosec" in line_content.lower():
                            continue
                            
                        findings.append({
                            "file": str(p.relative_to(root) if p.is_relative_to(root) else p),
                            "line": line_num,
                            "type": name,
                            "severity": sev,
                            "snippet": f"{matched_text[:8]}...{matched_text[-4:]}" if len(matched_text) > 12 else "***"
                        })

                # Configuration & .env unquoted secrets
                if is_env_file or ext in {'.yaml', '.yml', '.toml'}:
                    for match in ENV_ASSIGNMENT_PATTERN.finditer(content):
                        var_name = match.group(1)
                        val = match.group(2)
                        if any(ph in val.lower() for ph in PLACEHOLDER_SUBSTRINGS):
                            continue
                            
                        line_num = content[:match.start()].count('\n') + 1
                        line_content = lines[line_num - 1] if line_num <= len(lines) else ""
                        if "nosec" in line_content.lower():
                            continue
                            
                        findings.append({
                            "file": str(p.relative_to(root) if p.is_relative_to(root) else p),
                            "line": line_num,
                            "type": f"Unquoted Secret ({var_name})",
                            "severity": "high",
                            "snippet": f"{val[:6]}...{val[-4:]}" if len(val) > 10 else "***"
                        })

            # 2. Code Pattern Scanning (OWASP)
            if scan_type in {"all", "patterns"} and ext in CODE_EXTENSIONS:
                for pat, name, sev, category in DANGEROUS_PATTERNS:
                    for match in pat.finditer(content):
                        if cfg_test_idx != -1 and match.start() >= cfg_test_idx:
                            continue
                            
                        line_num = content[:match.start()].count('\n') + 1
                        line_content = lines[line_num - 1] if line_num <= len(lines) else ""
                        if "nosec" in line_content.lower():
                            continue
                            
                        findings.append({
                            "file": str(p.relative_to(root) if p.is_relative_to(root) else p),
                            "line": line_num,
                            "type": f"{name} ({category})",
                            "severity": sev,
                            "snippet": match.group(0)[:30]
                        })

    critical_count = sum(1 for f in findings if f["severity"] == "critical")
    high_count = sum(1 for f in findings if f["severity"] == "high")
    medium_count = sum(1 for f in findings if f["severity"] == "medium")
    
    status = "PASS" if (critical_count == 0 and high_count == 0) else "FAIL"
    
    return {
        "status": status,
        "scanned_files": scanned_count,
        "total_findings": len(findings),
        "critical": critical_count,
        "high": high_count,
        "medium": medium_count,
        "findings": findings
    }

def main():
    parser = argparse.ArgumentParser(description="Tadpole OS Sovereign Security Scan")
    parser.add_argument("path", nargs="?", default=".", help="Root path to scan")
    parser.add_argument("--project-path", "-p", dest="project_path", default=None, help="Root path to scan")
    parser.add_argument("--scan-type", "-t", dest="scan_type", default="all", choices=["all", "secrets", "patterns"])
    parser.add_argument("--scan-env", action="store_true", help="Force scanning of untracked local .env files")
    parser.add_argument("--output", "-o", choices=["summary", "json"], default="summary", help="Output format")
    args = parser.parse_args()
    
    target_root = args.project_path or args.path
    result = run_security_scan(target_root, scan_type=args.scan_type, scan_env=args.scan_env)
    
    if args.output == "json":
        print(json.dumps(result, indent=2))
    else:
        print(f"\n{'='*60}")
        print(f"[SECURITY SCAN] Audit Report: {target_root}")
        print(f"{'='*60}")
        print(f"Status: {result['status']}")
        print(f"Files Scanned: {result['scanned_files']}")
        print(f"Total Findings: {result['total_findings']}")
        print(f"  - Critical: {result['critical']}")
        print(f"  - High:     {result['high']}")
        print(f"  - Medium:   {result['medium']}")
        print(f"{'='*60}")
        
        if result['findings']:
            print("\nFindings Detail:")
            for item in result['findings'][:20]:
                print(f"  [{item['severity'].upper()}] {item['file']}:{item['line']} - {item['type']} ({item['snippet']})")
            if len(result['findings']) > 20:
                print(f"  ... and {len(result['findings']) - 20} more findings.")
        else:
            print("✅ No critical security vulnerabilities or credential leaks detected.")

    sys.exit(0 if result["status"] == "PASS" else 1)

if __name__ == "__main__":
    main()

# Metadata: [security_scan]
