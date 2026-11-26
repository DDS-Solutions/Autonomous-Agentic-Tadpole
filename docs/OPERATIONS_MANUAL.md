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
> Traceability via `execution/parity_guard.py`.

# Tadpole OS Operations Manual

This manual reflects the current runtime behavior in `server-rs/src`, `src`, and `execution`.

## Local Startup

The engine starts the system actor supervisor automatically. `ENABLE_EVENT_AUTOMATION=true` enables configured event-triggered continuity jobs; `ENABLE_COGNITIVE_MEMORY=true` enables provider-backed memory consolidation and can incur inference charges. Both flags default to false. Consolidation intervals have a 10-second minimum, thresholds range from 2 to 100 entries, and a batch reads at most 100 entries. These workers stop during engine shutdown. A memory save fails explicitly if its actor is unavailable; it never acknowledges discarded data.

Agent persistence conflicts require reloading the current state and reviewing the intended change. The engine does not advance an in-memory version after a failed batch or silently retry a stale full object. Ledger-linked agents cannot be deleted. Scheduled executions register their runner handles before starting, so gateway pause, reset, and delete can cancel them.

Cancelled missions retain acknowledged provider usage, including partial turns and completed workflow steps. Temporary terminal-write failures leave mission ownership and its receipt available for recovery; the stale reaper preserves live tasks and retries stopped ones. A per-agent recovery error is logged without stopping recovery of the remaining candidates; the sweep then returns the first error. Initialization, terminal persistence, and stale recovery finish their commit/publication operations independently of caller cancellation. Receipts and pending quota charges are in memory, so abrupt process termination still needs separate durability validation.

Event automation admits at most 16 concurrent job executors. Slow jobs no longer block every later trigger; the current event's remaining mappings wait for a slot. The broadcast queue can still lose events during sustained overload, with lag reported in logs. Stopping the monitor lets already admitted executors finish their run-record cleanup.

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
- `stop_AA_tadpole.bat`: stops the local stack.

## Engine Lifecycle

The engine entry point is `server-rs/src/main.rs`.

Operational phases:

1. Fast-path CLI handling for `--version`, `--help`, and `--status` (before env/schema validation).
2. Emergency panic hook registration (`sidecar_panic.log` with secret redaction).
3. `Config::load()` (`.env` loading + `.env.schema` validation) and optional `WORKSPACE_ROOT` directory switch.
4. Custom Tokio runtime creation (`startup::build_custom_runtime`).
5. Tracing initialization and environment loading.
6. `AppState` creation.
7. Axum router assembly and TCP listener bind (`SO_REUSEADDR`).
8. Background workers, OTP actor supervisor/registry, and orchestrator startup.
9. Boot gate notification.
10. Graceful shutdown and registry flush.

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

- `GET /health` (root engine health ping)
- `GET /v1/engine/health` (API v1 engine health ping)

WebSocket routes (`GET /v1/engine/ws`, `GET /v1/engine/live-voice`) are protected and require token authentication via the `Sec-WebSocket-Protocol: bearer.<token>` subprotocol header.

All other operational routes are protected and require `Authorization: Bearer <token>` unless documented otherwise in `server-rs/src/router.rs`.

### Token Rotation Runbook

To execute zero-downtime key rotation, run `execution/rotate_token.py`:

```bash
python execution/rotate_token.py              # default --grace-secs 300
# Restart the engine, migrate clients, then finalize:
python execution/rotate_token.py --confirm    # pass --force to revoke before grace expires
```

This will automatically:
1. Generate a new cryptographically secure 32-byte hexadecimal token.
2. Write the new token to `.env` as `NEURAL_TOKEN` and `NEURAL_TOKEN_NEW`.
3. Preserve the previous token as `NEURAL_TOKEN_OLD` and record `NEURAL_TOKEN_ROTATED_AT` + `NEURAL_TOKEN_GRACE_SECS`.
4. After restarting the engine, the grace window accepts `NEURAL_TOKEN`, `NEURAL_TOKEN_NEW`, and `NEURAL_TOKEN_OLD` (using constant-time SHA-256 digest comparison).
5. The grace period is strictly bounded by `NEURAL_TOKEN_ROTATED_AT` and `NEURAL_TOKEN_GRACE_SECS`. If either value is omitted or expired, `NEURAL_TOKEN_OLD` is rejected immediately (fail-closed). Running `--confirm` strips the rotation variables so only `NEURAL_TOKEN` remains active.

