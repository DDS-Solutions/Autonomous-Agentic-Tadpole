//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **build**: Core technical resource for the Tadpole OS infrastructure.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Unhandled errors, lock contention, or connection staling.
//! - **Telemetry Link**: Search `[build]` in tracing logs.
//! - **Trace Scope**: `src-tauri/build.rs`

fn main() {
    // [build] Tauri build script execution
    tauri_build::build()
}
