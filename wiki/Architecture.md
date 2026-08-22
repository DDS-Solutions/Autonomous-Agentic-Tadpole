> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[Architecture]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

# 🏗️ Architecture

## Overview

A-A Tadpole implements a strict **3-layer architecture** that separates concerns to maximize reliability:

```
Layer 1: DIRECTIVE      directives/        Markdown SOPs — what to do
Layer 2: ORCHESTRATION  server-rs/         Rust engine — intelligent routing & decision-making
Layer 3: EXECUTION      execution/         Python scripts — deterministic, testable work
```

---

## Runtime Layers

| Layer | Path | Technology | Responsibility |
|-------|------|------------|----------------|
| **Interface** | `src/` | React 19, TypeScript, Vite, Zustand, Tailwind | Operations dashboard, live telemetry, mission control |
| **Engine** | `server-rs/src/` | Rust 2021, Axum, Tokio, SQLx | API routing, agent lifecycle, state, security, workers |
| **Execution** | `execution/` | Python 3.10+, asyncio | Skills, MCP host, backup, audit, token rotation |

---

## Engine Boot Sequence

```
main.rs
  │
  ├── Load .env + validate config  (config.rs)
  ├── Init tracing + telemetry     (telemetry/)
  ├── Run SQLite migrations        (db.rs)
  ├── Build AppState               (state/mod.rs)
  │     ├── CommunicationHub       (broadcast channels, WebSocket)
  │     ├── GovernanceHub          (budget, swarm depth, privacy)
  │     ├── RegistryHub            (agents, providers, models, skills, MCP)
  │     ├── SecurityHub            (audit, budget guard, scanner, tokens)
  │     └── ResourceHub            (DB pool, HTTP client, base_dir, arbiter)
  │
  ├── Spawn background workers     (startup/services/)
  │     ├── NetworkingService      (mDNS Swarm Discovery)
  │     ├── SwarmPulseService      (heartbeat telemetry loop)
  │     ├── TelemetryService       (OTel exporter)
  │     └── SecurityService        (session eviction)
  │
  ├── signal boot_gate → true      (notify_boot_complete)
  └── Bind Axum on 127.0.0.1:8000
```

---

## AppState Hubs

`AppState` is the single dependency-injected context passed to all route handlers:

```rust
pub struct AppState {
    pub comms:      Arc<CommunicationHub>,   // Channels, WebSocket, oversight queues
    pub governance: Arc<GovernanceHub>,      // Budget, swarm limits, privacy mode
    pub registry:   Arc<RegistryHub>,        // Agents, providers, models, skills, MCP
    pub security:   Arc<SecurityHub>,        // Audit trail, budget guard, auth tokens
    pub resources:  Arc<ResourceHub>,        // DB pool, HTTP client, file paths
    pub boot_gate:  WatchChannel<bool>,      // Request gate until system is ready
    pub event_bus:  Arc<SystemEventBus>,     // Pub-sub for file/compute/webhook events
    pub start_time: DateTime<Utc>,           // For uptime_seconds in health endpoint
}
```

---

## Router Architecture

`server-rs/src/router.rs` assembles all routes with a layered middleware stack:

```
Request
  │
  ├── CORS layer
  ├── Compression (br, gzip, zstd)
  ├── Request timeout (120s → 408)
  ├── Deprecation headers
  ├── TraceLayer (request spans)
  ├── Rate limit headers
  ├── Security headers (CSP, HSTS, X-Frame)
  ├── Brute-force limiter
  ├── Boot gate (blocks until system ready)
  │
  ├── /v1/engine/health   ← Public (no auth)
  ├── /v1/engine/ws       ← Public (WebSocket)
  │
  └── /v1/** ──── validate_token middleware ────► Protected routes
                  (Bearer <token> check)
```

---

## Agent Runner Pipeline

When an agent executes a mission:

```
AgentRunner::run()
  │
  ├── 1. socratic.rs      Auto-inject 0-turn Socratic Context Contract (Scope, Threshold, Mode, Failure Policies)
  ├── 2. context.rs       Build prompt context (memory, skills, identity)
  ├── 3. intelligence.rs  Resolve knowledge graph + blast-radius
  ├── 4. provider.rs      Select LLM provider (Ollama, OpenAI, Anthropic, etc.)
  ├── 5. synthesis.rs     Generate response via LLM
  ├── 6. tools/mod.rs     Execute tool calls (MCP, Python, WASM, Docker)
  ├── 7. oversight.rs     Check governance gates (budget, approval queue)
  ├── 8. refinement.rs    Quality gate + retry logic
  ├── 9. lifecycle.rs     Update agent state and health
  └── 10. finalize.rs     Persist results, emit telemetry
```

