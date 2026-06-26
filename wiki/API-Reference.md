# 📡 API Reference

> **Base URL:** `http://127.0.0.1:8000`  
> **API Prefix:** `/v1`  
> **Auth:** `Authorization: Bearer <NEURAL_TOKEN>` (required for all protected routes)

---

## Public Routes (No Auth Required)

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/engine/health` | Engine health + extended metrics |
| `GET` | `/v1/engine/ws` | WebSocket stream (auth via subprotocol) |
| `GET` | `/v1/engine/live-voice` | Live voice WebSocket stream |

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
| `GET` | `/v1/agents/:id` | Get agent by ID |
| `PATCH` | `/v1/agents/:id` | Update agent |
| `DELETE` | `/v1/agents/:id` | Delete agent |
| `POST` | `/v1/agents/:id/tasks` | Execute agent task |
| `GET` | `/v1/agents/:id/tasks` | List agent tasks |
| `POST` | `/v1/agents/:id/pause` | Pause agent |
| `POST` | `/v1/agents/:id/resume` | Resume agent |
| `GET` | `/v1/agents/:id/memory` | Get agent memories |
| `DELETE` | `/v1/agents/:id/memory` | Clear agent memories |
| `GET` | `/v1/agents/graph` | Agent relationship graph |
| `GET` | `/v1/agents/:id/status-ledger` | Get agent living status document |
| `PUT` | `/v1/agents/:id/status-ledger` | Update status ledger / heartbeat |
| `POST` | `/v1/agents/:id/tasks/:task_id/claim` | Atomically claim a task for this agent |
| `POST` | `/v1/agents/:id/tasks/:task_id/receipts` | Post state transition receipt |

---

### `/v1/oversight` — Governance & Audit

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/oversight/decisions` | Pending oversight decisions |
| `POST` | `/v1/oversight/decisions/:id/approve` | Approve a decision |
| `POST` | `/v1/oversight/decisions/:id/reject` | Reject a decision |
| `GET` | `/v1/oversight/audit-trail` | Full tamper-evident audit log |
| `GET` | `/v1/oversight/ledger` | Agent spend ledger |
| `GET` | `/v1/oversight/quotas` | Agent quota status |
| `GET` | `/v1/oversight/health` | Governance system health |
| `GET` | `/v1/oversight/policy` | Current governance policy |
| `PUT` | `/v1/oversight/policy` | Update governance policy |

---

### `/v1/model-manager` — Model & Provider Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/model-manager/providers` | List configured providers |
| `POST` | `/v1/model-manager/providers` | Add provider |
| `GET` | `/v1/model-manager/models` | List available models |
| `GET` | `/v1/model-manager/catalog` | Full model catalog |
| `POST` | `/v1/model-manager/pull` | Pull Ollama model |
| `POST` | `/v1/model-manager/providers/:id/test` | Test provider connectivity |

---

### `/v1/skills` — Skill Registry

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/skills` | List registered skills |
| `POST` | `/v1/skills` | Register new skill |
| `GET` | `/v1/skills/:id` | Get skill by ID |
| `POST` | `/v1/skills/import` | Import skill from JSON manifest |
| `POST` | `/v1/skills/:id/promote` | Promote skill to production |
| `GET` | `/v1/skills/scripts` | List Python execution scripts |
| `GET` | `/v1/skills/mcp-tools` | List MCP tool definitions |
| `POST` | `/v1/skills/workflows` | Execute skill workflow |
| `POST` | `/v1/skills/hooks` | Trigger lifecycle hook |

---

### `/v1/intelligence` — Knowledge Graph

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/intelligence/graph` | Full symbol dependency graph |
| `POST` | `/v1/intelligence/resolve` | Resolve file path to symbols |
| `POST` | `/v1/intelligence/blast-radius` | Calculate change impact radius |
| `POST` | `/v1/intelligence/refresh` | Rebuild graph from workspace |

#### Blast Radius Request

```json
{
  "path": "server-rs/src/routes/health.rs",
  "depth": 3
}
```

---

### `/v1/continuity` — Scheduled Jobs

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/continuity/jobs` | List scheduled jobs |
| `POST` | `/v1/continuity/jobs` | Create job |
| `GET` | `/v1/continuity/jobs/:id` | Get job by ID |
| `DELETE` | `/v1/continuity/jobs/:id` | Delete job |
| `POST` | `/v1/continuity/jobs/:id/trigger` | Manual trigger |
| `GET` | `/v1/continuity/workflows` | List workflows |

---

### `/v1/benchmarks` — Benchmark Suite

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/benchmarks` | List benchmark definitions |
| `POST` | `/v1/benchmarks/:id/run` | Execute benchmark |
| `GET` | `/v1/benchmarks/:id/history` | Historical results |

---

### `/v1/sovereign` — Mission Sessions

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/sovereign/sessions` | List mission sessions |
| `POST` | `/v1/sovereign/sessions` | Create session |
| `GET` | `/v1/sovereign/sessions/:id/tree` | Session state tree |
| `POST` | `/v1/sovereign/sessions/:id/revert` | Revert to previous state |

---

### `/v1/engine/*` — Engine Control

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/v1/engine/shutdown` | Graceful shutdown (4-phase) |
| `POST` | `/v1/engine/kill` | Emergency kill switch |
| `POST` | `/v1/engine/deploy` | Deploy updated binary |
| `GET` | `/v1/engine/compute-profile` | CPU, RAM, GPU profile |

---

### `/v1/mcp/*` — MCP Bridge

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/v1/mcp/sse` | Server-Sent Events stream |
| `POST` | `/v1/mcp/message` | Send MCP message |
| `GET` | `/v1/mcp/tools` | List all MCP tools |
| `POST` | `/v1/mcp/execute` | Execute MCP tool |

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