### Model Slot Routing & Local Privacy Optimizer Runbook

To audit and optimize all registered agent model slots for local air-gapped inference:

```bash
python execution/optimize_local_slot_routing.py
```

This utility performs the following operations:
1. Validates `PRIVACY_MODE` from `.env`. If disabled, it safely skips local overwrites to protect cloud provider routing.
2. Queries the local Ollama daemon for installed models.
3. Dynamically identifies strategic leads (CEO, Architect, Commander, Lead) and assigns them **Slot 1** (`gemma4:12b` or high-capacity reasoning model).
4. Assigns fast sub-worker specialists to **Slot 2** (`phi3.5-safe:latest` / `gemma4:e4b` for sub-second execution).
5. Performs a **non-destructive** database update modifying only model configurations while preserving agent health, failure counters, and active statuses.

## Dashboard Operations

The dashboard boot logic lives in `src/App.tsx`. During startup it:

- syncs provider defaults and backend provider registrations
- initializes visual monitoring and memory/VRAM pressure polling
- runs optional First-Run Overlord onboarding (`src/components/onboarding/FirstRun_Wizard.tsx`) when `onboarding_completed` is false and pre-verifies `NEURAL_TOKEN` before storing it strictly in RAM (`INV-SEC-002`)
- starts the autonomous Browser Sentinel background daemon if `sentinel_mode` is enabled (`src/services/sentinel_daemon.ts`)
- fetches the current agent registry
- pre-loads route chunks in background idle time
- applies theme and density settings

Dashboard pages are registered in `src/constants/routes.ts`.

### Operating the Browser Sentinel (Governance Console)

The Browser Sentinel is managed and monitored directly through the **Governance** view (`/governance`):

1. **Enabling/Disabling Sentinel Mode**:
   - Toggle the **Active Sentinel Mode** switch in the Browser Sentinel card.
   - When enabled, the `sentinel_daemon` initializes the quantized model (`SmolLM-360M-Instruct` on WebGPU/WASM) and schedules 25-second idle health evaluations.
   - When disabled, the background daemon immediately un-registers, disposes of cached WebGPU buffers, and enters the `Sentinel Offline (Dormant)` state.

2. **Reading Sentinel Status Ribbons**:
   - `Sentinel Offline (Dormant)`: Sentinel mode is disabled; no GPU or memory is allocated.
   - `Warming Up (progress%)`: Model tensors are downloading/hydrating into WebGPU/WASM runtime.
   - `Auditing Active DOM...`: An active background or manual evaluation cycle is executing.
   - `Sentinel Active · WEBGPU`: Model is fully loaded and standing by on WebGPU hardware acceleration.
   - `Pipeline Exception`: A device loss or inference exception occurred.

3. **Manual Trigger & Emergency Reset**:
   - **Run Audit Now**: Bypasses the 25-second idle interval to immediately audit the current DOM for broken surfaces, error alerts, or high entropy.
   - **Reset**: Purges all WebGPU buffers, cleans pipeline caches, and forces re-initialization of the model runtime without reloading the browser tab.

4. **Deterministic Pre-Flight DLP Shield**:
   - Runs automatically on every command in `src/logic/command_processor.ts` (dispatching through `src/logic/commands/`) and on every DOM scrape.
   - All known API keys (`sk-proj-...`, `sk-ant-...`, `AIzaSy...`, `ghp_...`, Bearer tokens, private keys) are scrubbed to `[REDACTED_*]` before local reasoning or network transmission.

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
- read/write/delete agent memories (LanceDB when `vector-memory` is enabled, or SQLite `fallback_memories` by default)

Agent data is loaded from SQLite during `AppState::new` and persisted on graceful shutdown through batched writes.

### GDPR Cascading Deletion

