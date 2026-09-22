> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[API_Reference]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

# 📡 API Reference

> **Base URL:** `http://127.0.0.1:8000`  
> **API Prefix:** `/v1`  
> **Auth:** `Authorization: Bearer <NEURAL_TOKEN>` (required for all protected routes)

---

## Engine Telemetry & WebSocket Routes

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/health` | Core public health probe (Public, no auth) |
| `GET` | `/metrics` | Prometheus metrics export (Protected) |
| `GET` | `/v1/engine/health` | Engine health + extended metrics (Public, no auth) |
| `GET` | `/v1/engine/ws` | WebSocket telemetry stream (requires `Sec-WebSocket-Protocol: bearer.<token>`) |
| `GET` | `/v1/engine/live-voice` | Live voice WebSocket stream (requires `Sec-WebSocket-Protocol: bearer.<token>`) |

### Health Response Schema (v1.1.58+)

```json
{
  "status": "tadpole_online_rust",
  "version": "1.1.58",
  "heartbeat": "2026-06-26T22:00:00Z",
  "uptime_seconds": 3600,
  "database": {
    "status": "healthy",
    "pool_size": 5,
    "wal_size_bytes": 1024
  },
  "budget": {
    "total_spent_usd": 0.42,
    "limit_usd": 100.0
  },
  "swarm": {
    "agent_count": 3,
    "status": "nominal"
  }
}
```

---

## Protected Route Groups

### `/v1/agents` — Agent Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/agents` | List agents (paginated) |
| `POST` | `/v1/agents` | Create agent |
| `GET` | `/v1/agents/graph` | Agent relationship graph |
| `PUT` | `/v1/agents/{id}` | Update agent |
| `DELETE` | `/v1/agents/{id}` | Delete agent |
| `POST` | `/v1/agents/{id}/tasks` | Execute agent task |
| `POST` | `/v1/agents/{id}/reset` | Reset agent state |
| `POST` | `/v1/agents/{id}/pause` | Pause agent |
| `POST` | `/v1/agents/{id}/resume` | Resume agent |
| `POST` | `/v1/agents/{id}/mission` | Sync agent mission |
| `GET` | `/v1/agents/{id}/memories` | Get agent memories (feature: vector-memory) |
| `POST` | `/v1/agents/{id}/memories` | Save agent memory (feature: vector-memory) |
| `DELETE` | `/v1/agents/{id}/memories/{row_id}` | Delete specific agent memory row |
| `GET` | `/v1/agents/{id}/status-ledger` | Get agent living status document |
| `PUT` | `/v1/agents/{id}/status-ledger` | Update status ledger / heartbeat |
| `POST` | `/v1/agents/{id}/tasks/{task_id}/claim` | Atomically claim a task for this agent |
| `POST` | `/v1/agents/{id}/tasks/{task_id}/receipts` | Post state transition receipt |
| `GET` | `/v1/agents/{id}/context-packet` | Retrieve agent context packet |
| `PUT` | `/v1/agents/{id}/context-packet` | Update agent context packet |
| `GET` | `/v1/agents/{id}/skills/subscribed` | List subscribed skills |
| `POST` | `/v1/agents/{id}/skills/{skill_id}/subscribe` | Subscribe to a skill |
| `POST` | `/v1/agents/{id}/skills/{skill_id}/approve` | Approve a subscribed skill |
| `GET` | `/v1/agents/{id}/maintenance-report` | Retrieve agent maintenance report |

---

### `/v1/a2a` — Agent-to-Agent Economic Governance (2PC Ledger)

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/v1/a2a/prepare` | Prepare 2PC transaction with lock-aware projected spend cap check |
| `POST` | `/v1/a2a/commit` | Atomically commit prepared transaction via CAS lock guard |
| `POST` | `/v1/a2a/rollback` | Roll back prepared transaction lock |

---

### `/v1/oversight` — Governance & Security Audit

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/oversight/pending` | List pending oversight requests |
| `POST` | `/v1/oversight/{id}/decide` | Approve or reject an oversight decision |
| `GET` | `/v1/oversight/ledger` | Agent spend ledger |
| `PUT` | `/v1/oversight/settings` | Update governance and budget settings |
| `GET` | `/v1/oversight/token-burn` | Aggregated swarm token burn |
| `GET` | `/v1/oversight/security/quotas` | Agent quota allocations |
| `PUT` | `/v1/oversight/security/quotas/{id}` | Update agent quota allocation |
| `GET` | `/v1/oversight/security/missions/quotas` | Mission quota allocations |
| `PUT` | `/v1/oversight/security/missions/{id}/quota` | Update mission quota |
| `GET` | `/v1/oversight/security/audit-trail` | Tamper-evident cryptographically chained audit log |
| `GET` | `/v1/oversight/security/health` | Governance and security subsystem health |
| `GET` | `/v1/oversight/security/integrity` | Cryptographic signature and Merkle chain verification |
| `GET` | `/v1/oversight/security/policies` | Active security and permission policies |
| `PUT` | `/v1/oversight/security/policies` | Update security policy |

