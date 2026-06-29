> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[Configuration]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

# ⚙️ Configuration

All configuration is loaded from `.env` in the workspace root. Schema is validated at startup against `.env.schema`.

## Required Variables

| Variable | Description |
|----------|-------------|
| `NEURAL_TOKEN` | Master bearer token for all protected API routes |

---

## Core Engine Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `8000` | TCP port the engine binds to |
| `BIND_ADDRESS` | `127.0.0.1` | Network interface to bind (never use `0.0.0.0` in production) |
| `STATIC_DIR` | `dist` | Directory to serve as the React SPA |
| `WORKSPACE_ROOT` | `.` (cwd) | Base path for data, scripts, skills, and panic logs |
| `DATABASE_URL` | `sqlite:<WORKSPACE_ROOT>/data/tadpole.db` | SQLx connection string |
| `RESOURCE_ROOT` | *(optional)* | Additional resource directory |
| `HEARTBEAT_INTERVAL_SECS` | `3` | WebSocket heartbeat cadence in seconds |
| `SKIP_DB_SEED` | `false` | Skip baseline database seeding on startup |

---

## Security Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `NEURAL_TOKEN` | **Required** | Primary auth token |
| `NEURAL_TOKEN_OLD` | *(optional)* | Previous token — valid during rotation grace window |
| `NEURAL_TOKEN_NEW` | *(optional)* | Replacement token — valid to pre-validate before rotation |
| `NEURAL_ENGINE_ACCESS_TOKEN` | *(optional)* | Legacy secondary token accepted alongside NEURAL_TOKEN |
| `ALLOWED_ORIGINS` | Local dev origins | Comma-separated CORS allow-list |
| `PRIVACY_MODE` | `false` | `true` = restrict to local-only providers, block cloud APIs |
| `TADPOLE_ALLOW_LOCAL_HTTP` | *(unset)* | Allow insecure `http://` model-provider calls when set |

---

## Observability Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DISABLE_TELEMETRY` | `false` | `true` = disable OpenTelemetry stdout exporter |

---

## Testing / CI Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `TADPOLE_NULL_PROVIDERS` | *(unset)* | Forces null (mock) providers — use in CI test environments |

---

## Provider API Keys

Provider keys are optional — only configure the providers you intend to use:

```ini
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
GOOGLE_API_KEY=AIza...
GROQ_API_KEY=gsk_...
DEEPSEEK_API_KEY=sk-...
REPLICATE_API_KEY=r8_...
OLLAMA_HOST=http://localhost:11434
```

---

## Cargo Feature Flags

Default builds are lightweight. Enable optional heavy features at compile time:

| Feature | Dependency | What it enables |
|---------|-----------|-----------------|
| `vector-memory` | LanceDB, Arrow | Semantic search in Knowledge Store, `/v1/search/memory` endpoints |
| `neural-audio` | Whisper | `/v1/engine/transcribe` and voice pipelines |

```bash
# Enable vector memory
cargo run --manifest-path server-rs/Cargo.toml --features vector-memory

# Enable both
cargo run --manifest-path server-rs/Cargo.toml --features vector-memory,neural-audio
```

> **Note:** When `vector-memory` is disabled, memory endpoints return `501 Not Implemented`. The IKS still stores and retrieves text; it just has no semantic search.

---

## `.env.example` Template

```ini
# ─── Required ─────────────────────────────────────────────────────
NEURAL_TOKEN=change-me-to-a-strong-random-token

# ─── Engine ───────────────────────────────────────────────────────
PORT=8000
BIND_ADDRESS=127.0.0.1
WORKSPACE_ROOT=.
DATABASE_URL=sqlite:./data/tadpole.db
HEARTBEAT_INTERVAL_SECS=3
SKIP_DB_SEED=false

# ─── Security ─────────────────────────────────────────────────────
PRIVACY_MODE=false
ALLOWED_ORIGINS=http://localhost:5173,http://127.0.0.1:5173

# ─── Observability ────────────────────────────────────────────────
DISABLE_TELEMETRY=false

# ─── Provider Keys (optional) ─────────────────────────────────────
OPENAI_API_KEY=
ANTHROPIC_API_KEY=
GOOGLE_API_KEY=
GROQ_API_KEY=
OLLAMA_HOST=http://localhost:11434
```

---

## Environment Schema Validation

The engine validates `.env` on startup using `.env.schema`:

```bash
# Schema format example
PORT          int      default=8000          required=false
NEURAL_TOKEN  string                          required=true   sensitive=true
PRIVACY_MODE  bool     default=false          required=false
```

If a required key is missing or has the wrong type, the engine refuses to start with a descriptive error.

[//]: # (Metadata: [Configuration])
