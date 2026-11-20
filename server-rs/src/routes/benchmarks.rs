//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Performance Benchmarks (Metrics API)**: Orchestrates the
//! stress testing and efficiency measurement for the Tadpole OS
//! agent swarm. Features **Latency & Throughput Tracking**: provides
//! endpoints for triggering and retrieving results from automated
//! benchmark suites (e.g., `run_benchmark_suite`). Implements **Trend
//! Analysis**: stores and compares historical performance data to
//! identify regressions in model response times or token
//! efficiency. AI agents should use these endpoints to validate
//! system stability after significant architectural or provider-layer
//! modifications (PERF-02).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: 500 Internal Server Error due to database
//!   connectivity issues during result persistence, or benchmark
//!   timeouts during high-concurrency stress tests.
//! - **Telemetry Link**: Search for `[benchmarks]` or `[Performance]`
//!   in `tracing` logs for suite execution milestones.
//! - **Trace Scope**: `server-rs::routes::benchmarks`

use crate::agent::benchmarks::{self, BenchmarkResult};
use crate::error::AppError;

use crate::state::AppState;
use axum::http::StatusCode;
use axum::{
    extract::{Path, State, Query},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

#[tracing::instrument(skip(state), name = "metrics::list_benchmarks")]
pub async fn get_benchmarks(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let results = benchmarks::list_benchmarks(&state.resources.pool)
        .await?;
    Ok(Json(results))
}

#[tracing::instrument(skip(state), name = "metrics::get_benchmark_history")]
pub async fn get_benchmark_history(
    State(state): State<Arc<AppState>>,
    Path(test_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let results = benchmarks::get_benchmark_comparison(&state.resources.pool, &test_id)
        .await?;
    Ok(Json(results))
}

#[tracing::instrument(skip(state, payload), name = "metrics::save_benchmark")]
pub async fn create_benchmark(
    State(state): State<Arc<AppState>>,
    Json(mut payload): Json<BenchmarkResult>,
) -> Result<impl IntoResponse, AppError> {
    if payload.id.is_empty() {
        payload.id = Uuid::new_v4().to_string();
    } else if payload.id.len() > 64 || !payload.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(AppError::BadRequest(
            "Invalid benchmark ID format. Must be alphanumeric slug or UUID".to_string(),
        ));
    }

    benchmarks::save_benchmark(&state.resources.pool, payload)
        .await?;

    Ok(StatusCode::CREATED)
}

#[derive(serde::Deserialize, Debug)]
pub struct BenchmarkTriggerQuery {
    pub mission_id: Option<String>,
    pub node_id: Option<String>,
}

#[tracing::instrument(skip(state), name = "metrics::trigger_suite")]
pub async fn trigger_benchmark(
    State(state): State<Arc<AppState>>,
    Path(test_id): Path<String>,
    Query(query): Query<BenchmarkTriggerQuery>,
) -> Result<impl IntoResponse, AppError> {
    if test_id.trim().is_empty()
        || test_id.len() > 64
        || !test_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::BadRequest(
            "Invalid test_id format. Must be alphanumeric slug or UUID".to_string(),
        ));
    }

    let result = benchmarks::run_benchmark_suite(
        state,
        &test_id,
        query.mission_id,
        query.node_id,
    )
    .await?;
    Ok(Json(result))
}

/// POST /v1/benchmarks/readiness
/// Executes the 4-probe native Overlord Readiness check (DB, Providers, Guardrails, Fleet).
#[tracing::instrument(skip(state), name = "metrics::readiness_probe")]
pub async fn run_readiness_probe(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let report = benchmarks::run_readiness_probe(state).await?;
    Ok(Json(report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::create_router;
    use axum::body::Body;
    use axum::http::{header, Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_readiness_probe_happy_and_auth_paths() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        state.notify_boot_complete();
        let state_for_assert = Arc::clone(&state);
        let token = state.security.deploy_token.clone();
        let app = create_router(state);

        // Failure Path: Unauthenticated request rejected with 401
        let unauth = Request::builder()
            .method("POST")
            .uri("/v1/benchmarks/readiness")
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(unauth).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // Happy Path: Authenticated request returns structured readiness report
        let authed = Request::builder()
            .method("POST")
            .uri("/v1/benchmarks/readiness")
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(authed).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), 100_000).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["score"].as_u64().is_some());
        assert_eq!(json["checks"].as_array().unwrap().len(), 4);
        assert!(json["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "guardrail_integrity" && c["status"] == "pass"));
        // Verify PRAGMA journal_mode is reported in database_integrity check
        let db_check = json["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "database_integrity")
            .unwrap();
        assert!(db_check["detail"].as_str().unwrap().contains("journal_mode ="));
        // Verify readiness probe does not pollute the benchmarks table
        let bench_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM benchmarks WHERE test_id = 'overlord-readiness'")
            .fetch_one(&state_for_assert.resources.pool)
            .await
            .unwrap_or(-1);
        assert_eq!(bench_count, 0, "Readiness probe must not pollute benchmarks table");
    }
}



// Metadata: [benchmarks]