When an agent is removed via `DELETE /v1/agents/{id}`, the engine executes an atomic `BEGIN IMMEDIATE` transaction (`delete_agent_cascade` in `server-rs/src/agent/persistence.rs`) that enforces financial and forensic invariants before cleaning up dependent records:
*   **Ledger Counterparty Guard**: If the agent appears in `a2a_ledger` (`debit_agent_id` or `credit_agent_id`), deletion is rejected with `409 Conflict` to preserve financial counterparty history (suspend the agent instead).
*   **Forensic & Oversight Preservation**:
    *   `audit_trail`: **Preserved intact** — it is an append-only cryptographic Merkle chain (`prev_hash -> current_hash`), so historical entries are never deleted.
    *   `oversight_log`: Preserves oversight evidence while detaching its optional mission FK (`SET mission_id = NULL`).
*   **Cascade Target Tables (Deleted Child-First)**:
    *   Mission artifacts & logs: `mission_logs`, `swarm_context`, `agent_directives`, `peer_reviews`, `mission_nodes`, `mission_quotas`, `file_revisions`, and `mission_history`.
    *   Agent state & contracts: `agent_economics_meta`, `agent_quotas`, `sync_manifest`, `fallback_memories`, `agent_hires`, `skill_subscriptions`, `agent_tasks`, and finally `agents`.
*   **Runtime Cleanup**: After the SQLite transaction commits, any active runner handle is aborted and the agent + mission contexts are evicted from the in-memory registry.

## Oversight And Governance

Primary API groups:

- `/v1/oversight`
- `/v1/governance`
- `/v1/sovereign`
- `/v1/a2a`

Operational capabilities include:

- pending oversight decisions with 1-click guided remediation auto-population
- oversight ledger and audit logs
- **Governance Settings Management**: `GET /v1/oversight/settings` and `PUT /v1/oversight/settings` backed by the `governance_settings` SQLite table with ceiling enforcement (`max_agents <= 10,000`, `max_clusters <= 1,000`, `max_swarm_depth <= 64`, `max_task_length <= 1,048,576`, `default_budget_usd <= $1,000,000.0`)
- **Swarm Token Burn Analytics**: `GET /v1/oversight/token-burn` providing authoritative token and cost burn reconciled from `agent_tasks` or `agents` fallback with real-time Prometheus gauge export
- **Mission Scoping Authorization**: Fine-grained `OversightPrincipal` access control ensuring operators can only decide or inspect oversight entries within their authorized mission clusters
- **Error Sentinel Triage**: Agent QA-99 mid-flight fault classification (`directives/error_sentinel_triage.md`) attaching remediation prescriptions to tripped circuit breaker entries
- **RFC 9457 Problem Details Remediation Engine**: Structured `remediation` metadata emitted across all HTTP, tool execution, and runtime error paths
- security quotas
- mission quotas
- audit trail
- agent health
- integrity status
- policies
- governance blueprints
- sovereign manifest
- mission session history and branch state
- **A2E-01 Two-Phase Commit (2PC) Budget Ledger** (`/v1/a2a/prepare`, `/v1/a2a/commit`, `/v1/a2a/rollback`)
- **24-Hour Rolling Budget Resets** (`agent_economics_meta` table with automatic 24h reset checks)
- **Lock-Aware Projected Spend Calculation**: Evaluates $\text{Projected} = \text{Spent Today} + \sum \text{Pending Locks} + \text{New Amount}$ before issuing locks to prevent sub-agent overruns
- **Zero-Drift Micro-USDC Integer Accounting**: Integer arithmetic (`u64`) in micro-USDC ($1.00 = 1,000,000 micros) eliminating floating-point rounding errors

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

### Token & Context Optimization Engines (`GTK-01`)

The engine integrates two high-performance token optimization modules to accelerate monologue turns and prevent context window overflows:
1. **Model-Aware `TokenizerService`** (`server-rs/src/agent/tokenizer.rs`): Sub-microsecond (`< 1.0 µs`) BPE token counter powered by `tiktoken-rs` and a bounded 4,096-entry `moka::sync::Cache` (lock-free LRU/TinyLFU). Adjusts counts using model-aware multipliers (`Qwen`: 1.05x, `Llama 3`/`DeepSeek`: 1.02x, `Gemma`: 0.95x, `OpenAI`: 1.0x).
2. **2-Tier `ContextManager`** (`server-rs/src/agent/context_manager.rs`):
   - **Tier 1 (Zero-Cost Heuristic Compaction)**: Automatically collapses redundant CLI output and consecutive success logs (`tool_result: Success`) at **$0 cost and 0ms latency** with fact-preservation guards.
   - **Tier 2 (Semantic LLM Summarization)**: Automatically synthesizes a dense "Condensed State" when history reaches 80% context window saturation.

