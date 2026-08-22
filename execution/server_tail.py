"""
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**Core technical resource for the Tadpole OS Sovereign infrastructure.**
Single-shot server health diagnostic that gathers ALL telemetry in one
deterministic execution. Designed to replace manual multi-step API probing.

### 🔍 Debugging & Observability
- **Failure Path**: Server unreachable, auth token mismatch, or missing .env.
- **Telemetry Link**: Search `[server_tail]` in system logs.

### Usage
    # Full diagnostic (default)
    python execution/server_tail.py

    # JSON output for piping
    python execution/server_tail.py --format json

    # Custom host/port
    python execution/server_tail.py --host 127.0.0.1 --port 8000

    # Include last N panic log entries
    python execution/server_tail.py --panic-lines 50

    # Watch mode: re-run every N seconds
    python execution/server_tail.py --watch 30
"""

import argparse
import json
import os
import socket
import subprocess
import sys
import time
import urllib.request
import urllib.error
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional, Tuple

# Fix Windows console encoding (cp1252 can't handle Unicode symbols)
if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass



# ─── Configuration ───────────────────────────────────────────────────────────
# Diagnostic endpoints to probe. Tuple of (path, requires_auth, category).
DIAGNOSTIC_ENDPOINTS: List[Tuple[str, bool, str]] = [
    ("/v1/engine/health",                 False, "core"),
    ("/v1/agents",                        True,  "fleet"),
    ("/v1/oversight/pending",             True,  "governance"),
    ("/v1/oversight/ledger",              True,  "governance"),
    ("/v1/oversight/token-burn",          True,  "budget"),
    ("/v1/oversight/security/health",     True,  "security"),
    ("/v1/oversight/security/quotas",     True,  "security"),
    ("/v1/system/compute-profile",        True,  "infra"),
    ("/v1/infra/nodes",                   True,  "infra"),
    ("/v1/model-manager/providers",       True,  "models"),
    ("/v1/model-manager/models",          True,  "models"),
    ("/v1/governance/manifest",           True,  "governance"),
    ("/v1/skills",                        True,  "capabilities"),
]

# Files to check for diagnostic data
PANIC_LOG_FILENAME = "sidecar_panic.log"
SERVER_LOG_FILENAME = "server.log"


def resolve_workspace_root() -> str:
    """Walk up from the script's directory to find the workspace root (contains .env)."""
    script_dir = os.path.dirname(os.path.abspath(__file__))
    candidate = os.path.abspath(os.path.join(script_dir, ".."))
    if os.path.exists(os.path.join(candidate, ".env")):
        return candidate
    # Fallback: cwd
    if os.path.exists(os.path.join(os.getcwd(), ".env")):
        return os.getcwd()
    return candidate


