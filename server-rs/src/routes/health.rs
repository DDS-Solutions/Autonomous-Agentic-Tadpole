//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Engine Health (Static Heartbeat)**: Orchestrates the verification
//! of the Tadpole OS engine's operational status and diagnostics.
//! Returns database connection pool info, WAL file sizing, daily LLM budget usage,
//! swarm telemetry, and server uptime.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: 503 Service Unavailable if the Axum router
//!   is saturated, incorrect agent counts due to registry sync delays,
//!   or database locking issues blocking health queries.
//! - **Telemetry Link**: Search for `tadpole_online_rust` in health logs.
//! - **Trace Scope**: `server-rs::routes::health`

use crate::error::AppError;
use crate::state::AppState;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{extract::State, Json};
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct DatabaseHealth {
    pub status: String,           // "healthy" | "degraded" | "failed"
    pub pool_size: u32,
    pub pool_idle: u32,
    pub wal_size_mb: f64,
    pub busy_timeout_ms: u64,
}

#[derive(Serialize)]
pub struct BudgetHealth {
    pub total_spent_usd: f64,
    pub daily_limit_usd: f64,
    pub percent_used: f64,
    pub status: String,           // "ok" | "warning" | "critical"
}

#[derive(Serialize)]
pub struct SwarmHealth {
    pub connected_bunkers: usize,
    pub total_agents: usize,
    pub max_swarm_depth: u32,
    pub status: String,
}

/// Heartbeat status response containing system telemetry and feature flags.
#[derive(Serialize)]
pub struct HealthResponse {
    /// Operational status string.
    pub status: String,
    /// Current engine version from Cargo.toml.
    pub version: String,
    /// ISO 8601 server timestamp.
    pub heartbeat: String,
    /// Count of currently registered agent nodes.
    pub active_agents: usize,
    /// List of enabled compile-time features.
    pub features: Vec<String>,
    // Extended Metrics:
    pub database: DatabaseHealth,
    pub budget: BudgetHealth,
    pub swarm: SwarmHealth,
    pub uptime_seconds: u64,
}

/// A simple heartbeat endpoint that mirrors the old `router.get("/health")` in Express.
#[tracing::instrument(skip(state), name = "system::health")]
pub async fn health_check(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    #[allow(unused_mut)]
    let mut features = Vec::new();

    #[cfg(feature = "neural-audio")]
    features.push("neural-audio".to_string());

    #[cfg(feature = "vector-memory")]
    features.push("vector-memory".to_string());

    // 1. Compute Database Health
    let db_ping = sqlx::query("SELECT 1").execute(&state.resources.pool).await;
    let db_status = if db_ping.is_ok() {
        "healthy".to_string()
    } else {
        "failed".to_string()
    };

    let pool_size = state.resources.pool.size() as u32;
    let pool_idle = state.resources.pool.num_idle() as u32;
    
    // Resolve WAL file size
    let wal_size_mb = match tokio::fs::metadata("data/tadpole.db-wal").await {
        Ok(meta) => (meta.len() as f64) / (1024.0 * 1024.0),
        Err(_) => 0.0,
    };

    let database = DatabaseHealth {
        status: db_status,
        pool_size,
        pool_idle,
        wal_size_mb,
        busy_timeout_ms: 30000,
    };

    // 2. Compute Budget Health
    let total_spent_usd: f64 = sqlx::query_scalar::<_, f64>(
        "SELECT COALESCE(SUM(used_usd), 0.0) FROM agent_quotas"
    )
    .fetch_one(&state.resources.pool)
    .await
    .unwrap_or(0.0);

    let daily_limit_usd: f64 = sqlx::query_scalar::<_, f64>(
        "SELECT COALESCE(SUM(budget_usd), 0.0) FROM agent_quotas WHERE reset_period = 'daily'"
    )
    .fetch_one(&state.resources.pool)
    .await
    .unwrap_or(0.0);

    let percent_used = if daily_limit_usd > 0.0 {
        (total_spent_usd / daily_limit_usd) * 100.0
    } else {
        0.0
    };

    let budget_status = if daily_limit_usd > 0.0 && percent_used > 80.0 {
        "critical".to_string()
    } else if daily_limit_usd > 0.0 && percent_used > 50.0 {
        "warning".to_string()
    } else {
        "ok".to_string()
    };

    let budget = BudgetHealth {
        total_spent_usd,
        daily_limit_usd,
        percent_used,
        status: budget_status,
    };

    // 3. Compute Swarm Health
    let total_agents = state.registry.agents.len();
    let max_swarm_depth = state.governance.max_swarm_depth.load(std::sync::atomic::Ordering::Relaxed);
    let swarm_status = if total_agents > 0 {
        "healthy".to_string()
    } else {
        "empty".to_string()
    };

    let swarm = SwarmHealth {
        connected_bunkers: 2, // Local and main remote fallback
        total_agents,
        max_swarm_depth,
        status: swarm_status,
    };

    // 4. Calculate Uptime
    let uptime_seconds = (chrono::Utc::now() - state.start_time).num_seconds().max(0) as u64;

    Ok((
        StatusCode::OK,
        Json(HealthResponse {
            status: "tadpole_online_rust".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            heartbeat: chrono::Utc::now().to_rfc3339(),
            active_agents: total_agents,
            features,
            database,
            budget,
            swarm,
            uptime_seconds,
        }),
    ))
}