## Skills, Hybrid RAG, And Execution

Primary API group: `/v1/skills`.

Execution layer paths:

- `execution/`
- `execution/core/`
- `execution/skills/`
- `execution/tadpole_mcp_server.py`
- `execution/parity_guard.py` (`parity_guard`: Documentation-to-code synchronization gate)
- `execution/verify_ai_context.py` (`verify_ai_context`: AI context note and trace scope verification)
- `execution/security_scan.py` (`security_scan`: Automated security and vulnerability auditor)
- `execution/verify_telemetry.py` (`verify_telemetry`: Telemetry stream and pulse validator)
- `execution/resource_check.json` (`resource_check`: System resource and compute profile check)
- `execution/tool_loop_guard.py` (10-iteration ceiling & 3-repetition circuit breaker)
- `execution/evaluate_annealing.py` (Fault registry analyzer & self-annealing proposer)
- `execution/cargo_fast_check.py` (Fast background compiler checker)
- `execution/mission_diagnose.py` (`mission_diagnose`: Active and failed mission diagnostics via SQLite and OpenTelemetry correlation)
- `execution/debrief_mission.py` (`debrief_mission`: Mission debriefing and wisdom extraction committing to long-term memory)
- `execution/parse_errors.py` (`parse_errors`: Cargo check diagnostics parser for autonomous self-healing code repair)

Capabilities include:

- list skills
- list/read manifests
- list and execute MCP tools
- import/promote/register capabilities
- scan workspace skills
- manage scripts, workflows, and hooks
- resolve capability proposals
- **Hybrid RAG Triad**:
  - **LanceDB Vector RAG & SQLite Fallback** (`server-rs/src/agent/memory.rs`, `server-rs/src/routes/memory.rs`): High-dimensional semantic search with graceful SQLite fallback (`fallback_memories`, returning 200 OK) when vector memory is unconfigured or unavailable.
  - **TrustGraph GraphRAG** (`server-rs/src/agent/trustgraph.rs`): Directed entity-relation graph engine with $O(N)$ BFS multi-hop context traversal.
  - **BM25 Lexical Search** (`server-rs/src/services/bm25_memory.rs`): Sub-millisecond exact keyword and symbol search (< 1ms) via `GET /v1/memory/search/bm25`.

MCP bridge endpoints:

- `GET /v1/mcp/sse`
- `POST /v1/mcp/message`

### MCP Subprocess Security Hardening

The MCP server (`execution/tadpole_mcp_server.py`) enforces strict sandboxing limits on legacy execution tools:
1. **Shell-Free Execution**: Subprocesses are spawned without intermediate shells using argument-splitting (`asyncio.create_subprocess_exec`) to prevent Command Injection.
2. **Command Compliance**: Command patterns undergo Shell Scanner filtering, rejecting forbidden operators (`|`, `&`, `;`, `>` etc).
3. **Type-Safe Validation**: Parameter schemas are validated at the host boundary (zero-dependency custom validation).
4. **Limits & Timeouts**: Subprocesses are constrained by a hard 30s timeout and set resource limits (`RLIMIT_CPU=30s`, `RLIMIT_AS=256 MiB` where supported) to mitigate Denials of Service.

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

The endpoint `GET /v1/engine/health` returns detailed JSON telemetry for loopback monitoring (`server-rs/src/routes/health.rs`):
*   `status`: Global liveness indicator (`"tadpole_online_rust"` on loopback; `"ok"` on non-loopback `MinimalHealthResponse`).
*   `database`: `status` (`"healthy"` | `"failed"`), `pool_size`, `pool_idle`, `wal_size_mb`, and `busy_timeout_ms`.
*   `budget`: `total_spent_usd`, `daily_limit_usd`, `percent_used`, and `status` (`"ok"` | `"warning"` | `"critical"` | `"unmetered"`).
*   `swarm`: `connected_bunkers`, `total_agents`, `max_swarm_depth`, and `status` (`"healthy"` | `"empty"`).
*   `uptime_seconds`: Engine server runtime duration.
*   `GET /metrics`: Standard Prometheus metrics export (secured via `Authorization: Bearer <NEURAL_TOKEN>`).

