//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **lib**: Core technical resource for the Tadpole OS desktop shell runtime.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Tauri context generation failure, IPC initialization error, or window creation fault.
//! - **Telemetry Link**: Search `[lib]` in tracing logs.
//! - **Trace Scope**: `src-tauri/src/lib.rs`

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // [lib] Tadpole OS desktop runtime initialization
    tauri::Builder::default()
        .setup(|_app| {
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
