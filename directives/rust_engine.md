> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[rust_engine]` in audit logs.
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
> - **Failure Path**: Information drift or legacy terminology.
> - **Telemetry Link**: Search `[rust_engine]` in audit logs.

# Rust Engine Development Guidelines

This directive defines the code patterns, compiler constraints, and architectural invariants for modifying the `server-rs` Rust engine.

---

## Architectural Invariants

### 1. The Hub Pattern for Global State
All global systems and database connections reside under the centralized [AppState](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/state/mod.rs) struct. Sub-states are grouped logically into static hubs under [server-rs/src/state/hubs/](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/state/hubs/):
- **Registry Hub (`reg`)**: In-memory and persistent agent registry databases.
- **Resource Hub (`res`)**: Database pools, task executors, and file locks.
- **Security Hub (`sec`)**: Audit trails, budget guards, command scanners, and secret redactors.
- **Governance Hub (`gov`)**: Configuration values, active limits, and state variables.
- **Communication Hub (`comm`)**: Live event bus and WebSocket dispatchers.

**Rule**: Do not create new ad-hoc global states or static variables. All new subsystem states must be registered within an appropriate hub in `server-rs/src/state/hubs/` and initialized during boot.

---

## Code Patterns & Safety Guidelines

### 2. Bridge Type Exports (bridge.rs)
The contract bridge [bridge.rs](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/bridge.rs) exports Rust models to TypeScript for frontend use.
- **Invariant**: Always use the `export_type!` macro pattern.
- **Reasoning**: The `export_type!` macro handles errors gracefully (logging via `tracing::error!`) instead of calling `.unwrap()` or `.expect()`. A compilation or type export failure should **never** hard-panic `cargo test`.
- **Constraint**: Do not replace the macro with direct `.expect()` wrappers.

### 3. Background Services & SystemService Trait
All long-running background tasks must implement the `SystemService` trait defined in [startup/mod.rs](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/startup/mod.rs):
- **Lifecycle Integration**: Register new services in the phased warmup vector inside `spawn_background_tasks`.
- **Graceful Shutdown**: Always monitor the watch channel `context.shutdown_rx` for termination signals. Complete persistence or cleanup tasks within the service's designated shutdown window.
- **Example Pattern**:
  ```rust
  async fn start(&self, context: SystemContext) -> Result<(), anyhow::Error> {
      let mut shutdown_rx = context.shutdown_rx.clone();
      tokio::spawn(async move {
          loop {
              tokio::select! {
                  _ = shutdown_rx.changed() => {
                      // Perform graceful cleanup
                      break;
                  }
                  _ = tokio::time::sleep(tokio::time::Duration::from_secs(60)) => {
                      // Normal execution loop
                  }
              }
          }
      });
      Ok(())
  }
  ```

### 4. Async AppState Borrowing Patterns
`AppState` is typically wrapped in `Arc<AppState>`, meaning it is thread-safe and cheaply cloneable.
- When spawning async tasks or thread pools, always **clone the Arc** (e.g. `let state = app_state.clone()`) and move the cloned Arc into the async block.
- Minimize write locks on shared resources. Prefer atomic types (`AtomicU32`, `AtomicBool`) or concurrent maps (`DashMap`) over global lock acquisition to avoid thread starvation.

---

## Database & Migrations

### 5. SQLx Database Migrations
Migrations are managed via SQLx and located in [server-rs/migrations/](file:///g:/Autonomous-Agentic-Tadpole/server-rs/migrations/).
- **Naming Convention**: Use the timestamp-prefixed pattern: `YYYYMMDDHHMMSS_description.sql`.
- **Idempotency**: Ensure all SQL statements are safe to run multiple times (e.g., `CREATE TABLE IF NOT EXISTS`, `ALTER TABLE ... ADD COLUMN ...` check).
- **Execution**: Apply migrations automatically during startup or verify integrity in tests using `sqlx::migrate!().run(&pool).await`.

---

## Feature Flag Combinations
The engine is optimized for conditional compilation. Always test changes across the three primary configuration profiles:
1. **Bare Engine**: `cargo check --no-default-features`
2. **Vector Memory**: `cargo check --features vector-memory` (enables local embeddings and semantic vector storage)
3. **Neural Audio**: `cargo check --features neural-audio` (compiles hardware-accelerated TTS/STT pipelines)

---

[//]: # (Metadata: [rust_engine])