### Alerting & Monitoring Configuration

*   **Prometheus Rules**: Configured in `monitoring/alerts.yml`. Triggers alerts on:
    *   `EngineDown` (critical): Scrape target offline for `>30s`.
    *   `EngineDegraded` (critical): `tadpole_health_state == 0` for `>1m`.
    *   `HighLatency` (warning): API or tool execution latency (p99) exceeds `500ms` for `>2m`.
    *   `BudgetExhausted` (critical): Swarm budget usage exceeds `80%` (or `oversight_cost_usd > 100`) for `>1m`.
    *   `SQLiteLocked` (warning): Database busy/lock wait time exceeds `10s` for `>30s`.
*   **Grafana Dashboard**: Configured in `monitoring/grafana/dashboards/tadpole_dashboard.json` for live status visualization.

### Telemetry Watchdog & Execution Resilience
*   **SpanWatchdog**: Sweeps active telemetry spans every 10s. Enforces provider-aware TTLs (60s cloud, 300s local Ollama). Automatically closes orphaned spans with synthetic `SPAN_TIMEOUT_REAPED` events to prevent leaked spans from degrading dashboard performance.
*   **Shared Tool Cache**: High-performance LRU cache for idempotent reads with monotonic `mtime` and byte size validation, invalidated upon mutating disk writes or shell commands.
*   **SSRF Protection**: Strict host and IP validation for outbound agent web requests, pinning resolved public IPs to eliminate DNS rebinding vectors.
*   **Mission Benchmark Harness**: Standardized 5-scenario evaluation suite (`tests/mission_bench/run.py` and `tests/mission_bench/judge.py`) testing tool caching, loop prevention, monologue compression, SSRF protection, and span leak recovery.

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
- **Selective Impacted Test Discovery**: Query the dependency graph to isolate all unit and integration test files affected by changes to a given file path (`GET /v1/intelligence/impacted-tests?file=<Path>`). Allows agents to run targeted test validations rather than expensive full-suite executions.
- **Token-Budgeted AST Symbol Context**: The `get_symbol_context` agent tool extracts exact function signatures, types, and struct definitions up to an operator-specified token budget ceiling, avoiding context overflows during large-file edits.
- **Targeted Test Runner Tool**: The `get_impacted_tests` agent tool allows execution runners to automatically retrieve affected test suites before and after mutating workspace files.
- **Topological Swarm Delegation Guard**: When saving or updating directives (`save_directive`), the engine executes a BFS cycle check across parent-child delegation dependencies, rejecting circular graphs (`400 Bad Request`) to guarantee deadlock-free task execution.
- **Interactive Swarm HUD & Quick Command Bar**: The **Swarm Visualizer** (`src/components/Swarm_Visualizer.tsx`) renders active mission topologies with dangling-edge elimination, and includes a direct Quick Command Bar for dispatching directives or triggering agent actions straight from the graph canvas.
- **Interactive Neural Map**: The **Neural Map** toggle in the Operations Dashboard compiles the codebase structure on the first click (triggering lazy backend evaluation with a `"Synthesizing Symbol Graph..."` overlay) and caches it in memory for instant subsequent loads.
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

Database initialization is handled by `server-rs/src/db/mod.rs`.

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

LanceDB semantic vector search and vector-backed agent memories depend on:

```bash
cargo run --manifest-path server-rs/Cargo.toml --features vector-memory
```

Without `vector-memory`, `/v1/agents/{id}/memories` (`GET`/`POST`/`DELETE`) and `GET /v1/search/memory` fall back transparently to SQLite `fallback_memories` (`200 OK`), while `/v1/memory/search/bm25` and `/v1/memory/search/hybrid` remain active via BM25 + TrustGraph + SQLite fallback. Semantic knowledge vector search (`GET /v1/knowledge/search`) is omitted at compile time (`404 Not Found`) unless compiled with `--features vector-memory`.

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

The engine uses configurable Tokio runtime thread and stack parameters. See [CAPACITY_PLANNING.md](./CAPACITY_PLANNING.md) for full capacity limits.

