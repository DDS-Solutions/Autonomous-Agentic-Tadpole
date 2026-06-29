> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Documentation**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[OPERATIONS_MANUAL]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

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

### Token Rotation Runbook

To execute zero-downtime key rotation, run the `rotate_token.py` script:

```bash
python execution/rotate_token.py
```

This will automatically:
1. Generate a new cryptographically secure 32-byte hexadecimal token.
2. Add the new token to `.env` as `NEURAL_TOKEN` and `NEURAL_TOKEN_NEW`.
3. Set the previous token as `NEURAL_TOKEN_OLD`.
4. During this rotation grace period, the engine accepts requests with any of these three tokens (using constant-time timing-attack proof comparison).

After all clients are successfully migrated, revoke the old tokens and close the grace period:

```bash
python execution/rotate_token.py --confirm
```

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
- delete agents (triggering cascading deletion)
- get swarm graph
- send tasks
- reset agents
- pause agents
- resume agents
- sync missions
- read/write/delete agent memories when `vector-memory` is enabled

Agent data is loaded from SQLite during `AppState::new` and persisted on graceful shutdown through batched writes.

### GDPR Cascading Deletion

When an agent is removed via `DELETE /v1/agents/:id`, the engine executes an atomic database transaction (`delete_agent_cascade`) to perform cascading cleanup of all dependent records, preventing residual PII.
*   **Cascade Target Tables**:
    *   `agents`: Removes the core agent identity and configuration.
    *   `agent_quotas`: Revokes systemic CPU, memory, and token budgets.
    *   `sync_manifests`: Clears filesystem sync states and cached files.
    *   `fallback_memories`: Clears non-vector memory storage.
    *   `agent_hires`: Cancels/removes sub-agent communication contracts.
    *   `audit_trail`: Anonymizes/purges audit history logs associated with the agent.
*   **Durable State Persistence**: The registry state is saved to the SQLite database instantly on deletion to ensure consistency.

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

### MCP Subprocess Security Hardening

The MCP server (`tadpole_mcp_server.py`) enforces strict sandboxing limits on legacy execution tools:
1. **Shell-Free Execution**: Subprocesses are spawned without intermediate shells using argument-splitting (`asyncio.create_subprocess_exec`) to prevent Command Injection.
2. **Command Compliance**: Command patterns undergo Shell Scanner filtering, rejecting forbidden operators (`|`, `&`, `;`, `>` etc).
3. **Type-Safe Validation**: Parameter schemas are validated at the host boundary (zero-dependency custom validation).
4. **Limits & Timeouts**: Subprocesses are constrained by a hard 30s timeout and set resource limits (`setrlimit` on CPU time and virtual memory address space where supported) to mitigate Denials of Service.

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

### Health & Observability Metrics

The endpoint `GET /v1/engine/health` returns detailed JSON telemetry for monitoring:
*   `status`: Global health indicator (`healthy` | `failed`).
*   `database`: Connection pool size, idle count, WAL file size, and busy timeout config.
*   `budget`: Total daily USD spent by agents, limits, and percentage used.
*   `swarm`: Total active agents, connected bunkers, and max depth bounds.
*   `uptime_seconds`: Engine server runtime duration.

### Alerting & Monitoring Configuration

*   **Prometheus Rules**: Configured in `monitoring/alerts.yml`. Triggers alerts on:
    *   `EngineDown`: Heartbeat endpoint fails for >30s.
    *   `HighLatency`: Endpoint response latency (p99) exceeds 500ms.
    *   `BudgetExhausted`: Swarm budget usage exceeds 80%.
    *   `SQLiteLocked`: Database busy/lock wait time exceeds 10s.
*   **Grafana Dashboard**: Configured in `monitoring/dashboard.json` for live status visualization.

Useful log tags in code comments and traces:

- `[Main]`
- `[Sidecar]`
- `[Engine]`
- `[Bootstrap]`
- `[Hydra-RS]`
- `[Auth]`
- `[Router]`
- `[SecurityHeaders]`
- `[Graph]` - tracing symbol graph builds and blast radius computations

## Code Intelligence & Blast Radius Operations

Primary API group: `/v1/intelligence`.

