> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[Development_Guide]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

# 💻 Development Guide

## Local Dev Setup

### 1. Rust Engine (hot-reload)

```bash
# Install cargo-watch for auto-rebuild on save
cargo install cargo-watch

# Auto-recompile on change
cargo watch -x "run --manifest-path server-rs/Cargo.toml" -w server-rs/src
```

### 2. React Dashboard (hot-reload)

```bash
npm run dev
# Vite HMR updates instantly on save
```

### 3. VS Code / IDE Setup

Recommended extensions:
- `rust-analyzer` — Rust LSP
- `Even Better TOML` — Cargo.toml support
- `ESLint` + `Prettier` — TypeScript formatting
- `SQLite Viewer` — Browse `data/tadpole.db` directly

---

## Project Conventions

### Rust

| Convention | Rule |
|------------|------|
| Error type | Use `AppError` from `error.rs` for all route handlers |
| Logging | Use `tracing::info!/warn!/error!` with structured fields |
| Telemetry tags | Prefix with module name: `[IKS]`, `[graph]`, `[agent]` |
| Dead code | Add `#[allow(dead_code)]` to scaffolded/feature-gated items with a comment explaining why |
| Feature gates | Use `#[cfg(feature = "vector-memory")]` — never `cfg(not(...))` for primary paths |
| Panic policy | No `unwrap()` in route handlers — use `?` or `map_err()` |

### TypeScript / React

| Convention | Rule |
|------------|------|
| State | Zustand stores under `src/stores/` |
| API calls | Service layer under `src/services/` |
| Components | `PascalCase.tsx`, co-located test as `PascalCase.test.tsx` |
| Types | Shared contracts in `src/types/` |
| Routing | Routes defined in `src/constants/routes.ts` |

### Python (execution/)

| Convention | Rule |
|------------|------|
| Script entry | `if __name__ == "__main__":` with argparse |
| Error handling | Exit code 1 on failure, print to stderr |
| No shell=True | Always use `subprocess.run(args, ...)` with list |
| Env vars | Load via `python-dotenv` from `.env` |

---

## Adding a New API Endpoint

### 1. Create the route handler

```rust
// server-rs/src/routes/my_feature.rs
use crate::error::AppError;
use crate::state::AppState;
use axum::{Extension, Json};
use std::sync::Arc;

pub async fn get_my_data(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    // ... implementation
    Ok(Json(serde_json::json!({ "data": "hello" })))
}
```

### 2. Register in the router

```rust
// server-rs/src/router.rs
use crate::routes::my_feature;

// Inside build_protected_router():
.route("/v1/my-feature", get(my_feature::get_my_data))
```

### 3. Write a test

```rust
// server-rs/src/routes/my_feature_tests.rs
#[cfg(test)]
mod tests {
    // ... (see Testing Guide for the pattern)
}
```

### 4. Update OpenAPI docs

```bash
npm run docs:api
```

---

## Adding a New Agent Skill

### 1. Create the skill manifest

```json
// execution/skills/my_skill.json
{
  "id": "my_skill",
  "name": "My Skill",
  "description": "Does something useful",
  "version": "1.0.0",
  "entrypoint": "execution/skills/my_skill.py",
  "input_schema": {
    "type": "object",
    "properties": {
      "input": { "type": "string" }
    },
    "required": ["input"]
  }
}
```

### 2. Create the Python script

```python
# execution/skills/my_skill.py
import argparse, json, sys

def run(input: str) -> dict:
    return { "result": f"Processed: {input}" }

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True)
    args = parser.parse_args()
    print(json.dumps(run(args.input)))
```

### 3. Register via API

```bash
curl -X POST http://127.0.0.1:8000/v1/skills/import \
  -H "Authorization: Bearer $NEURAL_TOKEN" \
  -H "Content-Type: application/json" \
  -d @execution/skills/my_skill.json
```

---

## Database Migrations

```bash
# Create a new migration (use UTC timestamp)
# File: server-rs/migrations/20260627000000_add_my_table.sql

CREATE TABLE my_table (
    id   TEXT PRIMARY KEY NOT NULL,
    data TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
```

Migrations run automatically on engine startup via `sqlx::migrate!()`.

---

## Debugging

### Enable verbose Rust logging

```bash
RUST_LOG=debug npm run engine
# Or specific module:
RUST_LOG=server_rs::routes::health=trace npm run engine
```

### Inspect the database

```bash
# Via CLI
sqlite3 data/tadpole.db
.tables
SELECT * FROM agents LIMIT 5;

# Or use the VS Code SQLite Viewer extension
```

### Trace a failing test

```bash
RUST_BACKTRACE=1 cargo test --bin server-rs -- routes::my_test --nocapture
```

### Check audit trail

```bash
curl http://127.0.0.1:8000/v1/oversight/audit-trail \
  -H "Authorization: Bearer $NEURAL_TOKEN" | jq .data[-5:]
```

[//]: # (Metadata: [Development_Guide])
