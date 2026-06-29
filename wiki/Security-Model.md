> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[Security_Model]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

# 🔐 Security Model

## Overview

A-A Tadpole implements a **defense-in-depth** security model with multiple independent layers:

1. **Authentication** — Bearer token validation on all protected routes
2. **Token Rotation** — Zero-downtime dual-token grace window
3. **Rate Limiting** — Brute-force protection on auth endpoints
4. **Shell Scanning** — Pattern-based dangerous command detection
5. **MCP Sandboxing** — Subprocess isolation with resource limits
6. **Audit Trail** — Merkle-chained tamper-evident log
7. **Budget Guard** — Per-agent USD spend enforcement
8. **Path Traversal Protection** — Workspace boundary enforcement
9. **Secret Redaction** — Automatic scrubbing from logs and responses
10. **XSS Hardening** — Error response sanitization pipeline

---

## Authentication

All `/v1` management routes require:

```http
Authorization: Bearer <NEURAL_TOKEN>
```

WebSocket connections use a subprotocol instead:

```javascript
new WebSocket(url, [`tadpole.${NEURAL_TOKEN}`])
```

### Implementation Details

- Token comparison uses the `subtle` crate for **constant-time equality** — prevents timing-based side-channel attacks
- `NEURAL_ENGINE_ACCESS_TOKEN` is accepted as a secondary token for legacy compatibility
- Missing or malformed tokens return `401 Unauthorized` with RFC 9457 Problem Details

---

## Zero-Downtime Token Rotation

The auth middleware supports a **three-token grace window** for safe production rotation:

```
NEURAL_TOKEN        → Primary token (always accepted)
NEURAL_TOKEN_OLD    → Previous token (accepted during rotation window)
NEURAL_TOKEN_NEW    → Replacement token (accepted to pre-validate)
```

### Rotation Procedure

```bash
# Step 1: Stage the new token alongside the current one
# In .env:
NEURAL_TOKEN=current-token
NEURAL_TOKEN_OLD=current-token
NEURAL_TOKEN_NEW=new-token

# Step 2: Restart the engine (no downtime — both tokens valid)
npm run engine

# Step 3: Update all clients to use NEURAL_TOKEN_NEW

# Step 4: Promote the new token and remove old
NEURAL_TOKEN=new-token
# Remove NEURAL_TOKEN_OLD and NEURAL_TOKEN_NEW

# Step 5: Restart engine
npm run engine
```

Or use the automated script:

```bash
python execution/rotate_token.py --new-token new-token-value
```

---

## MCP Subprocess Sandboxing

The `execution/tadpole_mcp_server.py` spawns Python skill scripts. Each subprocess is hardened:

| Protection | Implementation |
|------------|----------------|
| No shell injection | `asyncio.create_subprocess_exec()` — never `shell=True` |
| Command splitting | `shlex.split()` before execution |
| Skill allowlist | Only registered skill IDs are executable |
| Input validation | JSON Schema validation on all tool arguments |
| Execution timeout | Hard 30-second limit via `asyncio.wait_for()` |
| Resource limits | `resource.setrlimit(RLIMIT_CPU, ...)` on Linux/Docker |
| Memory cap | `resource.setrlimit(RLIMIT_AS, 512MB)` on Linux/Docker |

---

## Shell Scanner

`security/scanner.rs` blocks dangerous patterns before any shell-adjacent execution:

```rust
// Blocked patterns (examples)
rm -rf / | curl | wget | eval | exec | base64 | nc | ncat
// Path traversal
../ | ..\\ | /etc/passwd | /proc/
// Code injection
$(command) | `command` | &&bad || bad ; bad
```

---

## Merkle Audit Trail

Every significant action is recorded in `security/audit.rs` as a Merkle-chained entry:

```
Entry N:
  agent_id: "agent-42"
  action: "execute_tool"
  params: { ... }
  timestamp: 2026-06-26T22:00:00Z
  prev_hash: sha256(Entry N-1)
  signature: hmac(entry, AUDIT_SECRET)
```

Tamper detection: any modification to a historical entry invalidates all subsequent hashes. The `verify_head()` method validates the full chain.

---

## Budget Guard

`security/metering.rs` enforces per-agent and system-wide spending limits:

```rust
// Before any LLM call:
budget_guard.check_and_reserve(agent_id, estimated_cost)?;

// After the call (actual cost known):
budget_guard.commit(agent_id, actual_cost);
```

Debounced flush writes spend data to SQLite every 5 seconds to avoid write amplification.

---

## Path Traversal Protection

The `utils/security.rs` `validate_path()` function canonicalizes and checks all file paths:

```rust
// All file access goes through:
validate_path(base_dir, user_supplied_path)?;
// → Resolves symlinks, checks prefix ⊆ base_dir
// → Returns AppError::Forbidden if outside workspace
```

---

## Error Response Hardening (XSS Prevention)

`error.rs` applies a 3-stage pipeline to all error messages before sending to clients:

```
Raw Error String
    │
    ├─ 1. TRUNCATE  → max 2048 characters
    ├─ 2. SANITIZE  → HTML-escape <, >, &, ", '
    └─ 3. REDACT    → remove secrets via SecretRedactor
         │
         └─ Safe ProblemDetails { title, detail, status }
```

---

## Privacy Mode

Set `PRIVACY_MODE=true` in `.env` to restrict all execution to local-only providers:

- Blocks cloud LLM API calls (OpenAI, Anthropic, Google, etc.)
- Blocks IKS embedding calls (requires Google API)
- Only Ollama and local models are accepted
- Audit trail is still written locally

---

## Security Checklist for Production

- [ ] Set a strong random `NEURAL_TOKEN` (32+ chars)
- [ ] Set `BIND_ADDRESS=127.0.0.1` (never expose to public internet)
- [ ] Set `ALLOWED_ORIGINS` to your specific frontend domain
- [ ] Rotate `NEURAL_TOKEN` quarterly using the rotation runbook
- [ ] Enable SQLite WAL backups via `execution/backup_sqlite.py`
- [ ] Review `/v1/oversight/audit-trail` periodically
- [ ] Run `python execution/parity_guard.py` after every deployment
- [ ] Monitor `/v1/engine/health` `budget.total_spent_usd` for anomalies

[//]: # (Metadata: [Security_Model])