These parameters can be overridden via environment variables at startup (`server-rs/src/config.rs`):
*   `TOKIO_WORKER_THREADS`: Number of OS threads spawned for the multi-threaded event loop (default: available CPUs, min `4`).
*   `TOKIO_MAX_BLOCKING_THREADS`: Max threads allowed for blocking I/O tasks (default: `32`).
*   `TOKIO_THREAD_STACK_SIZE_MB`: Stack size (in megabytes, default: `4`) allocated for each worker thread.

## Rollback & Recovery Automation

Automated scripts for Windows/PowerShell recovery and deployment are located in the `scripts/` directory:

*   **Deployment**: Running `scripts/deploy-linuxlite.ps1` builds a release package, backs up the currently running engine binary under `deploy_backups/server-rs_<version>.exe`, and copies the new binary into place.
*   **Rollback**: If a deployment fails or causes degradation, run `scripts/rollback.ps1` to stop the engine, swap in the previous working binary from `deploy_backups/`, and verify migration schema backward compatibility.

## Local Debug Endpoints

The engine provides authenticated diagnostic endpoints under `/v1/system/debug` (`server-rs/src/routes/system.rs`, protected by `NEURAL_TOKEN` middleware):
*   `GET /v1/system/debug/services`: Returns subsystem initialization phases (`phases`) and `uptime_seconds`.
*   `GET /v1/system/debug/queues`: Returns `active_runners_count`, `ingestion_queue_depth`, `continuity_jobs_pending`, and `mcp_message_queue` to locate bottlenecks.

## Sovereign Engine Hardening

- **Self-Annealing Intelligence & Open-Model Recovery**: The `PolyglotParser` incorporates balanced-brace function call extraction for open models (`<function=...>`, hallucinated brackets/parentheses) and appends `[STRUCTURED_ERROR_FEEDBACK]` (`retryable`, `suggested_action`) on tool failures to guide single-turn refinement.
- **Pinned-Tail Compaction & Disk Spill**: `TurnCompactor` (`server-rs/src/agent/runner/turn_compactor.rs`) permanently pins the system prompt and the last 3 conversational turns uncompressed. Tool observations exceeding `24,000` characters (`DEFAULT_OVERFLOW_THRESHOLD_CHARS`, configurable via `TOOL_OBSERVATION_THRESHOLD_CHARS`) are offloaded to `.tmp/tool_overflow/tool_output_<timestamp>_<id>.txt` with a disk pointer URI and 300-character preview, while tool failure observations and warnings are preserved verbatim.
- **Lock-Free Atomic Execution Telemetry**: `ExecutionMetricsRollup` maintains mission turn counts, token throughput, USD cost, and tool success/failure tallies using lock-free `AtomicU64` primitives, warning operators if summarizations loop ($\ge 3$).
- **Concurrent Workspace Edit Leases**: `ConflictManager` issues 30-second TTL leases on workspace file paths to prevent multi-agent race conditions during parallel modifications.
- **Panic Remediation**: Critical paths in the bridge, parser, and security modules use safe error propagation (via `Result` and `AppError`) rather than non-recoverable panics.
- **Non-Blocking Orchestration**: All filesystem I/O in the MCP execution and Memory Palace rehydration modules is migrated to `tokio::fs` to prevent event-loop stalling.
- **Non-Blocking Graph Rebuilding**: Decoupling the filesystem walking and AST parsing tasks from the main thread lock prevents Axum router request timeout cascades during compilation sweeps.
- **Database Persistence & Reconciliation Governance**:
  - Gated reconciliation: `ALLOW_MIGRATION_RECONCILE=true` must be explicitly set to reconcile legacy database schemas before marking newer migrations.
  - Native backup utilities: Shell scripts `scripts/backup_db.ps1` (PowerShell) and `scripts/backup_db.sh` (Bash) perform zero-downtime SQLite online backups with SHA-256 integrity verification.
  - Persistence validation: `python scripts/verify_persistence.py` independently verifies table presence, row counts, and migration status.
## Tracing & Telemetry Stream Operations

