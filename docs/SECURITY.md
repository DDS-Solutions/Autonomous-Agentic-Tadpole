> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Documentation**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[SECURITY]` in audit logs.
>
> ### AI Assist Note
> Tadpole OS security policy.
>
> ### Debugging & Observability
> Traceability via `execution/parity_guard.py`.

# Tadpole OS Security Policy

This document reflects the security controls currently implemented in the Rust engine and frontend runtime.

## Authentication Boundary

The engine requires an access token outside tests. Configure one of:

```ini
NEURAL_TOKEN=your-secret-token
NEURAL_ENGINE_ACCESS_TOKEN=your-secret-token
```

Protected REST requests use:

```http
Authorization: Bearer <token>
```

The auth middleware is implemented in `server-rs/src/middleware/auth.rs` and uses constant-time token comparison via the `subtle` crate.

Browser WebSocket upgrades may pass auth through:

```http
Sec-WebSocket-Protocol: bearer.<token>
```

Public bypasses currently exist for:

- `GET /v1/engine/health`
- `GET /v1/engine/ws`
- `GET /v1/engine/live-voice`

Route protection is applied in `server-rs/src/router.rs`.

## Middleware Controls

The router applies these controls:

- boot readiness gate
- auth brute-force limiter
- security headers
- request ID injection
- rate-limit headers
- tracing spans
- deprecation middleware
- request timeout
- response compression
- CORS

Security headers are injected by `server-rs/src/middleware/security_headers.rs`:

- Content-Security-Policy
- Strict-Transport-Security
- X-Content-Type-Options
- X-Frame-Options
- Referrer-Policy

## CORS Status

`server-rs/src/middleware/cors.rs` enforces an allow-list from `ALLOWED_ORIGINS`.

Behavior:

- `ALLOWED_ORIGINS=*` enables wildcard troubleshooting mode and disables credentials.
- A comma-separated list enables those exact origins with credentials.
- An empty value falls back to local development origins: `localhost`/`127.0.0.1` on ports `5173` and `3000`, plus `tauri://localhost`.

## Secret Redaction

Secret redaction is centralized through `server-rs/src/secret_redactor.rs` and used by AppState broadcast helpers before log/event publication.

Sensitive environment keys include:

- `NEURAL_TOKEN`
- cloud provider API keys
- other provider and credential-style values recognized by the redactor

Logs and telemetry should never intentionally include raw token values.

## Shell Scanner

`server-rs/src/security/scanner.rs` implements command/script risk detection. It checks:

- known loaded secrets
- common API key formats
- raw secret export patterns
- command concatenation such as `;`, `&&`, and `||`
- pipes and redirection
- command substitution
- optional aggressive checks when `AGGRESSIVE_SECURITY=true`

The scanner is intentionally conservative and can flag valid complex shell commands. Risky results should be treated as requiring review rather than automatically safe execution.

## Audit, Budget, And Permissions

Security state is owned by `SecurityHub` in `server-rs/src/state/hubs/sec.rs` and assembled through `AppState`.

The security hub includes:

- Merkle audit trail
- budget guard
- shell scanner
- secret redactor
- system monitor
- permission policy
- deploy token

Oversight/security API routes are exposed under `/v1/oversight/security/*`.

## Privacy Mode

`PRIVACY_MODE=true` steers runtime behavior toward local-only provider use. The privacy guard is started from `startup::spawn_background_tasks`.

Local insecure model-provider HTTP can be allowed with:

```ini
TADPOLE_ALLOW_LOCAL_HTTP=true
```

## Deployment Notes

Before deploying beyond local development:

1. Set a strong `NEURAL_TOKEN` or `NEURAL_ENGINE_ACCESS_TOKEN`.
2. Set `ALLOWED_ORIGINS` explicitly for deployed or shared environments.
3. Validate that `.env` is excluded from source control.
4. Run frontend and Rust test suites.
5. Confirm protected endpoints reject missing and invalid bearer tokens.
6. Confirm the dashboard stores and sends the same token configured for the engine.
7. Review feature gates before enabling `vector-memory` or `neural-audio`.

## Security Test Matrix

| Area | Expected behavior | Suggested check |
| --- | --- | --- |
| Public health | `GET /v1/engine/health` succeeds without auth | call endpoint without `Authorization` |
| Protected API | `/v1/agents` rejects missing auth | call endpoint without token and expect `401` |
| Invalid token | protected routes reject wrong bearer token | call with `Authorization: Bearer wrong` and expect `401` |
| Valid token | protected routes accept configured token | call with `.env` token and expect non-`401` |
| WebSocket token | protected WS flows can use `bearer.<token>` subprotocol when needed | verify browser upgrade path when route is protected |
| CORS allow-list | configured origins are allowed | set `ALLOWED_ORIGINS` and verify preflight |
| CORS wildcard | `ALLOWED_ORIGINS=*` disables credentials | verify wildcard response has no credential support |
| Vector memory disabled | memory routes return `501` without feature | run engine without `vector-memory` and call memory route |
| Vector memory enabled | memory routes become available | run engine with `--features vector-memory` |

[//]: # (Metadata: [SECURITY])
