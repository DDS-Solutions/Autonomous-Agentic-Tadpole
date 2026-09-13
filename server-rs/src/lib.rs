//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **Rust Service Library**: Re-exports all core sub-systems and routers
//! to facilitate unit and integration testing boundaries.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Struct configuration drift or compilation errors in dependency injection.
//! - **Telemetry Link**: Not tracked (static module declarations).
//!



pub mod adapter;
pub mod agent;
pub mod bridge;
pub mod db;
#[cfg(test)]
pub mod db_tests;
pub mod config;
pub mod env_schema;
pub mod error;
pub mod intelligence;
pub mod middleware;
pub mod networking;
pub mod router;
pub mod routes;
pub mod secret_redactor;
pub mod security;
pub mod services;
pub mod startup;
pub mod state;
pub mod system;
pub mod telemetry;
pub mod types;
pub mod utils;

pub use state::AppState;

// Metadata: [lib]

// Metadata: [lib]