The Tadpole OS telemetric bridge streams live OpenTelemetry-compatible spans over WebSockets (`/v1/engine/ws`) and internal channels to power the Neural Waterfall execution timeline (`src/components/Neural_Waterfall.tsx`):
- **Live Distributed Spans**: Root `AgentExecution` spans and child `ToolExecution` spans capture `trace_id`, `parent_id`, `agent_id`, and timing metrics.
- **Dynamic Field Recording**: The `TelemetryLayer::on_record` hook updates dynamic span metadata (including W3C `traceparent` bindings and `trace_id` assignment) to prevent disconnected tree hierarchies.
- **Detached Telemetry Streams**: Detached windows (`/detached-view?type=trace-stream`) automatically synchronize state via cross-tab `BroadcastChannel('trace_store_sync')` and establish dedicated WebSocket streams.
- **Diagnostic Telemetry**: When the engine is idle, operators can dispatch diagnostic telemetry directly from the standby interface to verify end-to-end timeline rendering, ticker intervals, and zoom multiplier controls.

## Metrics & Health Diagnostics

The engine exposes health and runtime metrics endpoints:
- `GET /health` and `GET /v1/engine/health`:
  - **Loopback Invariant**: Local loopback callers (`127.0.0.1`, `::1`) receive comprehensive diagnostic telemetry including database connection pool size, WAL sizing, daily LLM budget usage, active swarm counts, and system uptime.
  - **Remote Peer Redaction**: Non-loopback peers receive a redacted `MinimalHealthResponse` (`{"status":"ok","heartbeat":...}`) omitting internal database metrics, active agent allocations, and memory statistics.
  - **Cache Isolation**: Remote caller redaction is evaluated prior to cache lookups to eliminate cached telemetry disclosure across network trust boundaries.
- `GET /metrics`:
  - Returns Prometheus-compatible operational counters, histogram timings, and resource statistics.

## Model Inventory Quota & Expenditure Operations

The Model Manager provides comprehensive rate limiting and fiscal controls for all connected AI backends:

- **Configuring Context Windows**:
  - In the Model Manager inventory dashboard (`/models`) or via the Add Node modal, operators configure the active `context_window` capacity (e.g. 128,000 for Gemma/Llama, 1,000,000 for Gemini/Claude).
  - The runtime engine utilizes this context boundary to govern token budget allocation in the Adaptive Context Slicer and prevent context overflow panics.
- **Configuring Token Pricing Rates**:
  - Operators specify custom token rates per model:
    - **Token Input (`input_cost_per_m`)**: USD rate per million prompt/input tokens.
    - **Token Output (`output_cost_per_m`)**: USD rate per million completion/output tokens.
  - Setting both to `0.00` correctly accounts for free local models (Ollama/Local nodes).
  - Custom rates override default engine lookup tables, allowing exact tracking against custom negotiated provider contracts.
- **Live Rate Limit Enforcement**:
  - The engine enforces two tiers of quota limits alongside cost calculation:
    - **Per-Minute Limits**: Requests per minute (`RPM`) and tokens per minute (`TPM`).
    - **Daily Quotas**: Requests per day (`RPD`) and tokens per day (`TPD`).
  - Interactive tooltips across the UI display quota formulas and explanations for operator clarity.

## Swarm Evaluation Benchmark & Context Overflow Runbook

### Running Ground-Truth Architectural Evaluations
Operators can execute the ground-truth benchmark suite:
```bash
npm run test:evals
# Or directly via Python:
python evals/run.py
```
This suite verifies 20 architectural invariants with zero network drift:
1. Tool manifest parameter schemas
2. Loop guard recursion blocking
3. Pre-turn financial cap checks
4. Deterministic artifact existence on mission completion
5. Context window overflow containment
6. Ephemeral prompt caching headers
7. Structured role mapping (`ChatMessage`)
8. Heartbeat SQLite write throttling
9. Bulky observation offloading to `.tmp/tool_overflow/`
10. Sentinel gate narrative leaks
11. COO hierarchy redirects to Alpha Node
12. Parallel tool capping ($\le 10$)
13. Dynamic memory throttling ($\ge 90\%$)
14. Cached tool replay via parameter hashing
15. Turn-1 adversarial jailbreak detection
16. Script-aware non-Latin balance profiling
17. Provider failover dispatch
18. Error classification (`AppError::ContextOverflow` as Repairable)
19. Merkle audit trail non-repudiation
20. Silent mission completion closure synthesis

