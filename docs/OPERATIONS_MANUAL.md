> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Documentation**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[OPERATIONS_MANUAL]` in audit logs.
>
> ### AI Assist Note
> Tadpole OS operations manual.
>
> ### Debugging & Observability
> Traceability via `execution/parity_guard.py`.

# Tadpole OS Operations Manual

This manual reflects the current runtime behavior in `server-rs/src`, `src`, and `execution`.

## Local Startup

Required setup:

1. Install Node dependencies with `npm install`.
2. Copy `.env.example` to `.env`.
3. Set `NEURAL_TOKEN` or `NEURAL_ENGINE_ACCESS_TOKEN`.
4. Start the engine with `npm run engine`.
5. Start the dashboard with `npm run dev`.

Default endpoints:

- Dashboard: `http://localhost:5173`
- Engine: `http://127.0.0.1:8000`
- Health: `http://127.0.0.1:8000/v1/engine/health`

Windows helper scripts:

- `start_AA_tadpole.bat`: starts backend and frontend in separate windows.
- `start_backend.bat`: starts only the Rust engine.
- `start_frontend.bat`: starts only the Vite dashboard.
- `stop_AAtadpole.bat`: stops the local stack.

## Engine Lifecycle

The engine entry point is `server-rs/src/main.rs`.

Operational phases:

1. Panic hook registration.
2. Optional `WORKSPACE_ROOT` directory switch.
3. Tokio runtime creation.
4. Fast-path CLI handling for `--version`, `--help`, and `--status`.
5. Environment loading and tracing initialization.
6. `AppState` creation.
7. Background worker startup.
8. Actor registry startup.
9. Orchestrator startup.
10. Axum router binding.
11. Boot gate notification.
12. Graceful shutdown and registry flush.

If startup fails before tracing is fully available, check:

- terminal output
- `sidecar_panic.log`
- `sidecar_boot_error.log` if written under `WORKSPACE_ROOT`
- missing `NEURAL_TOKEN` or `NEURAL_ENGINE_ACCESS_TOKEN`
- port `8000` already in use
- SQLite lock or path issues under `data/`

## Authentication

Most management APIs require:

```http
Authorization: Bearer <NEURAL_TOKEN>
```

The engine accepts either:

- `NEURAL_TOKEN`
- `NEURAL_ENGINE_ACCESS_TOKEN`

Public routes:

- `GET /v1/engine/health`
- `GET /v1/engine/ws`
- `GET /v1/engine/live-voice`

All other operational routes should be treated as protected unless documented otherwise in `server-rs/src/router.rs`.

## Dashboard Operations

The dashboard boot logic lives in `src/App.tsx`. During startup it:

- syncs provider defaults
- syncs providers with the backend
- initializes visual monitoring
- starts VRAM/memory pressure polling
- optionally pre-warms browser inference when sentinel mode is enabled
- fetches the current agent registry
- applies theme and density settings

Dashboard pages are registered in `src/constants/routes.ts`.

## Agent And Swarm Management

Primary API group: `/v1/agents`.

Supported operations include:

- list agents
- create agents
- update agents
- get swarm graph
- send tasks
- reset agents
- pause agents
- resume agents
- sync missions
- read/write/delete agent memories when `vector-memory` is enabled

Agent data is loaded from SQLite during `AppState::new` and persisted on graceful shutdown through batched writes.

## Oversight And Governance

Primary API groups:

- `/v1/oversight`
- `/v1/governance`
- `/v1/sovereign`

Operational capabilities include:

- pending oversight decisions
- oversight ledger
- security quotas
- mission quotas
- audit trail
- agent health
- integrity status
- policies
- governance blueprints
- sovereign manifest
- mission session history and branch state

## Model And Provider Management

Primary API group: `/v1/model-manager`.

Capabilities include:

- list/update/delete providers
- test provider connections
- sync provider models
- list/update/delete models
- read model-store catalog
- pull models through local provider endpoints

Provider keys are read from `.env` and supported by `.env.example`.

## Skills, MCP, And Execution

Primary API group: `/v1/skills`.

Execution layer paths:

- `execution/`
- `execution/core/`
- `execution/skills/`
- `execution/tadpole_mcp_server.py`

Capabilities include:

- list skills
- list/read manifests
- list and execute MCP tools
- import/promote/register capabilities
- scan workspace skills
- manage scripts, workflows, and hooks
- resolve capability proposals

MCP bridge endpoints:

- `GET /v1/mcp/sse`
- `POST /v1/mcp/message`

## Continuity Jobs

Primary API group: `/v1/continuity`.

The scheduler starts from `startup::spawn_background_tasks` and executes jobs through `agent::continuity::executor`.

Capabilities include:

- create/list/get/update/delete jobs
- list job runs
- enable/disable jobs
- create/list/delete workflows
- add workflow steps

## Observability

Signals and diagnostics:

- `engine:health` events emitted by the heartbeat loop.
- WebSocket stream at `/v1/engine/ws`.
- live voice stream at `/v1/engine/live-voice`.
- tracing spans from the Rust middleware stack.
- telemetry aggregation from `server-rs/src/telemetry/`.
- dashboard views under `/engine`, `/dashboard`, `/oversight`, `/security`, and `/benchmarks`.

Useful log tags in code comments and traces:

- `[Main]`
- `[Sidecar]`
- `[Engine]`
- `[Bootstrap]`
- `[Hydra-RS]`
- `[Auth]`
- `[Router]`
- `[SecurityHeaders]`

## Database Operations

Default database:

```text
data/tadpole.db
```

Override:

```ini
DATABASE_URL=sqlite:/absolute/path/to/tadpole.db
```

Migration files:

```text
server-rs/migrations/
```

Database initialization is handled by `server-rs/src/db.rs`.

## Feature-Gated Operations

Memory search and agent memory routes depend on:

```bash
cargo run --manifest-path server-rs/Cargo.toml --features vector-memory
```

Without `vector-memory`, memory routes return `501 Not Implemented`.

Optional audio/native dependencies are enabled with:

```bash
cargo run --manifest-path server-rs/Cargo.toml --features neural-audio
```

## Verification

Frontend:

```bash
npm run test
npm run build
```

Rust:

```bash
cargo test --manifest-path server-rs/Cargo.toml
```

Python utilities:

- `execution/verify_all.py`
- `execution/verify_ai_context.py`
- `execution/parity_guard.py`
- `execution/sovereign_audit.py`

Documentation and release metadata:

```bash
npm run docs:api
npm run docs:parity
npm run version:sync
```

## Sovereign Engine Hardening

The engine implements several strategies to ensure resilience and zero-panic operation:

- **Self-Annealing Intelligence**: The `PolyglotParser` provides structured feedback on malformed tool calls, allowing the `IntelligenceLoop` to automatically re-prompt models for correction.
- **Panic Remediation**: Critical paths in the bridge, parser, and security modules use safe error propagation (via `Result` and `AppError`) rather than non-recoverable panics.
- **Non-Blocking Orchestration**: All filesystem I/O in the MCP execution and Memory Palace rehydration modules is migrated to `tokio::fs` to prevent event-loop stalling.

[//]: # (Metadata: [OPERATIONS_MANUAL])
