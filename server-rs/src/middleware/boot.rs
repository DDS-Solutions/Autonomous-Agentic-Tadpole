//! @docs ARCHITECTURE:Core
//! 
//! ### AI Assist Note
//! **Core technical resource for the Tadpole OS Sovereign infrastructure.**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[boot]` in tracing logs.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use crate::state::AppState;

/// Middleware that blocks requests until the system boot sequence is complete.
/// This ensures system actors (Audit, Memory, Security) are initialized before
/// any mission-critical routes are executed.
pub async fn wait_for_system_ready(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Await the boot gate. If already booted, returns immediately.
    state.wait_for_boot().await;
    
    Ok(next.run(request).await)
}

// Metadata: [boot]