Operational capabilities include:
- **Codebase Dependency Graph**: Request the complete directed dependency graph of functions, structs, classes, and interfaces (`GET /v1/intelligence/graph`).
- **Dependent Blast Radius**: Calculate downstream dependency paths for a specific symbol to foresee refactoring scope (`GET /v1/intelligence/blast-radius?name=<Name>&path=<Path>`).
- **Interactive Swarm HUD**: The **Neural Map** toggle in the Operations Dashboard compiles the codebase structure on the first click (triggering lazy backend evaluation with a `"Synthesizing Symbol Graph..."` overlay) and caches it in memory for instant subsequent loads.
- **Sovereign Agent Safety**: The `get_blast_radius` MCP agent tool allows autonomous execution agents to map codebase linkages before executing code edits, ensuring zero-regression commits.
- **Thread-safe Graph Compilation**: Graph compilation executes decoupled background sweeps to scan and parse files outside main read/write guards, preserving high-concurrency read routing.

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

### Database Backup & Restore Runbook

To prevent data loss, the engine supports online, transaction-safe database backups (fully WAL-safe).

**Create a Backup**:
Run the backup script or npm shortcut:
```bash
npm run db:backup
# or: python execution/backup_sqlite.py
```
This runs SQLite's native `.backup` API (preventing locks), performs a `PRAGMA integrity_check`, and writes a `.meta.json` file with sizes and SHA-256 signatures for tamper-detection.

**Restore from a Backup**:
Run the restore script or npm shortcut:
```bash
npm run db:restore <backup_filename>
# or: python execution/restore_sqlite.py <backup_filename_or_path>
```
This verifies the backup's checksum and SQLite integrity, makes a safe backup copy of the current active database as `tadpole.db.bak` to prevent accidents, copies the files over, and runs row-count parity checks across all tables to confirm success.

### Sandbox State Snapshotting

To quickly checkpoint and restore sandbox states during development or testing, use:

```bash
# Save a snapshot (saves .env and database state to .tmp/snapshots/)
python execution/snapshot_state.py --save <snapshot_name>

# Restore a snapshot (restores .env and database state to active sandbox)
python execution/snapshot_state.py --restore <snapshot_name>
```

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

## Capacity & Threading Planning

The engine uses configurable Tokio runtime thread and stack parameters. See [CAPACITY_PLANNING.md](file:///g:/Autonomous-Agentic-Tadpole/docs/CAPACITY_PLANNING.md) for full capacity limits.

These parameters can be overridden via environment variables at startup:
*   `TOKIO_WORKER_THREADS`: Number of OS threads spawned for the multi-threaded event loop.
*   `TOKIO_MAX_BLOCKING_THREADS`: Max threads allowed for blocking I/O tasks.
*   `TOKIO_THREAD_STACK_SIZE`: Stack size (in bytes) allocated for each worker thread.

## Rollback & Recovery Automation

Automated scripts for Windows/PowerShell recovery and deployment are located in the `scripts/` directory:

*   **Deployment**: Running `scripts/deploy-linuxlite.ps1` builds a release package, backs up the currently running engine binary under `deploy_backups/server-rs_<version>.exe`, and copies the new binary into place.
*   **Rollback**: If a deployment fails or causes degradation, run `scripts/rollback.ps1` to stop the engine, swap in the previous working binary from `deploy_backups/`, and verify migration schema backward compatibility.

## Local Debug Endpoints

The engine provides local diagnostic endpoints under `/v1/system/debug` for analyzing execution state (exposed only when running in development/local mode):
*   `GET /v1/system/debug/services`: Returns a list of active system services, their runtime status, and loop telemetry.
*   `GET /v1/system/debug/queues`: Returns oversight queues, sequence IDs, and active channel capacities to locate bottlenecks.

## Sovereign Engine Hardening

The engine implements several strategies to ensure resilience and zero-panic operation:

- **Self-Annealing Intelligence**: The `PolyglotParser` provides structured feedback on malformed tool calls, allowing the `IntelligenceLoop` to automatically re-prompt models for correction.
- **Panic Remediation**: Critical paths in the bridge, parser, and security modules use safe error propagation (via `Result` and `AppError`) rather than non-recoverable panics.
- **Non-Blocking Orchestration**: All filesystem I/O in the MCP execution and Memory Palace rehydration modules is migrated to `tokio::fs` to prevent event-loop stalling.
- **Non-Blocking Graph Rebuilding**: Decoupling the filesystem walking and AST parsing tasks from the main thread lock prevents Axum router request timeout cascades during compilation sweeps.



[//]: # (Metadata: [OPERATIONS_MANUAL])
