> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[SYSTEM_MAP]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[SYSTEM_MAP]` in audit logs.
>
> ### AI Assist Note
> Tadpole OS system architecture map.
>
> ### Debugging & Observability
> Traceability via `execution/parity_guard.py`.

# Tadpole OS System Map

**Version**: 1.1.57
**Runtime shape**: React dashboard + Rust Axum engine + SQLite persistence + Python execution layer.

This map reflects the current code layout and should be used as the first orientation document when reviewing the system.

## Runtime Entry Points

| Concern | Path | Language | Notes |
| --- | --- | --- | --- |
| Dashboard boot | `src/main.tsx`, `src/App.tsx` | TypeScript/React | Mounts the dashboard, route shell, provider sync, visual monitor, VRAM monitor, and agent hydration. |
| Dashboard routes | `src/constants/routes.ts` | TypeScript | Defines the main UI navigation registry. |
| Engine boot | `server-rs/src/main.rs` | Rust | Loads env, initializes tracing, builds `AppState`, starts workers, actors, orchestrator, and Axum. |
| API router | `server-rs/src/router.rs` | Rust | Nests routes under `/v1`, applies middleware, serves `dist/` when present. |
| Global state | `server-rs/src/state/mod.rs` | Rust | Owns AppState hubs, registries, DB pool, actor registry, and boot gate. |
| Startup workers | `server-rs/src/startup.rs` | Rust | Starts CodeGraph warmup, heartbeat, scheduler, reaper, ingestion, discovery, privacy guard, telemetry aggregation, and pulse loop. |
| Python MCP host | `execution/tadpole_mcp_server.py` | Python | Runs JSON-defined tools and modular skill execution. |
| CodeGraph API | `server-rs/src/routes/intelligence.rs` | Rust | Exposes codebase-wide symbol graph synthesis and dependent blast-radius calculations. |

## Major Subsystems

| Subsystem | Primary paths | Purpose |
| --- | --- | --- |
| Frontend shell | `src/layouts/`, `src/components/`, `src/pages/` | Dashboard navigation, operations views, detached windows, visualizations, and forms. |
| Stores and hooks | `src/stores/`, `src/hooks/` | Client state, dashboard data, logs, engine status, agents, models, settings, skills, and telemetry. |
| Frontend services | `src/services/` | API clients, sockets, model/provider services, browser inference, VRAM monitoring, governance, and mission services. |
| Agent engine | `server-rs/src/agent/` | Providers, mission runner, registry, skills, MCP bridge, continuity, tools, hooks, and agent persistence. |
| Routes | `server-rs/src/routes/` | REST and WebSocket handlers for agents, oversight, model manager, skills, docs, governance, system, and engine control. |
| Code Intelligence | `server-rs/src/intelligence/`, `src/components/intelligence/` | Codebase-wide symbol mapping, directed force-graph visualization, and downstream blast-radius analysis. |
| State hubs | `server-rs/src/state/hubs/` | Communication, governance, registry, resources, and security hub separation. |
| Actors | `server-rs/src/system/actors/` | Audit, memory, security, and skill actor infrastructure. |
| Security | `server-rs/src/security/`, `server-rs/src/middleware/`, `server-rs/src/secret_redactor.rs` | Auth, rate limiting, security headers, scanner, permissions, privacy, audit, budget guard, and redaction. |
| Persistence | `server-rs/src/db.rs`, `server-rs/migrations/`, `data/` | SQLite initialization, migrations, local runtime data, and registry persistence. |
| Execution tools | `execution/`, `execution/core/`, `execution/skills/` | JSON tool manifests, Python scripts, BaseSkill framework, and skill templates. |
| Documentation | `README.md`, `docs/`, `SYSTEM_MAP.md` | Public orientation, architecture, operations, security, API reference, and OpenAPI. |

## API Boundary

Public routes:

- `GET /v1/engine/health`
- `GET /v1/engine/ws`
- `GET /v1/engine/live-voice`

Protected route groups require `Authorization: Bearer <NEURAL_TOKEN>`:

- `/v1/agents`
- `/v1/oversight`
- `/v1/infra`
- `/v1/model-manager`
- `/v1/skills`
- `/v1/benchmarks`
- `/v1/continuity`
- `/v1/docs`
- `/v1/system`
- `/v1/governance`
- `/v1/sovereign`
- `/v1/intelligence/*` symbol graph and blast radius APIs
- `/v1/search/memory`
- `/v1/env-schema`
- `/v1/engine/*` management routes
- `/v1/mcp/*`

## Data And Registry Locations

| Data | Current path/default | Notes |
| --- | --- | --- |
| Main SQLite database | `data/tadpole.db` | Default from `AppState::new` when `DATABASE_URL` is unset. |
| SQL migrations | `server-rs/migrations/` | Applied through `server-rs/src/db.rs`. |
| Agent registry data | SQLite plus `data/agents.json` where present | Agent records are loaded from SQLite; JSON files remain part of registry/runtime data. |
| Audio cache | `data/audio_cache.db` | Initialized by AppState, falls back to no-op if unavailable. |
| Built dashboard | `dist/` | Served by the Rust router when present. |
| Panic diagnostics | `sidecar_panic.log` | Written under `WORKSPACE_ROOT` when possible. |

## Optional Feature Gates

Default Cargo features are empty. These features are opt-in:

- `vector-memory`: enables LanceDB/Arrow-backed memory routes.
- `neural-audio`: enables optional audio dependencies.

Without `vector-memory`, memory routes intentionally return `501 Not Implemented`.

[//]: # (Metadata: [SYSTEM_MAP])

[//]: # (Metadata: [SYSTEM_MAP])
