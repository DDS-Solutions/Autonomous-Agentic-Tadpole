//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **Intelligence Routes Integration Tests**: Verifies path boundaries
//! and correct error mapping behavior under path traversal attempts.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Failures to load mock state or unexpected panic.
//! - **Telemetry Link**: Not tracked (integration test runner).
//!

use std::sync::Arc;
use server_rs::{
    error::AppError,
    intelligence::service::IntelligenceService,
    state::AppState,
};

#[tokio::test]
async fn test_intelligence_path_validation_boundaries() {
    let app_state = AppState::new_minimal_mock().await;
    let service = IntelligenceService::new(Arc::new(app_state));

    // Case 1: Path Traversal (outside authorized workspace root)
    // Should immediately return AppError::Forbidden (boundary violation)
    let traversal_res = service.blast_radius("my_symbol", "../../etc/passwd").await;
    assert!(traversal_res.is_err(), "Expected boundary violation error");
    match traversal_res.unwrap_err() {
        AppError::Forbidden(msg) => {
            assert!(msg.contains("Invalid path boundary"));
        }
        other => panic!("Expected AppError::Forbidden, got: {:?}", other),
    }

    let traversal_res_resolve = service.resolve_context("my_symbol", "../../etc/passwd", 4000).await;
    assert!(traversal_res_resolve.is_err(), "Expected boundary violation error");
    match traversal_res_resolve.unwrap_err() {
        AppError::Forbidden(msg) => {
            assert!(msg.contains("Invalid path boundary"));
        }
        other => panic!("Expected AppError::Forbidden, got: {:?}", other),
    }

    // Case 2: Valid path inside workspace, but missing in obfuscated map
    // Should return AppError::IntelPathUnknown (404) rather than Forbidden
    let unknown_res = service.blast_radius("my_symbol", "src/valid_but_unknown.rs").await;
    assert!(unknown_res.is_err(), "Expected unknown path error");
    match unknown_res.unwrap_err() {
        AppError::IntelPathUnknown(msg) => {
            assert!(msg.contains("Path lookup failed"));
        }
        other => panic!("Expected AppError::IntelPathUnknown, got: {:?}", other),
    }

    let unknown_res_resolve = service.resolve_context("my_symbol", "src/valid_but_unknown.rs", 4000).await;
    assert!(unknown_res_resolve.is_err(), "Expected unknown path error");
    match unknown_res_resolve.unwrap_err() {
        AppError::IntelPathUnknown(msg) => {
            assert!(msg.contains("Path lookup failed"));
        }
        other => panic!("Expected AppError::IntelPathUnknown, got: {:?}", other),
    }
}

// Metadata: [intelligence_routes]

// Metadata: [intelligence_routes]