def load_env_token(workspace_root: str) -> Optional[str]:
    """Extract NEURAL_TOKEN from .env file without external dependencies."""
    env_path = os.path.join(workspace_root, ".env")
    if not os.path.exists(env_path):
        return None
    with open(env_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line.startswith("NEURAL_TOKEN=") and not line.startswith("#"):
                return line.split("=", 1)[1].strip().strip('"').strip("'")
    return None


def load_env_value(workspace_root: str, key: str) -> Optional[str]:
    """Extract any value from .env file."""
    env_path = os.path.join(workspace_root, ".env")
    if not os.path.exists(env_path):
        return None
    with open(env_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line.startswith(f"{key}=") and not line.startswith("#"):
                return line.split("=", 1)[1].strip().strip('"').strip("'")
    return None


def check_port(host: str, port: int, timeout: float = 2.0) -> bool:
    """Quick TCP connect check."""
    try:
        with socket.create_connection((host, port), timeout=timeout):
            return True
    except (OSError, socket.timeout):
        return False


def fetch_endpoint(base_url: str, path: str, token: Optional[str],
                   timeout: float = 10.0) -> Dict[str, Any]:
    """
    Hit a single endpoint and return structured result.
    Returns: { "status": int, "data": dict|list|None, "error": str|None,
               "latency_ms": float, "size_bytes": int }
    """
    url = f"{base_url}{path}"
    headers = {"Accept": "application/json", "User-Agent": "TadpoleOS/1.1.58"}
    if token:
        headers["Authorization"] = f"Bearer {token}"

    start = time.perf_counter()
    try:
        req = urllib.request.Request(url, headers=headers)
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read()
            latency = (time.perf_counter() - start) * 1000
            content_type = resp.headers.get("Content-Type", "")

            # Detect SPA fallback (HTML instead of JSON)
            if "html" in content_type.lower():
                return {
                    "status": resp.status,
                    "data": None,
                    "error": "SPA_FALLBACK: Got HTML instead of JSON (route not matched)",
                    "latency_ms": round(latency, 2),
                    "size_bytes": len(raw),
                    "content_type": content_type,
                }

            try:
                data = json.loads(raw)
            except json.JSONDecodeError:
                data = raw.decode("utf-8", errors="replace")[:500]

            return {
                "status": resp.status,
                "data": data,
                "error": None,
                "latency_ms": round(latency, 2),
                "size_bytes": len(raw),
                "content_type": content_type,
            }
    except urllib.error.HTTPError as e:
        latency = (time.perf_counter() - start) * 1000
        body = ""
        try:
            body = e.read().decode("utf-8", errors="replace")[:300]
        except Exception:
            pass
        return {
            "status": e.code,
            "data": None,
            "error": f"HTTP {e.code}: {e.reason}",
            "latency_ms": round(latency, 2),
            "size_bytes": len(body),
            "content_type": "",
            "response_body": body,
        }
    except Exception as e:
        latency = (time.perf_counter() - start) * 1000
        return {
            "status": 0,
            "data": None,
            "error": str(e),
            "latency_ms": round(latency, 2),
            "size_bytes": 0,
            "content_type": "",
        }


def read_panic_log(workspace_root: str, max_lines: int = 50) -> Dict[str, Any]:
    """Parse sidecar_panic.log for structured panic entries."""
    path = os.path.join(workspace_root, PANIC_LOG_FILENAME)
    if not os.path.exists(path):
        return {"exists": False, "path": path, "panics": [], "count": 0}

    stat = os.stat(path)
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        content = f.read()

    # Parse individual panic blocks
    panics = []
    blocks = content.split("--- PANIC DETECTED ---")
    for block in blocks[1:]:  # Skip empty first split
        lines = block.strip().split("\n")
        panic_entry: Dict[str, str] = {}
        for line in lines:
            line = line.strip()
            if line.startswith("Message:"):
                panic_entry["message"] = line[8:].strip()
            elif line.startswith("Location:"):
                panic_entry["location"] = line[9:].strip()
        if panic_entry:
            panics.append(panic_entry)

    # Deduplicate by message to find unique panic types
    unique_messages: Dict[str, int] = {}
    import re
    for p in panics:
        msg = p.get("message", "unknown")
        # Normalize span IDs out for dedup (e.g., Id(12345) -> Id(X))
        normalized = re.sub(r'Id\(\d+\)', 'Id(X)', msg)
        unique_messages[normalized] = unique_messages.get(normalized, 0) + 1

    return {
        "exists": True,
        "path": path,
        "file_size_bytes": stat.st_size,
        "last_modified": datetime.fromtimestamp(stat.st_mtime, tz=timezone.utc).isoformat(),
        "total_panics": len(panics),
        "unique_panic_types": len(unique_messages),
        "panic_summary": [{"message": k, "count": v} for k, v in unique_messages.items()],
        "raw_panics": panics[-max_lines:] if len(panics) > max_lines else panics,
    }


def get_process_info(port: int) -> Dict[str, Any]:
    """Get server process details via OS commands (Windows/Linux)."""
    info: Dict[str, Any] = {"found": False}
    try:
        if sys.platform == "win32":
            # Use netstat to find PID
            result = subprocess.run(
                ["netstat", "-ano"],
                capture_output=True, text=True, timeout=5
            )
            pid = None
            for line in result.stdout.split("\n"):
                if f":{port}" in line and "LISTENING" in line:
                    parts = line.strip().split()
                    if parts:
                        pid = int(parts[-1])
                        break

            if pid:
                # Get process details via tasklist (no /V to avoid slow full-process scan)
                result = subprocess.run(
                    ["tasklist", "/FI", f"PID eq {pid}", "/FO", "CSV"],
                    capture_output=True, text=True, timeout=10
                )
                info["found"] = True
                info["pid"] = pid
                # Parse CSV output: "Image Name","PID","Session Name","Session#","Mem Usage"
                lines = result.stdout.strip().split("\n")
                if len(lines) >= 2:
                    parts = lines[1].replace('"', '').split(',')
                    if len(parts) >= 5:
                        info["process_name"] = parts[0]
                        info["memory_usage"] = parts[4].strip()
        else:
            # Linux/macOS: use lsof or ss
            result = subprocess.run(
                ["lsof", "-i", f":{port}", "-t"],
                capture_output=True, text=True, timeout=5
            )
            if result.stdout.strip():
                pid = int(result.stdout.strip().split("\n")[0])
                info["found"] = True
                info["pid"] = pid
    except Exception as e:
        info["error"] = str(e)

    return info


def analyze_agent_fleet(agents_data: Any) -> Dict[str, Any]:
    """Analyze agent fleet health from /v1/agents response."""
    if not agents_data or not isinstance(agents_data, dict):
        return {"error": "No agent data available"}

    agents = agents_data.get("agents", agents_data.get("data", []))
    if isinstance(agents_data, list):
        agents = agents_data

    status_counts: Dict[str, int] = {}
    unhealthy: List[str] = []
    bankrupt: List[str] = []
    throttled: List[str] = []

    for agent in agents:
        status = agent.get("status", "unknown")
        status_counts[status] = status_counts.get(status, 0) + 1
        name = agent.get("name", agent.get("agent_id", "?"))
        if not agent.get("is_healthy", True):
            unhealthy.append(name)
        if agent.get("is_bankrupt", False):
            bankrupt.append(name)
        if agent.get("is_throttled", False):
            throttled.append(name)

    return {
        "total": len(agents),
        "status_distribution": status_counts,
        "unhealthy_agents": unhealthy,
        "bankrupt_agents": bankrupt,
        "throttled_agents": throttled,
        "fleet_health": "HEALTHY" if not unhealthy and not bankrupt else "DEGRADED",
    }


def analyze_ledger(ledger_data: Any) -> Dict[str, Any]:
    """Summarize oversight ledger for failure patterns."""
    if not ledger_data:
        return {"total_entries": 0}

    entries = ledger_data.get("data", []) if isinstance(ledger_data, dict) else []
    total = len(entries)
    failures = [e for e in entries if e.get("result", {}).get("success") is False]
    unique_errors: Dict[str, int] = {}
    for f in failures:
        err = f.get("result", {}).get("error", "unknown")
        # Truncate long errors
        short = err[:100] if len(err) > 100 else err
        unique_errors[short] = unique_errors.get(short, 0) + 1

    return {
        "total_entries": total,
        "total_failures": len(failures),
        "failure_rate": f"{(len(failures) / total * 100):.1f}%" if total > 0 else "N/A",
        "unique_error_patterns": unique_errors,
    }


def check_log_freshness(workspace_root: str) -> Dict[str, Any]:
    """Check if server.log exists and how fresh it is."""
    log_path = os.path.join(workspace_root, SERVER_LOG_FILENAME)
    if not os.path.exists(log_path):
        return {"exists": False, "path": log_path}

    stat = os.stat(log_path)
    mtime = datetime.fromtimestamp(stat.st_mtime, tz=timezone.utc)
    age_hours = (datetime.now(timezone.utc) - mtime).total_seconds() / 3600

    return {
        "exists": True,
        "path": log_path,
        "size_bytes": stat.st_size,
        "size_mb": round(stat.st_size / (1024 * 1024), 2),
        "last_modified": mtime.isoformat(),
        "age_hours": round(age_hours, 1),
        "is_stale": age_hours > 1.0,  # Stale if older than 1 hour
    }


def run_diagnostic(host: str, port: int, workspace_root: str,
                   panic_lines: int = 50, output_format: str = "text") -> Dict[str, Any]:
    """
    Master diagnostic function. Gathers ALL telemetry in a single deterministic pass.

    Returns a structured report dict with sections:
    - meta: timestamp, host, port, workspace
    - connectivity: port check, process info
    - endpoints: per-endpoint results grouped by category
    - panics: parsed panic log
    - logs: server log freshness
    - fleet: agent fleet analysis
    - ledger: oversight ledger analysis
    - verdicts: summary of good/bad/needs-improvement
    """
    print("\n🔬 TadpoleOS Server Diagnostic starting...\n")
    report: Dict[str, Any] = {
        "meta": {
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "host": host,
            "port": port,
            "workspace_root": workspace_root,
            "diagnostic_version": "1.0.0",
        }
    }

    # ─── Phase 1: Connectivity ────────────────────────────────────────────
    print("Phase 1: Checking connectivity...", flush=True)
    is_up = check_port(host, port)
    print(f"  Server {'reachable' if is_up else 'UNREACHABLE'} on {host}:{port}")
    print("Phase 1b: Getting process info...", flush=True)
    process_info = get_process_info(port) if is_up else {"found": False}
    print(f"  Process: {'found' if process_info.get('found') else 'not found'}")
    report["connectivity"] = {
        "server_reachable": is_up,
        "process": process_info,
    }

    if not is_up:
        report["verdicts"] = {
            "status": "OFFLINE",
            "good": [],
            "bad": ["Server is not reachable on {host}:{port}"],
            "needs_improvement": [],
        }
        return report

    # ─── Phase 2: Auth Token ──────────────────────────────────────────────
    token = load_env_token(workspace_root)
    report["auth"] = {
        "token_found": token is not None,
        "token_source": os.path.join(workspace_root, ".env"),
    }

    # ─── Phase 3: Endpoint Sweep ──────────────────────────────────────────
    print(f"\nPhase 3: Sweeping {len(DIAGNOSTIC_ENDPOINTS)} endpoints...", flush=True)
    base_url = f"http://{host}:{port}"
    endpoints: Dict[str, Dict[str, Any]] = {}
    endpoint_data: Dict[str, Any] = {}  # Store raw data for analysis

    for i, (path, requires_auth, category) in enumerate(DIAGNOSTIC_ENDPOINTS, 1):
        print(f"  [{i}/{len(DIAGNOSTIC_ENDPOINTS)}] {path}...", end="", flush=True)
        auth_token = token if requires_auth else None
        result = fetch_endpoint(base_url, path, auth_token, timeout=5.0)
        ok = result["status"] == 200 and result.get("error") is None
        print(f" {'OK' if ok else 'FAIL'} ({result['latency_ms']:.0f}ms)")
        endpoints[path] = {
            "category": category,
            "requires_auth": requires_auth,
            "status": result["status"],
            "latency_ms": result["latency_ms"],
            "size_bytes": result["size_bytes"],
            "error": result.get("error"),
            "ok": ok,
        }
        # Store data for cross-analysis (only if successful)
        if result["status"] == 200 and result.get("data") is not None:
            endpoint_data[path] = result["data"]

    report["endpoints"] = endpoints

    # ─── Phase 4: Panic Log ───────────────────────────────────────────────
    print("\nPhase 4: Reading panic log...", flush=True)
    report["panics"] = read_panic_log(workspace_root, panic_lines)

    # ─── Phase 5: Server Log Freshness ────────────────────────────────────
    print("Phase 5-10: Analyzing fleet, ledger, providers, compute...", flush=True)
    report["logs"] = check_log_freshness(workspace_root)

    # ─── Phase 6: Fleet Analysis ──────────────────────────────────────────
    agents_data = endpoint_data.get("/v1/agents")
    report["fleet"] = analyze_agent_fleet(agents_data) if agents_data else {"error": "No agent data"}

    # ─── Phase 7: Ledger Analysis ─────────────────────────────────────────
    ledger_data = endpoint_data.get("/v1/oversight/ledger")
    report["ledger"] = analyze_ledger(ledger_data) if ledger_data else {"error": "No ledger data"}

    # ─── Phase 8: Provider Analysis ───────────────────────────────────────
    providers_data = endpoint_data.get("/v1/model-manager/providers", [])
    if isinstance(providers_data, list):
        report["providers"] = {
            "total": len(providers_data),
            "with_api_key": sum(1 for p in providers_data if p.get("has_api_key")),
            "without_api_key": sum(1 for p in providers_data if not p.get("has_api_key")),
            "details": [
                {"id": p.get("id"), "name": p.get("name"),
                 "has_key": p.get("has_api_key", False), "protocol": p.get("protocol")}
                for p in providers_data
            ],
        }

    # ─── Phase 9: Compute Verdict ─────────────────────────────────────────
    compute = endpoint_data.get("/v1/system/compute-profile", {})
    if compute:
        cpu = compute.get("cpu_usage", 0)
        mem_used = compute.get("memory_used", 0)
        mem_total = compute.get("memory_total", 1)
        mem_pct = (mem_used / mem_total * 100) if mem_total > 0 else 0
        report["compute"] = {
            "cpu_percent": round(cpu, 1),
            "memory_percent": round(mem_pct, 1),
            "memory_used_gb": round(mem_used / (1024**3), 2),
            "memory_total_gb": round(mem_total / (1024**3), 2),
            "active_processes": compute.get("active_processes", 0),
            "cpu_status": "CRITICAL" if cpu > 90 else "WARNING" if cpu > 70 else "OK",
            "memory_status": "CRITICAL" if mem_pct > 90 else "WARNING" if mem_pct > 70 else "OK",
        }

    # ─── Phase 10: Build Verdicts ─────────────────────────────────────────
    good: List[str] = []
    bad: List[str] = []
    needs_improvement: List[str] = []

    # Endpoint health
    ok_count = sum(1 for v in endpoints.values() if v["ok"])
    fail_count = sum(1 for v in endpoints.values() if not v["ok"])
    if ok_count == len(endpoints):
        good.append(f"All {ok_count} API endpoints responding correctly")
    elif fail_count > 0:
        bad.append(f"{fail_count}/{len(endpoints)} endpoints failed")

    # Health data analysis
    health_data = endpoint_data.get("/v1/engine/health", {})
    if health_data:
        db_status = health_data.get("database", {}).get("status")
        if db_status == "healthy":
            good.append("Database pool healthy")
        else:
            bad.append(f"Database status: {db_status}")

        budget = health_data.get("budget", {})
        pct = budget.get("percent_used", 0)
        if pct < 50:
            good.append(f"Budget utilization low ({pct:.1f}%)")
        elif pct > 80:
            bad.append(f"Budget utilization critical ({pct:.1f}%)")
        else:
            needs_improvement.append(f"Budget at {pct:.1f}% — monitor closely")

        swarm = health_data.get("swarm", {})
        if swarm.get("status") == "healthy":
            good.append("Swarm framework healthy")

        version = health_data.get("version", "?")
        good.append(f"Engine v{version} running")

    # Fleet analysis
    fleet = report.get("fleet", {})
    if fleet.get("fleet_health") == "HEALTHY":
        good.append(f"All {fleet.get('total', 0)} agents healthy")
    if fleet.get("unhealthy_agents"):
        bad.append(f"Unhealthy agents: {fleet['unhealthy_agents']}")
    status_dist = fleet.get("status_distribution", {})
    offline = status_dist.get("offline", 0)
    total = fleet.get("total", 0)
    if total > 0 and offline / total > 0.8:
        needs_improvement.append(f"{offline}/{total} agents offline — fleet underutilized")

    # Panic analysis
    panics = report.get("panics", {})
    if panics.get("total_panics", 0) > 0:
        bad.append(f"{panics['total_panics']} panics logged ({panics.get('unique_panic_types', 0)} unique types)")
    else:
        good.append("No panics detected")

    # Log freshness
    logs = report.get("logs", {})
    if logs.get("is_stale"):
        needs_improvement.append(f"Server log is {logs.get('age_hours', '?')}h old — no active file logging")

    # Ledger failures
    ledger = report.get("ledger", {})
    if ledger.get("total_failures", 0) > 0:
        bad.append(f"Ledger shows {ledger['total_failures']} tool execution failures")
        for err, count in ledger.get("unique_error_patterns", {}).items():
            bad.append(f"  → {err} (x{count})")

    # Compute
    compute_info = report.get("compute", {})
    if compute_info.get("cpu_status") == "CRITICAL":
        bad.append(f"CPU at {compute_info.get('cpu_percent', '?')}% — host under heavy load")
    elif compute_info.get("cpu_status") == "WARNING":
        needs_improvement.append(f"CPU at {compute_info.get('cpu_percent', '?')}%")

    # Providers
    prov = report.get("providers", {})
    if prov.get("without_api_key", 0) > 0:
        needs_improvement.append(f"{prov['without_api_key']} providers missing API keys")

    report["verdicts"] = {
        "status": "CRITICAL" if bad else "HEALTHY" if not needs_improvement else "NEEDS_ATTENTION",
        "good": good,
        "bad": bad,
        "needs_improvement": needs_improvement,
        "score": f"{len(good)}/{len(good) + len(bad) + len(needs_improvement)}",
    }

    return report


def format_text_report(report: Dict[str, Any]) -> str:
    """Format the diagnostic report as human-readable text."""
    lines: List[str] = []
    meta = report.get("meta", {})

    lines.append("=" * 72)
    lines.append("  TADPOLE OS SERVER DIAGNOSTIC REPORT")
    lines.append(f"  Timestamp : {meta.get('timestamp', '?')}")
    lines.append(f"  Target    : {meta.get('host', '?')}:{meta.get('port', '?')}")
    lines.append(f"  Workspace : {meta.get('workspace_root', '?')}")
    lines.append("=" * 72)

    # Verdicts (top of report for quick scanning)
    verdicts = report.get("verdicts", {})
    status = verdicts.get("status", "UNKNOWN")
    status_icon = {"HEALTHY": "🟢", "NEEDS_ATTENTION": "🟡", "CRITICAL": "🔴", "OFFLINE": "⚫"}.get(status, "❓")
    lines.append(f"\n  STATUS: {status_icon} {status}  ({verdicts.get('score', '?')})")

    for item in verdicts.get("good", []):
        lines.append(f"    ✅ {item}")
    for item in verdicts.get("bad", []):
        lines.append(f"    ❌ {item}")
    for item in verdicts.get("needs_improvement", []):
        lines.append(f"    ⚠️  {item}")

    # Connectivity
    conn = report.get("connectivity", {})
    lines.append(f"\n── Connectivity ────────────────────────────────────────")
    lines.append(f"  Server reachable: {conn.get('server_reachable', False)}")
    proc = conn.get("process", {})
    if proc.get("found"):
        lines.append(f"  PID: {proc.get('pid', '?')} | Process: {proc.get('process_name', '?')} | Mem: {proc.get('memory_usage', '?')}")

    # Endpoints
    endpoints = report.get("endpoints", {})
    if endpoints:
        lines.append(f"\n── Endpoint Sweep ({len(endpoints)} endpoints) ──────────────────────")
        for path, info in endpoints.items():
            icon = "✅" if info.get("ok") else "❌"
            latency = info.get("latency_ms", 0)
            size = info.get("size_bytes", 0)
            err = f" | {info.get('error')}" if info.get("error") else ""
            lines.append(f"  {icon} {info.get('status', '?')} | {latency:6.1f}ms | {size:>7}b | {path}{err}")

    # Compute
    compute = report.get("compute", {})
    if compute:
        lines.append(f"\n── Compute Profile ─────────────────────────────────────")
        lines.append(f"  CPU: {compute.get('cpu_percent', '?')}% ({compute.get('cpu_status', '?')})")
        lines.append(f"  Memory: {compute.get('memory_used_gb', '?')} / {compute.get('memory_total_gb', '?')} GB ({compute.get('memory_percent', '?')}%)")
        lines.append(f"  Processes: {compute.get('active_processes', '?')}")

    # Fleet
    fleet = report.get("fleet", {})
    if fleet and not fleet.get("error"):
        lines.append(f"\n── Agent Fleet ({fleet.get('total', 0)} agents) ─────────────────────────")
        lines.append(f"  Health: {fleet.get('fleet_health', '?')}")
        for status_name, count in fleet.get("status_distribution", {}).items():
            lines.append(f"    {status_name}: {count}")

    # Panics
    panics = report.get("panics", {})
    if panics.get("exists"):
        lines.append(f"\n── Panic Log ───────────────────────────────────────────")
        lines.append(f"  Total panics: {panics.get('total_panics', 0)} ({panics.get('unique_panic_types', 0)} unique)")
        lines.append(f"  Last modified: {panics.get('last_modified', '?')}")
        for p in panics.get("panic_summary", []):
            lines.append(f"    [{p.get('count', 1)}x] {p.get('message', '?')[:80]}")

    # Ledger
    ledger = report.get("ledger", {})
    if ledger and not ledger.get("error"):
        lines.append(f"\n── Oversight Ledger ────────────────────────────────────")
        lines.append(f"  Total entries: {ledger.get('total_entries', 0)}")
        lines.append(f"  Failures: {ledger.get('total_failures', 0)} ({ledger.get('failure_rate', 'N/A')})")

    lines.append("\n" + "=" * 72)
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(
        description="TadpoleOS Server Health Diagnostic — Single-shot telemetry gathering",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python server_tail.py                    # Full text diagnostic
  python server_tail.py --format json      # JSON for piping to other tools
  python server_tail.py --watch 30         # Re-run every 30 seconds
  python server_tail.py --port 9000        # Custom port
        """,
    )
    parser.add_argument("--host", default="127.0.0.1", help="Server host (default: 127.0.0.1)")
    parser.add_argument("--port", type=int, default=None, help="Server port (auto-detected from .env if not set)")
    parser.add_argument("--format", choices=["text", "json"], default="text", help="Output format")
    parser.add_argument("--panic-lines", type=int, default=50, help="Max panic entries to include")
    parser.add_argument("--watch", type=int, default=0, help="Re-run interval in seconds (0 = once)")
    parser.add_argument("--output", type=str, help="Write report to file")
    parser.add_argument("--workspace", type=str, default=None, help="Workspace root (auto-detected)")
    args = parser.parse_args()

    workspace_root = args.workspace or resolve_workspace_root()

    # Auto-detect port from .env if not specified
    port = args.port
    if port is None:
        env_port = load_env_value(workspace_root, "PORT")
        port = int(env_port) if env_port else 8000

    def run_once():
        report = run_diagnostic(
            host=args.host,
            port=port,
            workspace_root=workspace_root,
            panic_lines=args.panic_lines,
        )

        if args.format == "json":
            output = json.dumps(report, indent=2, default=str)
        else:
            output = format_text_report(report)

        if args.output:
            with open(args.output, "w", encoding="utf-8") as f:
                f.write(output)
            print(f"[OK] Report written to {args.output}")
        else:
            print(output)

        return report

    if args.watch > 0:
        print(f"[WATCH MODE] Re-running every {args.watch}s. Press Ctrl+C to stop.\n")
        try:
            while True:
                run_once()
                time.sleep(args.watch)
                if args.format == "text":
                    print("\n" + "─" * 72 + "\n")
        except KeyboardInterrupt:
            print("\n[WATCH] Stopped.")
    else:
        report = run_once()
        # Exit with non-zero if critical
        if report.get("verdicts", {}).get("status") == "CRITICAL":
            sys.exit(1)


if __name__ == "__main__":
    main()


# Metadata: [server_tail]
