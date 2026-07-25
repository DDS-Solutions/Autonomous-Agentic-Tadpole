> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Documentation**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[CONTRIBUTING]` in audit logs.
>
> ### AI Assist Note
> Contributing to Tadpole OS
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

# Contributing to Tadpole OS

Welcome, Sovereign Engineers and AI Agents. This manual defines the operational guidelines, codebase extension patterns, and validation protocols required to maintain the architectural integrity of Tadpole OS.

---

## Technical Stack & Architecture

Tadpole OS utilizes a **3-Layer Architecture**:
1. **Directive Layer (`directives/`)**: Standard Operating Procedures (SOPs) in Markdown (e.g., [LONG_TERM_MEMORY.md](file:///g:/Autonomous-Agentic-Tadpole/directives/LONG_TERM_MEMORY.md)).
2. **Orchestration Layer (AI Agents / Nexus Engineer)**: Reads directives, triggers execution tools, and coordinates workflows.
3. **Execution Layer (`execution/`)**: Deterministic Python scripts that interface with external networks, validate codebases, or run security scans.

The core engine is implemented in Rust (`server-rs/`), and the user interface runs as a React/TypeScript application.

---

## Extension Patterns

### 1. Adding a New API Route

API routes are registered within [router.rs](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/router.rs) using the [Axum](https://docs.rs/axum/latest/axum/) framework.

**Workflow:**
1. Create a new handler in `server-rs/src/routes/<module_name>.rs`:
   ```rust
   use axum::{response::IntoResponse, Json, http::StatusCode};
   use serde::Serialize;
   
   #[derive(Serialize)]
   pub struct CustomResponse {
       status: String,
   }
   
   pub async fn handle_custom_route() -> impl IntoResponse {
       (StatusCode::OK, Json(CustomResponse { status: "Active".to_string() }))
   }
   ```
2. Re-export the route handler inside `server-rs/src/routes/mod.rs`:
   ```rust
   pub mod custom_module;
   ```
3. Register the route handler in [router.rs](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/router.rs) under the appropriate route builder function (`build_protected_v1_routes`, `build_engine_public_routes`, etc.):
   ```rust
   Router::new().route("/custom-path", get(routes::custom_module::handle_custom_route))
   ```
4. **Important**: Document the new route in `docs/openapi.yaml` (or run `python execution/generate_api_reference.py`) and run `python execution/parity_guard.py` to verify documentation alignment.

---

### ⚓ Pre-Commit Hook Setup

To automatically keep `docs/openapi.yaml` and `docs/API_REFERENCE.md` in sync whenever backend route definitions change, install the pre-commit hook:

```bash
cp hooks/pre-commit .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
```

---

### 2. Adding a New SystemService (Background Service)

The startup orchestration uses a phased startup system based on the `SystemService` trait defined in [mod.rs](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/startup/mod.rs).

**Workflow:**
1. Define a service struct in a new file under `server-rs/src/startup/services/` (e.g., `server-rs/src/startup/services/custom_service.rs`):
   ```rust
   use crate::startup::{SystemService, SystemContext};
   use async_trait::async_trait;
   
   pub struct CustomSystemWorker;
   
   #[async_trait]
   impl SystemService for CustomSystemWorker {
       fn name(&self) -> &'static str {
           "CustomSystemWorker"
       }
   
       fn is_critical(&self) -> bool {
           false // Set to true if boot should fail if this service crashes
       }
   
       async fn start(&self, context: SystemContext) -> Result<(), anyhow::Error> {
           let mut shutdown_rx = context.shutdown_rx.clone();
           tokio::spawn(async move {
               loop {
                   tokio::select! {
                       _ = shutdown_rx.changed() => {
                           tracing::info!("[CustomSystemWorker] Received shutdown signal. Draining...");
                           break;
                       }
                       _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                           tracing::info!("[CustomSystemWorker] Periodic heartbeat tick.");
                       }
                   }
               }
           });
           Ok(())
       }
   }
   ```
2. Re-export the service in `server-rs/src/startup/services/mod.rs`:
   ```rust
   pub mod custom_service;
   pub use custom_service::CustomSystemWorker;
   ```
3. Register the service in the boot flow inside [mod.rs](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/startup/mod.rs):
   ```rust
   warmup_tasks.push(Box::new(services::CustomSystemWorker));
   ```

---

### 3. Creating a New Execution Script

Deterministic tasks should live in the `execution/` directory as Python scripts.

**Conventions:**
- **AI Assist block**: Every script must start with a docstring containing:
  - `@docs` pointer specifying architecture link.
  - `### AI Assist Note` with a clear description of purpose.
  - `### 🔍 Debugging & Observability` mapping the failure paths and telemetry logs.
- **Parameters**: Never interpolate raw user strings into shells. Always validate or pass parameters safely.
- **Exit codes**: Explicitly exit with `0` for success and non-zero (e.g. `1`) for failure.

---

### 4. Cargo Feature Flags

Ensure compilation parity by configuring Cargo features properly. The key features defined in `server-rs/Cargo.toml` are:
- `default`: Normal engine configuration.
- `vector-memory`: Compiles high-performance vector DB bindings (disables fallback memory models).
- `neural-audio`: Integrates advanced neural audio synthesis drivers.

Use conditional compilation guards when accessing feature-specific routes:
```rust
#[cfg(feature = "vector-memory")]
{
    // Feature-specific logic
}
```

---

## Verification & Quality Gates

Before submitting a Pull Request, you must run the verification scripts located in `execution/`:

1. **Parity Guard Check**: Compares routes registered in Rust against documentation to ensure no drift occurs.
   ```bash
   python execution/parity_guard.py
   ```
2. **AI Context Alignment Verification**: Verifies that 100% of codebase files contain proper AI Context markers.
   ```bash
   python execution/verify_ai_context.py
   ```
   If markers are missing, run the remediator tool to auto-align them:
   ```bash
   python execution/awaken.py
   ```
3. **Rust Integrity**: Compile and test the workspace.
   ```bash
   cd server-rs
   cargo check
   cargo test --all
   ```

---

[//]: # (Metadata: [CONTRIBUTING])
