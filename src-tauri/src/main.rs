//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **main**: Core technical resource for the Tadpole OS infrastructure.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Unhandled errors, lock contention, or connection staling.
//! - **Telemetry Link**: Search `[main]` in tracing logs.
//! - **Trace Scope**: `src-tauri/src/main.rs`

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // [main] Tadpole OS desktop entry point
    tadpole_os_lib::run();
}