### Handling Context Overflow (`AppError::ContextOverflow`)
When an agent session accumulates tokens exceeding the configured `context_window`:
- The engine returns `AppError::ContextOverflow` mapped to HTTP 413 Payload Too Large.
- The error is categorized as `ErrorClass::Repairable` and `ErrorScope::Turn`.
- The turn compactor prunes earlier observations and stores full outputs in `.tmp/tool_overflow/` with restorable URI pointers.
- The mission remains active; if the context remains above capacity, operators can adjust the model's `context_window` in `/models` or compact the mission transcript.

### Handling Tool Repetition Alerts (`[TOOL_WARNING]` & `RecursionBlocked`)
When an agent repeats identical tool calls:
- At 3 consecutive executions with identical parameter hashes, a `[TOOL_WARNING]` is injected into the observation stream.
- At 5 repetitions, the mission is halted with `RunnerError::RecursionBlocked`.
- Operators can review the mission transcript in `/missions`, identify the missing dependency or parameter mismatch, and issue a clarifying directive from the UI.

### Tiered Operator Authentication (`OperatorScope`)
The engine enforces server-authoritative token scopes in `server-rs/src/middleware/auth.rs`:
- **`NEURAL_TOKEN_ADMIN` (`Admin`)**: Required when configured for:
  - Host & template control: `/v1/engine/{deploy,pre-pr,kill,shutdown}`, `/v1/engine/templates/install`, and mutating `/v1/engine/templates/*`.
  - Skill/script mutations & execution: mutating `/v1/skills/{scripts,workflows,hooks}/*`, `POST /v1/skills/{register,promote}`, `POST /v1/skills/proposals/{id}/resolve`, and `POST /v1/skills/mcp-tools/{name}/execute`.
  - Governance, provider, continuity & MCP mutations: mutating `/v1/oversight/settings`, `/v1/oversight/security/*`, `/v1/model-manager/*`, `/v1/api/pull`, `/v1/governance/blueprints*`, `/v1/continuity/*`, `/v1/mcp/message`, and `POST /v1/agents/{id}/skills/{skill_id}/approve`.
- **`NEURAL_TOKEN` (`Operator`, or `Admin` when `NEURAL_TOKEN_ADMIN` is unset)**: Standard swarm, agent, mission, oversight decisions, and WebSocket/SSE stream operations; preserves zero-breaking local development when only `NEURAL_TOKEN` is set.
- **`NEURAL_TOKEN_READONLY` (`ReadOnly`)**: Restricted to safe `GET`/`HEAD`/`OPTIONS` observability endpoints; mutating requests, WebSocket upgrades, and `GET /v1/mcp/sse` return HTTP 403 (`Insufficient Scope`).

### Day-1 Bootstrap & Progressive Cockpit Operations
- **Automated Bootstrap (`npm run init`)**: Runs `execution/tadpole_init.py` to generate a 64-hex `NEURAL_TOKEN` (plus optional Admin/ReadOnly tokens), detect Ollama/Docker availability, and write a hardened `.env` without echoing raw credentials. `start_AA_tadpole.bat` invokes it automatically when `.env` is missing.
- **First-Run Setup Wizard**: Launches automatically for fresh installs (and anytime via `Cmd/Ctrl+K` → *Open Day-1 Overlord Setup Wizard* or the header sparkle button) to verify engine handshake, apply a Model Tier Preset (`Local Sovereign`, `Hybrid Overlord`, `Full Cloud Max`), configure guardrails, and run a live pre-flight readiness check.
- **Focus vs Expert Cockpit (`Ctrl+Shift+E`)**: Fresh installs open in **Focus Mode** (6 core views: Operations, Missions, Oversight, Agents, Models, Settings). Existing upgraded users stay in **Expert Mode** (all 18 grouped views). Toggle anytime via the sidebar switch, header pill, or Command Palette.
- **Keyboard-First Oversight Triage (`/oversight`)**: Navigate pending approvals with `j`/`k` (or arrow keys), press `a` to approve, `r` to reject, `g` to add inline guidance, or `Shift+A` to batch-approve **low-risk read-only actions only** (high- and medium-risk mutations always require explicit individual approval).
- **Pre-Flight Readiness Check (`POST /v1/benchmarks/readiness`)**: Run from `/models`, `/benchmarks`, or the Setup Wizard to score SQLite integrity, active model tier, runtime security guardrails, and fleet governance (`0–100`) with concrete remediation hints.

[//]: # (Metadata: [OPERATIONS_MANUAL])

