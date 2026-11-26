> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[Testing_Guide]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

# 🧪 Testing Guide

## Test Stack

| Layer | Framework | Location |
|-------|-----------|----------|
| Rust unit + integration | `cargo test` / `npm run test:rust` | `server-rs/src/**/*_tests.rs` |
| Invariant gates | Vitest (`npm run test:invariants`) | `tests/invariants/` |
| WASM Codec | `cargo test` / `npm run test:wasm` | `crates/wasm-codec/` |
| React unit | Vitest (`npm test`) | `src/**/*.test.ts(x)` |
| Python unit | pytest | `tests/unit/` |
| React E2E | Playwright | `tests/e2e/` |
| Type safety | `tsc --noEmit` | `npm run typecheck:ts7` |

---

## Rust Tests

### Run All Tests

```bash
# Via npm script
npm run test:rust

# Or directly via cargo
cargo test --manifest-path server-rs/Cargo.toml
```

### Run Specific Test

```bash
# A specific test
cargo test --manifest-path server-rs/Cargo.toml -- routes::health_endpoint_tests

# With output
cargo test --manifest-path server-rs/Cargo.toml -- --nocapture
```

### Test Modules (v1.1.59)

| Crate / Module | Scope |
|----------------|-------|
| `crates/tadpole-types` (`cargo test -p tadpole-types`) | Core enums, model capabilities, mission DTOs, MFS RAG scoring |
| `crates/tadpole-intelligence` (`cargo test -p tadpole-intelligence`) | Tree-sitter Rust/TS parser, `CodeSymbolGraph`, blast-radius & incremental caching |
| `crates/tadpole-guard` (`cargo test -p tadpole-guard`) | `syn`-based Rust router AST extraction & scheduler invariant checks |
| `crates/tadpole-sec` (`cargo test -p tadpole-sec`) | `.gitleaks.toml` single-source-of-truth `RegexSet` secret scanner |
| `server-rs` (`cargo test -p server-rs --lib`) | Routes (`agent`, `auth`, `benchmarks` readiness probe, `intelligence`, `deploy`, `skills`, `shutdown`, `token_rotation`), middleware (`auth` tiered scopes, rate limit, CORS), security (Merkle audit, budget metering, shell scanner, `validate_tokenized_command`), swarm decomposition |
| `crates/wasm-codec` (`npm run test:wasm`) | Binary MessagePack / WASM telemetry framing |

### Writing a New Route Test

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use axum::body::Body;
    use http::{Request, StatusCode};
    use tower::ServiceExt; // for .oneshot()

    #[tokio::test]
    async fn test_my_endpoint() {
        let state = AppState::new_minimal_mock().await;
        // IMPORTANT: signal boot gate or requests will hang
        state.notify_boot_complete();

        let app = create_router(state.into());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/my-route")
                    .header("Authorization", "Bearer test-token")
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
```

> **Critical:** Always call `state.notify_boot_complete()` before sending requests in tests. The boot-gate middleware blocks all requests until this is called.

---

## Python Tests

```bash
# Run all unit tests
python -m pytest tests/unit/ -v

# Run a specific test file
python -m pytest tests/unit/test_backup_integrity.py -v

# With coverage
python -m pytest tests/unit/ --cov=execution --cov-report=term-missing
```

### Python Test Modules

| File | Coverage |
|------|----------|
| `test_backup_integrity.py` | SHA-256 hash, integrity_check, file creation |
| `test_token_rotation.py` | rotate_token.py dual-token logic |
| `test_mcp_sandbox.py` | Allowlist, timeout, shell=False enforcement |
| `test_snapshot_state.py` | Snapshot creation and restore |
| `test_verify_ai_context_fix.py` | AI context marker parity |

---

## React / Frontend Tests

```bash
# Unit tests via Vitest
npm test

# With coverage
npm run test:coverage

# Type check (no compilation output)
npm run typecheck:ts7
```

---

## Invariant Gate Tests

The codebase enforces strict invariant gates defined in `tests/invariants/`:

```bash
npm run test:invariants
```

| Invariant Suite | Invariants Enforced |
|-----------------|---------------------|
| `agent_lifecycle.test.ts` | State machine transitions, atomic claiming |
| `audit_merkle.test.ts` | Tamper detection, hash chain continuity |
| `auth_boundary.test.ts` | Unauthenticated rejection, WebSocket subprotocol |
| `boot_gate.test.ts` | Boot-gate barrier before ready signal |
| `budget_metering.test.ts` | Token cost floor, reservation/commit |
| `config_defaults.test.ts` | Safe default isolation, strict type coercion |
| `error_sanitization.test.ts` | Truncation (2048), HTML-escaping, secret redacting |
| `path_traversal.test.ts` | Base directory confinement, symlink resolution |
| `retries_occ.test.ts` | Optimistic concurrency control retry guarantees |
| `rpc_envelope.test.ts` | RPC error format, RFC 9457 problem details |
| `sidecar_lifecycle.test.ts` | Process supervisory, clean SIGTERM/SIGINT teardown |

---

## WASM Codec Tests

```bash
npm run test:wasm
# Or directly via cargo:
cargo test --manifest-path crates/wasm-codec/Cargo.toml
```

---

## CI Pipeline

GitHub Actions runs on every push to `main` and on all PRs:

```yaml
# .github/workflows/ci.yml (simplified)
jobs:
  rust:
    runs-on: ubuntu-latest
    steps:
      - cargo test --bin server-rs
      - cargo clippy -- -D warnings

  frontend:
    runs-on: ubuntu-latest
    steps:
      - npm run build
      - npm run test

  python:
    runs-on: ubuntu-latest
    steps:
      - python -m pytest tests/unit/
```

---

## Common Test Pitfalls

### Boot Gate Hangs (408 Timeout)

**Symptom:** Test requests time out after 120 seconds.  
**Cause:** `wait_for_system_ready` middleware blocks until `notify_boot_complete()` is called.  
**Fix:** Add `state.notify_boot_complete();` before calling `create_router()`.

### NOT NULL Schema Failures

**Symptom:** `NOT NULL constraint failed: agents.metadata`  
**Cause:** Test INSERT missing required columns from the migration schema.  
**Fix:** Always include all `NOT NULL` columns when seeding test data. Check `server-rs/migrations/` for the current schema.

### Feature-Gated Endpoints Return 501

**Symptom:** `/v1/search/memory` returns `501 Not Implemented`.  
**Cause:** The `vector-memory` Cargo feature is not enabled.  
**Fix:** Test the fallback behavior OR enable the feature: `--features vector-memory`.

### Token Auth Failures in Tests

**Symptom:** Requests return 401 in integration tests.  
**Cause:** `NEURAL_TOKEN` env var not set; `AppState::new_minimal_mock()` uses `"test-token"`.  
**Fix:** Use `"Bearer test-token"` in test request headers.

[//]: # (Metadata: [Testing_Guide])
