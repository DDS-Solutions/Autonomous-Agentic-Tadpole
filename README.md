> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[README]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Autonomous Agentic Tadpole OS runtime, dashboard, Rust engine, and Python execution layer.
>
> ### Debugging & Observability
> Traceability via `execution/parity_guard.py`.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.
<div align="center">

# Autonomous Agentic Tadpole

**A local-first operating system for autonomous agent swarms, governed execution, model operations, and live mission telemetry.**

[![Rust](https://img.shields.io/badge/Rust-2021-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/React-19-149ECA?style=for-the-badge&logo=react&logoColor=white)](https://react.dev/)
[![TypeScript](https://img.shields.io/badge/TypeScript-6-3178C6?style=for-the-badge&logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![Vite](https://img.shields.io/badge/Vite-8-646CFF?style=for-the-badge&logo=vite&logoColor=white)](https://vite.dev/)
[![SQLite](https://img.shields.io/badge/SQLite-SQLx-003B57?style=for-the-badge&logo=sqlite&logoColor=white)](https://www.sqlite.org/)

[Quick Start](#quick-start) • [Architecture](#architecture) • [API](#api) • [Configuration](#configuration) • [Verification](#verification) • [📚 Wiki](wiki/Home.md)

</div>

## Why This Exists

                ⚠️ **Warning: With great power comes great responsibility. Sandbox Use Only** ⚠️

Most agent frameworks stop at prompt orchestration. Autonomous Agentic Tadpole goes further: it combines a Rust control plane, a production-grade React operations console, SQLite-backed persistence, governance gates, skill execution, MCP tooling, model-provider management, and live telemetry into one local-first system.

The result is a desktop-ready agent operations platform that can run missions, manage agent state, inspect model providers, schedule continuity jobs, surface security posture, and keep a durable audit trail without depending on a hosted control plane.

## 🏆 Framework Comparison Matrix

| Dimension | Marketplace Frameworks (CrewAI, AutoGen, LangChain) | Autonomous Agentic Tadpole | Enterprise Impact |
|---|---|---|---|
| **Core Kernel** | Python / Node.js (Interpreted, GIL locks) | **Compiled Rust (`server-rs`) + Axum + Tokio** | **10x – 100x Throughput & Sub-ms Latency** |
| **Token Optimization** | Naive string counts / uncached BPE | **Model-Aware `TokenizerService` (< 1µs DashMap LRU)** | **Sub-Microsecond Zero-Allocation Token Tracking** |
| **Context Compression** | Truncation or basic sliding window | **2-Tier `ContextManager` (Heuristic + LLM Summarizer)** | **30% – 50% Token Cost Reduction** |
| **RAG Architecture** | Single Naive Vector DB (Chroma/Pinecone) | **Hybrid RAG Triad (Vector + TrustGraph + BM25 Lexical)** | **Sub-ms Exact Code Symbol & Multi-Hop RAG** |
| **Financial Safety** | Loose per-call limits / unconstrained loops | **A2E-01 2PC Ledger + 24h Rolling Cap + Lock Awareness** | **Zero Risk of Runaway API Overruns** |
| **Self-Healing** | Uncaught exceptions / endless loops | **`tool_loop_guard.py` + Boot DB Reconciler + Annealing** | **Deterministic Circuit Breakers** |

## What It Does

- **Runs a Rust agent engine** with Axum, Tokio, background actors, lifecycle workers, and protected `/v1` APIs.
- **Ships a real operations dashboard** built with React 19, TypeScript, Vite, Tailwind CSS, Zustand, and React Router.
- **Persists operational state locally** with SQLite migrations through SQLx.
- **Executes skills and tools** through Python scripts, JSON manifests, modular `execution/core/` skills, and MCP endpoints.
- **Manages providers and models** across local and cloud backends, with Ollama support and provider-key configuration.
- **Streams telemetry live** through WebSocket routes, heartbeat events, system logs, and dashboard health panels.
- **Enforces governance and security controls** through bearer-token auth, request middleware, shell scanning, audit trails, quotas, oversight queues, and privacy-mode behavior.
- **Visualizes codebase interdependencies** with an interactive 2D Force-Directed Knowledge Graph and down-stream blast-radius impact analysis.
- **Institutional Knowledge Store (IKS)**: Provides cross-cluster, persistent semantic memory backed by local SQLite databases and optional LanceDB vectors, complete with TTL evictions and time-aware confidence decay.
- **Unified Graph Analysis**: Implements off-thread, lock-protected codebase dependency tracking with O(1) reverse-obfuscated path lookup for secure blast-radius calculations.
- **Modular Sovereign UI Layouts**: Refactored the monolithic chat console into highly cohesive frontend sub-components and specialized React hooks.
- **Automated Self-Healing State Reversion**: Integrates benchmark telemetry with sovereign time-travel state rollbacks, autonomously reverting missions to the last known green state on regression.
- **Hardened Sandboxed Execution**: Spawns dynamic agent-generated skills inside secure WebAssembly (Wasmtime) or Docker micro-containers with strict CPU/memory limits and filesystem token isolation.
- **Tiered Cognitive Memory consolidation**: Consolidates episodic short-term memories into semantic long-term knowledge via a background summarization loop, with a fully database-backed SQLite fallback when vector features are disabled.
- **Inter-Agent Communication Protocol (IACP)**: Enables agents to negotiate tasks, delegate sub-tasks, peer-review skills, and transfer operational micro-budgets.
- **System Event Bus**: Combines Tokio channels with system triggers (file modifications, system load, webhooks) to spawn Continuity Jobs proactively.
- **Keeps optional heavy features explicit**: vector memory and neural audio are Cargo features, disabled by default for lightweight local builds.

## System At A Glance

```mermaid
flowchart LR
  Dashboard["React + TypeScript Dashboard"] --> API["/v1 REST + WebSocket API"]
  API --> Engine["Rust Axum Engine"]
  Engine --> State["AppState Hubs"]
  State --> DB["SQLite + SQLx Migrations"]
  State --> Actors["Audit, Memory, Security, Skill Actors"]
  Engine --> Workers["Telemetry, Scheduler, Discovery, Reaper"]
  Engine --> MCP["MCP Host + Skill Registry"]
  MCP --> Execution["Python Scripts + JSON Skill Manifests"]
  Dashboard --> BrowserRuntime["Browser Inference + VRAM Monitor"]
```

## Quick Start

Prerequisites:

- Node.js and npm compatible with the checked-in `package-lock.json`.
- Rust toolchain for `server-rs`.
- Python 3 for scripts in `execution/`.
- Optional: Ollama or cloud provider API keys for model execution.

Install dependencies:

```bash
npm install
```

Create local environment config:

```bash
cp .env.example .env
```

Set one engine token in `.env`:

```ini
NEURAL_TOKEN=your-secret-token
```

Start the Rust engine:

```bash
npm run engine
```

Start the dashboard in a second terminal:

```bash
npm run dev
```

Open:

- Dashboard: `http://localhost:5173`
- Engine health: `http://127.0.0.1:8000/v1/engine/health`

On Windows, these helpers wrap the same flow:

- `start_AA_tadpole.bat`
- `start_backend.bat`
- `start_frontend.bat`
- `stop_AAtadpole.bat`

## Architecture

Autonomous Agentic Tadpole has three practical runtime layers.

| Layer | Code | Responsibility |
| --- | --- | --- |
| Interface | `src/` | Dashboard shell, pages, stores, services, browser monitoring, provider sync, detached views |
| Engine | `server-rs/src/` | Axum routes, AppState, actors, middleware, telemetry, agent runner, security, startup workers |
| Execution | `execution/` | Python tools, MCP server, JSON skill definitions, verification scripts, modular skill framework |

The engine boot path starts in `server-rs/src/main.rs`, initializes environment and tracing, creates `AppState`, starts background workers, spawns system actors, launches the orchestrator, and binds Axum on `127.0.0.1:8000` unless configured otherwise.

`server-rs/src/router.rs` assembles all `/v1` API routes. Public engine health and WebSocket routes remain open; management routes require `Authorization: Bearer <NEURAL_TOKEN>`. When `dist/` exists, the same Rust process serves the built React app with SPA fallback.

## Repository Map

| Path | Purpose |
| --- | --- |
| `src/` | React dashboard, stores, hooks, components, services, contracts, pages, tests |
| `server-rs/src/` | Rust API, agent runner, AppState hubs, actors, middleware, routes, telemetry, security |
| `server-rs/migrations/` | SQLite schema migrations |
| `execution/` | Python execution layer, MCP host, skill manifests, verification and audit utilities |
| `execution/core/` | Modular skill framework foundation |
| `directives/` | Governance, identity, orchestration, and provider operating instructions |
| `docs/` | Architecture, operations, API reference, OpenAPI, and security docs |
| `data/` | Local runtime data, including the default `tadpole.db` |
| `dist/` | Production dashboard build served by the Rust engine |
| `tests/` | Shared frontend test setup and e2e support |

## Dashboard Surface

Navigation is defined in `src/constants/routes.ts`.

| URL | View |
| --- | --- |
| `/dashboard` | Operations dashboard |
| `/org-chart` | Hierarchy |
| `/standups` | Standups |
| `/workspaces` | Workspaces |
| `/missions` | Missions |
| `/models` | Model manager |
| `/agents` | Agent manager |
| `/engine` | Engine dashboard |
| `/oversight` | Oversight |
| `/skills` | Skills |
| `/benchmarks` | Benchmarks |
| `/scheduled-jobs` | Jobs |
| `/infra/model-store` | Intelligence store |
| `/docs` | Documentation |
| `/settings` | Settings |
| `/store` | Template store |
| `/security` | Security |
| `/governance` | Governance |

Detached windows are available at `/detached-view`, `/detached/swarm-pulse`, and `/detached/chat`.

## API

The Rust engine binds to `127.0.0.1:8000` by default. All application APIs are nested under `/v1`.

Public routes:

| Method | Route | Purpose |
| --- | --- | --- |
| `GET` | `/v1/engine/health` | Engine health check |
| `GET` | `/v1/engine/ws` | Engine WebSocket stream |
| `GET` | `/v1/engine/live-voice` | Live voice WebSocket stream |

Protected route groups:

| Prefix | Purpose |
| --- | --- |
| `/v1/agents` | Agent CRUD, graph, tasks, pause/resume, memory |
| `/v1/oversight` | Decisions, ledger, quotas, audit trail, health, policy |
| `/v1/infra` | Node discovery and infrastructure nodes |
| `/v1/model-manager` | Providers, models, catalog, pulls, provider tests |
| `/v1/skills` | Skill manifests, MCP tools, imports, promotion, scripts, workflows, hooks |
| `/v1/benchmarks` | Benchmark definitions, runs, and history |
| `/v1/continuity` | Scheduled jobs and workflows |
| `/v1/docs` | Knowledge docs and operations manual |
| `/v1/system` | Compute profile and system introspection |
| `/v1/governance` | Blueprints and sovereign manifest |
| `/v1/sovereign` | Mission session tree and branch state |
| `/v1/intelligence` | High-fidelity symbol graph mapping and dependent blast-radius analytics |
| `/v1/search/memory` | Global memory search |
| `/v1/env-schema` | Runtime environment schema |
| `/v1/engine/*` | Deploy, kill, shutdown, transcribe, speak, template install |
| `/v1/mcp/*` | MCP SSE and message bridge |

Protected routes require:

```http
Authorization: Bearer <NEURAL_TOKEN>
```

Memory endpoints return `501 Not Implemented` unless the Rust `vector-memory` feature is enabled.

## Scripts

| Command | Purpose |
| --- | --- |
| `npm run dev` | Start Vite on port 5173 |
| `npm run engine` | Run the Rust engine via Cargo |
| `npm run build` | Type-check and build the frontend |
| `npm run lint` | Run ESLint |
| `npm run test` | Run Vitest |
| `npm run test:coverage` | Run Vitest with coverage |
| `npm run preview` | Preview the Vite build |
| `npm run docs:api` | Regenerate `docs/openapi.yaml` and `docs/API_REFERENCE.md` from `server-rs/src/router.rs` |
| `npm run docs:parity` | Run documentation/API/version parity checks |
| `npm run tauri:dev` | Start Tauri dev mode |
| `npm run tauri:build` | Build Tauri app |
| `npm run docs:dev` | Start VitePress docs |
| `npm run docs:build` | Build VitePress docs |
| `npm run docs:preview` | Preview built docs |
| `npm run version:sync` | Sync `version.json` into manifests and docs |

## Configuration

Common environment variables:

| Variable | Default | Purpose |
| --- | --- | --- |
| `NEURAL_TOKEN` | Required | Bearer token for protected API routes |
| `NEURAL_TOKEN_OLD` | Optional | Previous token kept valid during zero-downtime rotation |
| `NEURAL_TOKEN_NEW` | Optional | Replacement token staged during rotation grace window |
| `NEURAL_ENGINE_ACCESS_TOKEN` | Required alternative | Alternate auth token accepted by the engine |
| `ALLOWED_ORIGINS` | Local dev origins | Comma-separated origin allow-list; use `*` only for wildcard troubleshooting with credentials disabled |
| `PORT` | `8000` | Engine port |
| `BIND_ADDRESS` | `127.0.0.1` | Engine bind address |
| `STATIC_DIR` | `dist` | Production static asset directory |
| `WORKSPACE_ROOT` | Current directory | Base path for data, scripts, skills, and panic logs |
| `DATABASE_URL` | `sqlite:<workspace>/data/tadpole.db` | SQLx database URL |
| `RESOURCE_ROOT` | Optional | Static/model resource root |
| `HEARTBEAT_INTERVAL_SECS` | `3` | Engine health heartbeat cadence |
| `SKIP_DB_SEED` | `false` | Skip baseline database seeding |
| `PRIVACY_MODE` | `false` | Restrict execution toward local-only providers |
| `TADPOLE_ALLOW_LOCAL_HTTP` | unset | Allows insecure local HTTP model-provider calls when set |
| `TADPOLE_NULL_PROVIDERS` | unset | Forces null providers for tests and integration runs |
| `DISABLE_TELEMETRY` | `false` | Disables OpenTelemetry stdout exporter when `true` |

Provider keys supported by `.env.example`:

- `OPENAI_API_KEY`
- `ANTHROPIC_API_KEY`
- `GOOGLE_API_KEY`
- `GROQ_API_KEY`
- `DEEPSEEK_API_KEY`
- `REPLICATE_API_KEY`

## Production Build

Build frontend assets:

```bash
npm run build
```

Start the engine:

```bash
npm run engine
```

If `dist/` exists, the Rust router serves it automatically and falls back to `dist/index.html` for dashboard routes.

Override the static asset directory:

```ini
STATIC_DIR=dist
```

## Optional Rust Features

Default Cargo features are intentionally empty to keep local Windows and lightweight machines from pulling in heavy native dependencies.

Enable vector memory:

```bash
cargo run --manifest-path server-rs/Cargo.toml --features vector-memory
```

Enable neural audio:

```bash
cargo run --manifest-path server-rs/Cargo.toml --features neural-audio
```

Enable both:

```bash
cargo run --manifest-path server-rs/Cargo.toml --features vector-memory,neural-audio
```

## Data And Persistence

- Default database: `data/tadpole.db`.
- Rust migrations: `server-rs/migrations/`.
- Agent data is loaded from SQLite and related registry files under the workspace data directory.
- Provider and model registries are persisted on graceful shutdown by `AppState`.
- Panic diagnostics are written to `sidecar_panic.log` under `WORKSPACE_ROOT` when possible.

## Verification

Frontend tests:

```bash
npm run test
```

Frontend type-check and production build:

```bash
npm run build
```

Rust tests:

```bash
cargo test --manifest-path server-rs/Cargo.toml
```

Python-side verification utilities:

- `execution/verify_all.py`
- `execution/verify_ai_context.py`
- `execution/parity_guard.py`
- `execution/sovereign_audit.py`

## Documentation

| Document | Path |
| --- | --- |
| Architecture | `docs/ARCHITECTURE.md` |
| Operations | `docs/OPERATIONS_MANUAL.md` |
| API reference | `docs/API_REFERENCE.md` |
| OpenAPI | `docs/openapi.yaml` |
| Security | `docs/SECURITY.md` |
| System map | `SYSTEM_MAP.md` |

## 📚 Wiki

The `wiki/` directory contains a comprehensive knowledge base designed for developers and operators:

| Page | Description |
|------|-------------|
| [🏠 Home](wiki/Home.md) | Overview, system diagram, quick-start TL;DR |
| [🚀 Getting Started](wiki/Getting-Started.md) | Prerequisites, install, first run, Windows shortcuts |
| [🏗️ Architecture](wiki/Architecture.md) | Engine boot, AppState hubs, router stack, agent pipeline, repo map |
| [📡 API Reference](wiki/API-Reference.md) | All 50+ REST/WebSocket endpoints with schemas |
| [⚙️ Configuration](wiki/Configuration.md) | Every env var, Cargo feature flag, and provider key |
| [🔐 Security Model](wiki/Security-Model.md) | Auth, token rotation, MCP sandboxing, audit trail, budget guard |
| [🏥 Operations Manual](wiki/Operations-Manual.md) | Backup/restore, token rotation runbook, Grafana, GDPR, shutdown |
| [🧪 Testing Guide](wiki/Testing-Guide.md) | Rust/Python/React test suites, CI, common pitfalls |
| [💻 Development Guide](wiki/Development-Guide.md) | Local dev setup, conventions, adding endpoints, migrations |
| [🔧 Troubleshooting](wiki/Troubleshooting.md) | 12 common issues with exact step-by-step fixes |

## Recent Upgrades (v1.1.58) — Operational Hardening

### 🔒 Security & Credential Management
- **Zero-Downtime Token Rotation**: Auth middleware now supports dual-token validation via `NEURAL_TOKEN_OLD` / `NEURAL_TOKEN_NEW` env vars, enabling safe production token rotation with a configurable grace window. Documented in `docs/SECURITY.md`.
- **MCP Subprocess Sandboxing**: Hardened `execution/tadpole_mcp_server.py` with `asyncio.create_subprocess_exec` (no `shell=True`), `shlex`-based command splitting, a skill allowlist, JSON Schema input validation, hard 30-second execution timeout, and `resource.setrlimit` CPU/memory constraints on Linux/Docker.

### 🧠 Graph Engine & Oversight Hardening
- **Lock-Free Concurrent Graph Engine**: Replaced `parking_lot::RwLock` with lock-free concurrent access via `arc_swap::ArcSwap` in `ResourceHub` to guarantee non-blocking reads during background graph rebuilds.
- **Path Traversal Hardening & Error Segregation**: Tightened `/v1/intelligence/blast-radius` and `/v1/intelligence/resolve` to validate input paths unconditionally. Missing paths return `404 Not Found` (`IntelPathUnknown`), whereas path boundary violations (like directory traversal attempts) return `403 Forbidden` (`Forbidden`).
- **Oversight Ledger Pagination Fix**: Resolved a critical double-pagination bug in `/v1/oversight/ledger` and `/v1/oversight/security/audit-trail` that capped the approved card count in the dashboard at 100 entries.
- **SHA-256 Hashing for Code Review Graph**: Replaced MD5 hashing with SHA-256 in `graph_store::scan_files::file_hash` and bumped the schema version to 10 with a dual-write `file_hash_sha256` migration window.
- **Git Resolution at Boot**: Cached `git` executable absolute path at boot via `which` to ensure hermetic and safe execution of sub-processes.
- **Axum Request Limit**: Added a global 16 KiB default request limit in router to prevent Denial-of-Service attacks.
- **Robustness Cap Warning and Truncation**: Demoted `MAX_NODES` and `MAX_EDGES` exceeding checks from fatal errors to early-termination warning logs, leaving the previous graph intact, and resolved workspace-wide file bloat via `MAX_DISCOVERED_FILES` truncation.

### 🗄️ Database Reliability
- **Hot SQLite Backup & Restore**: Added `execution/backup_sqlite.py` and `execution/restore_sqlite.py` with WAL-mode-safe `VACUUM INTO` hot backups, SHA-256 integrity hashing, and `PRAGMA integrity_check` verification.
- **DB helper API in Rust**: Added `run_backup()` and `check_integrity()` helpers to `server-rs/src/db.rs` for programmatic backup orchestration.

### 📊 Observability & Health
- **Extended Health Endpoint**: `GET /v1/engine/health` now returns structured `database` (WAL size, pool stats), `budget` (spend, limit), `swarm` (agent count, status), and `uptime_seconds` fields.
- **Monitoring Dashboards**: Added `monitoring/alerts.yml` (Prometheus alert rules) and `monitoring/grafana/dashboards/tadpole_dashboard.json` (pre-built Grafana dashboard).

### 🧪 Test Coverage Expansion
- **Rust Integration Tests** (`server-rs/src/routes/`):
  - `backup_restore_tests.rs` — hot backup + `PRAGMA integrity_check`
  - `token_rotation_tests.rs` — dual-token grace window auth
  - `health_endpoint_tests.rs` — extended health schema validation
  - `shutdown_orchestrator_tests.rs` — graceful shutdown + emergency kill switch
- **Python Unit Tests** (`tests/unit/`):
  - `test_backup_integrity.py`, `test_token_rotation.py`, `test_mcp_sandbox.py`
  - `test_snapshot_state.py`, `test_verify_ai_context_fix.py`

### 📖 Documentation
- **`docs/OPERATIONS_MANUAL.md`**: Added Token Rotation Runbook, Database Backup/Restore procedure, Sandbox Snapshotting guide, MCP Security Hardening details, Prometheus/Grafana alerting setup, and GDPR cascading deletion runbook.
- **`docs/SECURITY.md`**: Updated with dual-token rotation mechanism, MCP subprocess boundary hardening, and RCE mitigation details.

### 🔧 Infrastructure
- **CI Pipeline Fix**: Corrected missing `uses:` keyword in `.github/workflows/ci.yml` that caused workflow parse failures.
- **Compiler Warning Cleanup**: Eliminated all 70 Rust compiler warnings — unused imports (feature-gated re-exports, test fixtures), unused variables prefixed with `_`, and dead code in scaffolded actor subsystems suppressed with targeted `#[allow(...)]` attributes.

---

## Previous Upgrades (v1.1.57)

- **Institutional Knowledge Store (IKS)**: Implemented cross-restart, cross-cluster durable memory storing agent playbooks, decision history, and curated facts. Uses SQLite metadata indexing + LanceDB k-NN semantic search, complete with a time-aware confidence decay rate (0.01 per day) and automatic TTL eviction sweeps.
- **Path Traversal Hardened Graph Engine**: Re-engineered `/v1/intelligence/blast-radius` and `/v1/intelligence/resolve` to look up paths via an obfuscated-to-real-path registry, enforcing strict directory checks against workspace boundaries to prevent traversal attacks.
- **Sovereign Chat UI Modularization**: Monolithic chat layouts have been split into standalone, testable React components under `src/components/chat/` and custom hooks (`use_chat_dispatch`, `use_chat_voice`).
- **Observability Tag Alignment**: Unified telemetry tags (`[okf_gate]`, `[graph]`, `[IKS]`, `[intelligence]`) across comments and tracing logs to ensure 100% synchronization and prevent documentation drift.

## Engineering Notes

- The default engine token is intentionally required outside tests.
- The frontend stores the API token locally through the settings flow and sends it as a bearer token to protected endpoints.
- Vite runs on port `5173`; the Rust engine runs on port `8000`.
- The Rust engine can serve the built dashboard directly from `dist/`, so production mode does not require a separate Vite server.
- `vector-memory` and `neural-audio` are opt-in Cargo features because they can introduce heavier native dependencies.
- SQLite backups use `VACUUM INTO` and must target a file path (not in-memory). Run `execution/backup_sqlite.py` before any major migration or deployment.
- Token rotation uses a two-token grace window: set `NEURAL_TOKEN_OLD` to the current token and `NEURAL_TOKEN_NEW` to the replacement before restarting the engine.





[//]: # (Metadata: [README])
