> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[server_tailing]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
> Single-shot server health diagnostic and endpoint sweep.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

# Server Tailing & Health Diagnostic


## Purpose

When asked to "tail the server", "check server health", "diagnose localhost", or
"what's the server doing", use `execution/server_tail.py` instead of manually
probing endpoints one-by-one.

## Why This Exists

Manual server diagnostics previously required **8+ sequential tool calls**:
1. `netstat` to find the PID
2. `Get-Process` to get memory/CPU
3. Recursive file search for log files
4. Reading `.env` to extract `NEURAL_TOKEN`
5. Guessing endpoint paths (`/api/*` vs `/v1/*`)
6. Hitting 13+ endpoints individually with auth headers
7. Reading `sidecar_panic.log` separately
8. Cross-referencing and summarizing all JSON responses

This is error-prone, slow (~3 minutes), and wastes context tokens.
The `server_tail.py` script does **all of this in a single execution** (~2 seconds).

## Usage

### Standard Diagnostic (Text Report)
```bash
python execution/server_tail.py
```

### JSON Output (For Piping or Parsing)
```bash
python execution/server_tail.py --format json
```

### Watch Mode (Continuous Monitoring)
```bash
python execution/server_tail.py --watch 30
```

### Custom Target
```bash
python execution/server_tail.py --host 192.168.1.100 --port 9000
```

### Save Report to File
```bash
python execution/server_tail.py --output .tmp/health_report.txt
```

## What It Gathers (10 Phases)

| Phase | What | Source |
|-------|------|--------|
| 1 | Port connectivity + PID | `netstat` / `lsof` |
| 2 | Auth token | `.env` (auto-parsed) |
| 3 | 13 API endpoints | HTTP sweep with auth |
| 4 | Panic log | `sidecar_panic.log` (parsed + deduped) |
| 5 | Server log freshness | `server.log` stat check |
| 6 | Agent fleet analysis | `/v1/agents` cross-analysis |
| 7 | Oversight ledger | `/v1/oversight/ledger` failure patterns |
| 8 | Provider status | `/v1/model-manager/providers` key check |
| 9 | Compute profile | `/v1/system/compute-profile` CPU/RAM |
| 10 | Verdict synthesis | Good / Bad / Needs Improvement |

## Output Format

The report includes structured verdicts:
- **✅ Good**: Things working correctly
- **❌ Bad**: Critical issues requiring attention
- **⚠️ Needs Improvement**: Moderate issues to monitor

Exit code is `1` if status is CRITICAL, `0` otherwise.

## Agent Integration

When an agent needs to diagnose the server:
```
Input: python execution/server_tail.py --format json
Output: Structured JSON with all diagnostic data
```

The `--format json` flag produces machine-parseable output that can be
fed directly into other scripts or AI analysis pipelines.

## Learnings & Edge Cases

- **Port Auto-Detection**: If `--port` is not specified, the script reads
  `PORT=` from `.env`. Falls back to 8000 if not found.
- **SPA Fallback Detection**: If an endpoint returns HTML instead of JSON,
  it's flagged as `SPA_FALLBACK` (route not matched by the API router).
- **Panic Deduplication**: Span IDs are normalized via regex `Id(\d+)` → `Id(X)`.
  **WARNING**: The original `while`-loop approach caused an infinite loop because
  `Id(X)` still matches `Id(` — always use `re.sub()` for this.
- **Windows cp1252 Encoding**: The script reconfigures `sys.stdout` to UTF-8 on
  Windows because `cp1252` can't render emoji (🔬, ✅, ❌). Without this fix,
  `UnicodeEncodeError` crashes the script.
- **Windows `tasklist /V` Hang**: The `/V` (verbose) flag on `tasklist` iterates
  ALL processes with full column data, which hangs for 30+ seconds on machines
  with 400+ processes. Use `tasklist /FI "PID eq X" /FO CSV` (no `/V`) instead.
- **Windows Compatibility**: Process detection uses `netstat`/`tasklist` on
  Windows and `lsof` on Linux/macOS.
- **Exit Code**: Returns `1` if status is CRITICAL, `0` otherwise. Useful for
  CI/CD gating.


## Dependencies

- Python 3.8+ (stdlib only — no pip install needed)
- Network access to localhost (or target host)
- Read access to workspace `.env` and log files

[//]: # (Metadata: [server_tailing])