---

## Intelligence / Knowledge Graph

The `intelligence/graph/` module implements a **force-directed symbol graph**:

- **Discovery** (`discover/`) — Walks the workspace file tree
- **Parsing** (`parse/`) — Extracts Rust/TypeScript symbols via Tree-sitter AST
- **Indexing** (`key/`) — Obfuscated-path registry for traversal-safe lookups
- **Caching** (`cache/`) — Lock-protected in-memory cache with LRU eviction
- **Engine** (`engine.rs`) — `CodeSymbolGraph` orchestrates the full pipeline
- **API** (`routes/intelligence.rs`) — `/v1/intelligence/blast-radius`, `/v1/intelligence/resolve`

---

## Security Architecture

See the full [Security Model](Security-Model) page. Key elements:

| Component | Implementation |
|-----------|----------------|
| Auth | `middleware/auth.rs` — constant-time Bearer token check via `subtle` crate |
| Token rotation | Dual-token grace window (`NEURAL_TOKEN_OLD` / `NEURAL_TOKEN_NEW`) |
| Shell scanning | `security/scanner.rs` — blocks dangerous shell patterns |
| Audit trail | `security/audit.rs` — Merkle-chained tamper-evident log |
| Budget guard | `security/metering.rs` — per-agent USD spend enforcement |
| MCP sandboxing | `execution/tadpole_mcp_server.py` — no shell=True, setrlimit, allowlist |

---

## Data Flow: Mission Execution

```
Dashboard ──POST /v1/agents/{id}/tasks──► AgentRunner
                                               │
                              ┌────────────────┤
                              ▼                ▼
                         LLM Provider      MCP / Skills
                         (Ollama, OAI)     (Python / WASM)
                              │                │
                              └────────┬───────┘
                                       ▼
                              Oversight Queue
                              (if budget > threshold)
                                       │
                              ┌────────▼────────┐
                              │  Mission Result  │
                              │  + Audit Entry   │
                              │  + Telemetry     │
                              └─────────────────┘
```

---

## Repository Map

```
Autonomous-Agentic-Tadpole/
├── src/                          React dashboard source
│   ├── components/               UI components (chat, graph, panels)
│   ├── pages/                    Route pages (agents, missions, etc.)
│   ├── stores/                   Zustand state stores
│   ├── services/                 API service layer
│   └── types/                    TypeScript type contracts
│
├── server-rs/
│   ├── src/
│   │   ├── agent/                Agent runner, MCP host, knowledge store
│   │   │   ├── runner/           Execution pipeline (10 stages)
│   │   │   ├── socratic.rs       0-Turn Socratic Context Contracts
│   │   │   ├── knowledge_store/  IKS — durable semantic memory
│   │   │   └── mcp/              MCP host and tool registry
│   │   ├── intelligence/graph/   Symbol graph + blast-radius engine
│   │   ├── middleware/           Auth, rate-limit, boot-gate, headers
│   │   ├── routes/               All /v1 endpoint handlers
│   │   ├── security/             Audit trail, budget, scanner, permissions
│   │   ├── services/             ACL, discovery, swarm services
│   │   ├── state/                AppState + all hubs
│   │   ├── startup/              Boot sequence + service modules
│   │   ├── system/               Actors, orchestrator, event bus, supervisor
│   │   ├── telemetry/            Pulse, bridge, OTel exporter
│   │   └── types/                Shared Rust types
│   └── migrations/               SQLite schema migration files
│
├── execution/                    Python execution layer
│   ├── tadpole_mcp_server.py     MCP server (sandboxed subprocess host)
│   ├── optimize_local_slot_routing.py Model slot routing optimizer (Ollama)
│   ├── backup_sqlite.py          WAL-safe hot backup utility
│   ├── restore_sqlite.py         Backup restoration + integrity check
│   ├── rotate_token.py           Zero-downtime token rotation
│   ├── parity_guard.py           Documentation parity auditor
│   ├── verify_ai_context.py      AI context marker validator
│   └── core/                     Modular skill framework
│
├── directives/                   Markdown SOPs for agent behavior
├── docs/                         Architecture, API, Security, Operations docs
├── monitoring/                   Prometheus alerts + Grafana dashboards
├── wiki/                         This wiki
├── tests/unit/                   Python unit tests
└── .github/workflows/            CI pipeline (ci.yml, release.yml)
```

[//]: # (Metadata: [Architecture])
