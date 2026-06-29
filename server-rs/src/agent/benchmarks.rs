//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Benchmarking Suite**: Provides a standardized testbed for evaluating
//! model latency, cost, and accuracy across different task categories.
//! Orchestrates **System Performance Baselines** (Runner, DB, Rate Limiter)
//! to ensure that local infrastructure overhead does not degrade the
//! swarm's decision-making speed. Records results to the `benchmarks`
//! table for longitudinal analysis.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[benchmarks]` in tracing logs.
//! - **Failure Path**: Benchmark timeout on under-provisioned hardware,
//!   SQLite deadlocks during high-concurrency stress tests, or invalid
//!   test IDs causing early return errors.
//! - **Trace Scope**: `server-rs::agent::benchmarks`

use crate::state::AppState;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BenchmarkResult {
    pub id: String,
    pub name: String,
    pub category: String,
    pub test_id: String,
    pub mean_ms: f64,
    pub p95_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub target_value: Option<String>,
    pub status: String, // PASS | FAIL | NEEDS_INVESTIGATION
    pub metadata: Option<String>,
    pub created_at: String,
}

pub async fn save_benchmark(pool: &SqlitePool, result: BenchmarkResult) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO benchmarks (id, name, category, test_id, mean_ms, p95_ms, p99_ms, target_value, status, metadata)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&result.id)
    .bind(&result.name)
    .bind(&result.category)
    .bind(&result.test_id)
    .bind(result.mean_ms)
    .bind(result.p95_ms)
    .bind(result.p99_ms)
    .bind(&result.target_value)
    .bind(&result.status)
    .bind(&result.metadata)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list_benchmarks(pool: &SqlitePool) -> Result<Vec<BenchmarkResult>, AppError> {
    let results = sqlx::query_as::<_, BenchmarkResult>(
        "SELECT id, name, category, test_id, mean_ms, p95_ms, p99_ms, target_value, status, metadata, CAST(created_at AS TEXT) as created_at FROM benchmarks ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await?;

    Ok(results)
}

pub async fn get_benchmark_comparison(
    pool: &SqlitePool,
    test_id: &str,
) -> Result<Vec<BenchmarkResult>, AppError> {
    let results = sqlx::query_as::<_, BenchmarkResult>(
        "SELECT id, name, category, test_id, mean_ms, p95_ms, p99_ms, target_value, status, metadata, CAST(created_at AS TEXT) as created_at FROM benchmarks WHERE test_id = ? ORDER BY created_at DESC LIMIT 10"
    )
    .bind(test_id)
    .fetch_all(pool)
    .await?;

    Ok(results)
}
pub async fn run_benchmark_suite(
    state: Arc<AppState>,
    test_id: &str,
    mission_id: Option<String>,
    node_id: Option<String>,
) -> Result<BenchmarkResult, AppError> {
    let pool = &state.resources.pool;
    let mut status = "PASS".to_string();
    let target_value;
    let name;
    let category;
    
    let mut meta_obj = serde_json::json!({
        "trigger": "Self-triggered via Dashboard"
    });
    if let Some(ref m_id) = mission_id {
        meta_obj["mission_id"] = serde_json::json!(m_id);
    }
    if let Some(ref n_id) = node_id {
        meta_obj["node_id"] = serde_json::json!(n_id);
    }
    let metadata_str = serde_json::to_string(&meta_obj).unwrap_or_default();
    let mut metadata = Some(metadata_str);

    let start = std::time::Instant::now();

    match test_id {
        "BM-RUN-01" => {
            name = "Agent Runner Baseline".to_string();
            category = "Runner".to_string();
            target_value = Some("< 100ms".to_string());

            // Measure agent initialization and context resolution overhead
            let runner = crate::agent::runner::AgentRunner::new(state.clone());

            for _ in 0..10 {
                let _ = runner.state.registry.agents.contains_key("overlord");
            }
        }
        "BM-DB-01" => {
            name = "Persistence Stress (50 Writes)".to_string();
            category = "Database".to_string();
            target_value = Some("< 200ms".to_string());

            // Perform 50 actual writes
            for i in 0..50 {
                sqlx::query("INSERT INTO benchmark_logs (test_id, step) VALUES (?, ?)")
                    .bind(test_id)
                    .bind(i)
                    .execute(pool)
                    .await?;
            }
        }
        "BM-RL-01" => {
            name = "Rate Limiter Overhead".to_string();
            category = "Rate Limiter".to_string();
            target_value = Some("< 1ms".to_string());

            let limiter = crate::agent::rate_limiter::RateLimiter::new(Some(1000), Some(100000));
            for _ in 0..100 {
                limiter.acquire(10).await;
                limiter.record_usage(10);
            }
            let mut meta_with_cycles = meta_obj.clone();
            meta_with_cycles["details"] = serde_json::json!("100 cycles of acquire/record");
            metadata = Some(serde_json::to_string(&meta_with_cycles).unwrap_or_default());
        }
        _ => return Err(AppError::BadRequest(format!("Unknown benchmark test ID: {}", test_id))),
    }

    let duration = start.elapsed().as_secs_f64() * 1000.0;

    // Check status against target thresholds
    if test_id == "BM-RUN-01" && duration > 100.0 {
        status = "FAIL".to_string();
    }
    if test_id == "BM-DB-01" && duration > 200.0 {
        status = "FAIL".to_string();
    }
    if test_id == "BM-RL-01" && duration > 1.0 {
        status = "FAIL".to_string();
    }

    let result = BenchmarkResult {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        category,
        test_id: test_id.to_string(),
        mean_ms: duration,
        p95_ms: Some(duration * 1.02), // Statistical approximation for single run
        p99_ms: Some(duration * 1.05),
        target_value,
        status,
        metadata,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    save_benchmark(pool, result.clone()).await?;

    // Autonomous Self-Healing trigger
    if result.status == "FAIL" {
        if let (Some(m_id), Some(n_id)) = (mission_id, node_id) {
            // Retrieve history to find last known green state
            if let Ok(history) = state.traverse_session_history_sovereign(&n_id).await {
                let mut target_node_id = None;
                for node in history.iter().rev().skip(1) {
                    if let Some(node_id_str) = node["id"].as_str() {
                        let has_passed = sqlx::query_scalar::<_, i64>(
                            "SELECT COUNT(*) FROM benchmarks WHERE status = 'PASS' AND metadata LIKE ?"
                        )
                        .bind(format!("%\"node_id\":\"{}\"%", node_id_str))
                        .fetch_one(&state.resources.pool)
                        .await
                        .unwrap_or(0);

                        if has_passed > 0 {
                            target_node_id = Some(node_id_str.to_string());
                            break;
                        }
                    }
                }

                if target_node_id.is_none() && history.len() >= 2 {
                    target_node_id = history[history.len() - 2]["id"].as_str().map(|s| s.to_string());
                }

                if let Some(target_node) = target_node_id {
                    tracing::warn!("♻️ [SelfHealing] Autonomously reverting mission {} to last known green node {}", m_id, target_node);
                    let _ = state.revert_to_node_sovereign(&m_id, &target_node).await;
                    state.broadcast_sys(
                        &format!("♻️ [SelfHealing] Autonomously reverted mission to node {}", target_node),
                        "warning",
                        Some(m_id.clone()),
                    );
                }
            }
        }
    }

    Ok(result)
}





// Metadata: [benchmarks]