---

### `/v1/infra` — Infrastructure Node Discovery

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/infra/nodes` | List registered Bunker and edge nodes |
| `POST` | `/v1/infra/nodes/discover` | Trigger mDNS node discovery |

---

### `/v1/model-manager` — Model & Provider Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/model-manager/providers` | List configured providers |
| `PUT` | `/v1/model-manager/providers/{id}` | Update provider configuration |
| `DELETE` | `/v1/model-manager/providers/{id}` | Remove provider |
| `POST` | `/v1/model-manager/providers/{id}/test` | Test provider connectivity |
| `POST` | `/v1/model-manager/providers/{id}/sync` | Sync provider model catalog |
| `GET` | `/v1/model-manager/models` | List available models |
| `PUT` | `/v1/model-manager/models/{id}` | Update model configuration |
| `DELETE` | `/v1/model-manager/models/{id}` | Remove model from registry |
| `GET` | `/v1/model-manager/model-store/catalog` | Full model catalog |
| `POST` | `/v1/model-manager/model-store/pull` | Pull model into local model store |
| `POST` | `/v1/api/pull` | Ollama-compatible pull proxy |

---

### `/v1/skills` — Skill Registry & MCP Tools

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/skills` | List all registered skills |
| `GET` | `/v1/skills/manifests` | List skill manifests |
| `GET` | `/v1/skills/manifests/{name}` | Get specific skill manifest |
| `GET` | `/v1/skills/mcp-tools` | List MCP tool definitions |
| `POST` | `/v1/skills/mcp-tools/{name}/execute` | Execute MCP tool |
| `POST` | `/v1/skills/import` | Import capability from JSON manifest |
| `POST` | `/v1/skills/promote` | Promote artifact to skill |
| `POST` | `/v1/skills/register` | Register capability |
| `POST` | `/v1/skills/scan` | Scan workspace for new skills |
| `GET` | `/v1/skills/proposals` | List capability proposals |
| `POST` | `/v1/skills/proposals/{id}/resolve` | Resolve capability proposal |
| `PUT` | `/v1/skills/scripts/{name}` | Create/update Python execution script |
| `DELETE` | `/v1/skills/scripts/{name}` | Delete Python script |
| `PUT` | `/v1/skills/workflows/{name}` | Create/update workflow |
| `DELETE` | `/v1/skills/workflows/{name}` | Delete workflow |
| `PUT` | `/v1/skills/hooks/{name}` | Create/update lifecycle hook |
| `DELETE` | `/v1/skills/hooks/{name}` | Delete hook |

---

### `/v1/intelligence` — Code Intelligence Graph

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/intelligence/graph` | Full symbol dependency graph (query: `path_prefix`, `max_nodes`) |
| `POST` | `/v1/intelligence/graph/rebuild` | Rebuild AST symbol graph from workspace |
| `GET` | `/v1/intelligence/blast-radius` | Calculate change impact radius (query: `name`, `path`, `limit`) |
| `GET` | `/v1/intelligence/resolve` | Resolve dependent symbols for token budget (query: `name`, `path`, `budget`) |
| `GET` | `/v1/intelligence/impacted-tests` | Identify test files impacted by modified source path (query: `file`) |

---

### `/v1/knowledge` — Integrated Knowledge Store (IKS & OKF v0.3)

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/v1/knowledge` | Write new OKF knowledge entry (dedup by content hash) |
| `GET` | `/v1/knowledge` | List entries (query: `topic`, `cluster_id`, `concept_type`, `limit`, `offset`) |
| `GET` | `/v1/knowledge/search` | Semantic k-NN vector search (requires Cargo feature `vector-memory`) |
| `POST` | `/v1/knowledge/edges` | Add typed relational graph edge |
| `GET` | `/v1/knowledge/edges` | List relational graph edges (query: `source_id`, `target_id`) |
| `POST` | `/v1/knowledge/synthesize` | Synthesize cross-agent knowledge entries via Ollama |
| `POST` | `/v1/knowledge/{id}/confirm` | Human-confirm knowledge entry |
| `GET` | `/v1/knowledge/{id}/peers` | Get semantic peer nodes for knowledge entry |
| `DELETE` | `/v1/knowledge/{id}` | Delete knowledge entry by ID |

---

### `/v1/continuity` — Scheduled Jobs & Workflows

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/continuity/jobs` | List scheduled continuity jobs |
| `POST` | `/v1/continuity/jobs` | Create continuity job |
| `GET` | `/v1/continuity/jobs/{id}` | Get job by ID |
| `PUT` | `/v1/continuity/jobs/{id}` | Update job schedule or params |
| `DELETE` | `/v1/continuity/jobs/{id}` | Delete job |
| `GET` | `/v1/continuity/jobs/{id}/runs` | List historical job execution runs |
| `POST` | `/v1/continuity/jobs/{id}/enable` | Enable scheduled job |
| `POST` | `/v1/continuity/jobs/{id}/disable` | Disable scheduled job |
| `GET` | `/v1/continuity/workflows` | List workflows |
| `POST` | `/v1/continuity/workflows` | Create workflow |
| `POST` | `/v1/continuity/workflows/{id}/steps` | Append step to workflow |
| `DELETE` | `/v1/continuity/workflows/{id}` | Delete workflow |

---

### `/v1/benchmarks` — Benchmark Suite

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/benchmarks` | List benchmark definitions |
| `POST` | `/v1/benchmarks` | Create benchmark run definition |
| `POST` | `/v1/benchmarks/run/{id}` | Execute benchmark run |
| `GET` | `/v1/benchmarks/{id}` | Historical benchmark results |

---

### `/v1/sovereign` — Sovereign Mission State

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/sovereign/missions/{id}/leaves` | Get active leaf nodes of a mission session |
| `POST` | `/v1/sovereign/missions/{id}/nodes` | Append session node |
| `GET` | `/v1/sovereign/missions/{id}/nodes/{leaf_id}/history` | Get branch history leading to node |
| `POST` | `/v1/sovereign/missions/{id}/nodes/{node_id}/revert` | Revert to previous session node |

---

### `/v1/system` & Memory Search

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/system/compute-profile` | CPU, RAM, GPU hardware profile |
| `GET` | `/v1/system/debug/services` | Debug registered system services |
| `GET` | `/v1/system/debug/queues` | Debug internal actor and task queues |
| `GET` | `/v1/env-schema` | Schema definitions for environment configuration |
| `GET` | `/v1/memory/search/bm25` | Sub-millisecond exact keyword/symbol search (< 1ms) |
| `GET` | `/v1/memory/search/hybrid` | Unified Hybrid Triad Reciprocal Rank Fusion search |
| `GET` | `/v1/search/memory` | Global vector memory search (feature: `vector-memory`) |

---

### `/v1/engine/*` — Engine Control & Templates

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/v1/engine/shutdown` | Graceful shutdown (4-phase) |
| `POST` | `/v1/engine/kill` | Emergency kill switch |
| `POST` | `/v1/engine/deploy` | Deploy updated engine binary |
| `POST` | `/v1/engine/pre-pr` | Run pre-PR automated check gates |
| `POST` | `/v1/engine/transcribe` | Whisper audio transcription |
| `POST` | `/v1/engine/speak` | Text-to-speech generation |
| `GET` | `/v1/engine/templates/catalog` | List available workspace templates |
| `POST` | `/v1/engine/templates/install` | Install starter kit template |
| `GET` | `/v1/engine/templates/installed` | List installed templates |
| `DELETE` | `/v1/engine/templates/{id}` | Uninstall template |

---

### `/v1/governance` & `/v1/iacp`

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/governance/blueprints` | List system blueprints |
| `POST` | `/v1/governance/blueprints` | Save system blueprint |
| `DELETE` | `/v1/governance/blueprints/{id}` | Delete blueprint |
| `GET` | `/v1/governance/manifest` | Get sovereign manifest |
| `POST` | `/v1/iacp/negotiate` | Negotiate inter-agent contract |
| `POST` | `/v1/iacp/hire` | Execute inter-agent delegation hire |

---

### `/v1/mcp/*` — MCP SSE Transport Bridge

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/mcp/sse` | Server-Sent Events stream for external MCP clients |
| `POST` | `/v1/mcp/message` | JSON-RPC message ingress for SSE sessions |

---

## Pagination

List endpoints support standard query parameters:

| Param | Default | Description |
|-------|---------|-------------|
| `page` | `1` | Page number (1-indexed) |
| `per_page` | `20` | Items per page (max 100) |

Response includes HATEOAS links:

```json
{
  "data": [...],
  "pagination": {
    "page": 1,
    "per_page": 20,
    "total": 87,
    "next": "/v1/agents?page=2&per_page=20",
    "prev": null
  }
}
```

---

## Error Responses (RFC 9457)

All errors use Problem Details format:

```json
{
  "type": "https://tadpole.os/errors/not-found",
  "title": "Not Found",
  "status": 404,
  "detail": "Agent with id 'abc-123' was not found",
  "instance": "/v1/agents/abc-123"
}
```

| Status | Meaning |
|--------|---------|
| `400` | Bad Request — invalid payload |
| `401` | Unauthorized — missing or invalid token |
| `403` | Forbidden — path traversal / permission denied |
| `404` | Not Found |
| `408` | Request Timeout (120s limit) |
| `429` | Too Many Requests — brute-force protection |
| `500` | Internal Server Error |
| `501` | Not Implemented — feature requires Cargo feature flag |

[//]: # (Metadata: [API_Reference])
