//! @docs ARCHITECTURE:AgenticEngine
//!
//! ### AI Assist Note
//! Computes agent maintenance reports from operational health dimensions.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Health metric query or threshold evaluation failure.
//! - **Telemetry Link**: Search [agentic_engine] in tracing logs.

use crate::error::AppError;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use specta::Type;
use std::sync::Arc;

// ─── Maintenance Report Types & Evaluators ───────────────────────────────────

#[derive(Serialize, Type, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentMaintenanceReport {
    pub agent_id: String,
    pub overall_score: f64,
    pub status: String,
    pub capability_drift: MaintenanceDimension,
    pub memory_health: MaintenanceDimension,
    pub rate_limits: MaintenanceDimension,
    pub latency: MaintenanceDimension,
    pub error_rate: MaintenanceDimension,
    pub budget_headroom: MaintenanceDimension,
    pub dependency_health: MaintenanceDimension,
    #[specta(type = f64)]
    pub evaluated_at: i64,
}

#[derive(Serialize, Type, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceDimension {
    pub score: f64,
    pub status: String, // "optimal" | "warning" | "critical"
    pub details: String,
}

impl MaintenanceDimension {
    fn new(score: f64, status: &str, details: impl Into<String>) -> Self {
        Self {
            score,
            status: status.to_string(),
            details: details.into(),
        }
    }
}

fn evaluate_capability_drift(pending_reapproval_count: i64) -> MaintenanceDimension {
    if pending_reapproval_count > 0 {
        MaintenanceDimension::new(
            0.7,
            "warning",
            format!(
                "{} skill subscription(s) pending re-approval due to scope hash change.",
                pending_reapproval_count
            ),
        )
    } else {
        MaintenanceDimension::new(
            1.0,
            "optimal",
            "No capability drift detected. All approved skill scope hashes are aligned.",
        )
    }
}

fn evaluate_memory_health(wm_len: usize) -> MaintenanceDimension {
    if wm_len > 2000 {
        MaintenanceDimension::new(
            0.8,
            "warning",
            format!(
                "Working memory load is elevated ({} characters). Consider memory compaction.",
                wm_len
            ),
        )
    } else {
        MaintenanceDimension::new(
            1.0,
            "optimal",
            format!(
                "Working memory size is within nominal limits ({} characters).",
                wm_len
            ),
        )
    }
}

fn evaluate_rate_limits(rate_limit_hits: i64) -> MaintenanceDimension {
    if rate_limit_hits > 0 {
        MaintenanceDimension::new(
            0.6,
            "warning",
            format!(
                "Rate limiting occurrences detected in {} recent tasks.",
                rate_limit_hits
            ),
        )
    } else {
        MaintenanceDimension::new(
            1.0,
            "optimal",
            "No rate limit violations or HTTP 429 throttles detected.",
        )
    }
}

fn evaluate_latency(avg_latency: Option<f64>) -> MaintenanceDimension {
    match avg_latency {
        Some(lat) if lat > 180.0 => MaintenanceDimension::new(
            0.5,
            "critical",
            format!(
                "Critical average execution latency: {:.1}s (exceeds 180s threshold).",
                lat
            ),
        ),
        Some(lat) if lat > 60.0 => MaintenanceDimension::new(
            0.8,
            "warning",
            format!(
                "Elevated average execution latency: {:.1}s (exceeds 60s threshold).",
                lat
            ),
        ),
        Some(lat) => MaintenanceDimension::new(
            1.0,
            "optimal",
            format!("Nominal average execution latency: {:.1}s.", lat),
        ),
        None => MaintenanceDimension::new(
            1.0,
            "optimal",
            "No completed tasks available to measure latency.",
        ),
    }
}

fn evaluate_error_rate(task_stats: Option<(i64, i64)>) -> MaintenanceDimension {
    match task_stats {
        Some((total, failed)) if total > 0 => {
            let rate = failed as f64 / total as f64;
            if rate > 0.3 {
                MaintenanceDimension::new(
                    0.5,
                    "critical",
                    format!(
                        "High task error rate: {:.1}% ({}/{} failed).",
                        rate * 100.0,
                        failed,
                        total
                    ),
                )
            } else if rate > 0.1 {
                MaintenanceDimension::new(
                    0.8,
                    "warning",
                    format!(
                        "Moderate task error rate: {:.1}% ({}/{} failed).",
                        rate * 100.0,
                        failed,
                        total
                    ),
                )
            } else {
                MaintenanceDimension::new(
                    1.0,
                    "optimal",
                    format!(
                        "Nominal error rate: {:.1}% ({}/{} failed).",
                        rate * 100.0,
                        failed,
                        total
                    ),
                )
            }
        }
        _ => MaintenanceDimension::new(
            1.0,
            "optimal",
            "No recent tasks available to evaluate error rate.",
        ),
    }
}

fn evaluate_budget_headroom(budget: f64, cost: f64) -> MaintenanceDimension {
    if budget <= 0.0 {
        MaintenanceDimension::new(
            1.0,
            "optimal",
            "Unlimited budget limit. Full headroom available.",
        )
    } else if cost >= budget {
        MaintenanceDimension::new(
            0.0,
            "critical",
            format!(
                "Budget fully exhausted (${:.2} spent of ${:.2} budget). Agent locked.",
                cost, budget
            ),
        )
    } else {
        let headroom_ratio = (budget - cost) / budget;
        if headroom_ratio < 0.1 {
            MaintenanceDimension::new(
                0.5,
                "critical",
                format!(
                    "Critical budget headroom: {:.1}% remaining (${:.2} spent of ${:.2} budget).",
                    headroom_ratio * 100.0,
                    cost,
                    budget
                ),
            )
        } else if headroom_ratio < 0.25 {
            MaintenanceDimension::new(
                0.8,
                "warning",
                format!(
                    "Low budget headroom: {:.1}% remaining (${:.2} spent of ${:.2} budget).",
                    headroom_ratio * 100.0,
                    cost,
                    budget
                ),
            )
        } else {
            MaintenanceDimension::new(
                1.0,
                "optimal",
                format!(
                    "Nominal budget headroom: {:.1}% remaining (${:.2} spent of ${:.2} budget).",
                    headroom_ratio * 100.0,
                    cost,
                    budget
                ),
            )
        }
    }
}

fn evaluate_dependency_health(declined_count: i64, pending_count: i64) -> MaintenanceDimension {
    if declined_count > 0 {
        MaintenanceDimension::new(
            0.4,
            "critical",
            format!(
                "Critical: {} requested skill subscriptions were declined.",
                declined_count
            ),
        )
    } else if pending_count > 0 {
        MaintenanceDimension::new(
            0.8,
            "warning",
            format!(
                "Warning: {} skill subscription(s) are pending approval.",
                pending_count
            ),
        )
    } else {
        MaintenanceDimension::new(
            1.0,
            "optimal",
            "All skill dependencies approved and aligned.",
        )
    }
}

/// GET /v1/agents/:id/maintenance-report
///
/// Evaluates the agent node across 7 dimensions (capability drift, memory health,
/// rate limits, latency, error rate, budget headroom, dependency health).
///
/// @docs API_REFERENCE:GetAgentMaintenanceReport
#[tracing::instrument(skip(state), name = "governance::get_maintenance_report")]
pub async fn get_maintenance_report(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let now = chrono::Utc::now().timestamp();

    // Extract snapshot metrics from DashMap and drop the shard guard BEFORE any `.await` point
    // to prevent TOCTOU panics and cross-await DashMap shard deadlocks.
    let (wm_len, budget_usd, cost_usd) = {
        let agent_entry = state
            .registry
            .agents
            .get(&agent_id)
            .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", agent_id)))?;
        let agent = agent_entry.value();
        let wm_len = serde_json::to_string(&agent.state.working_memory)
            .unwrap_or_default()
            .len();
        (wm_len, agent.economics.budget_usd, agent.economics.cost_usd)
    };

    // Query all three counts from skill_subscriptions in a single trip to the DB
    let (pending_reapproval_count, declined_count, pending_count): (i64, i64, i64) =
        sqlx::query_as(
            "SELECT 
                COALESCE(SUM(CASE WHEN subscription_status = 'pending_reapproval' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN subscription_status = 'declined' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN subscription_status = 'pending' THEN 1 ELSE 0 END), 0)
             FROM skill_subscriptions 
             WHERE agent_id = ?1",
        )
        .bind(&agent_id)
        .fetch_one(&state.resources.pool)
        .await
        .map_err(AppError::Sqlx)?;

    let rate_limit_hits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_tasks 
         WHERE agent_id = ?1 AND (receipt_history LIKE '%rate_limit%' OR receipt_history LIKE '%429%')",
    )
    .bind(&agent_id)
    .fetch_one(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let avg_latency: Option<f64> = sqlx::query_scalar(
        "SELECT AVG(updated_at - claimed_at) FROM (
            SELECT claimed_at, updated_at FROM agent_tasks 
            WHERE agent_id = ?1 AND status = 'done' AND claimed_at IS NOT NULL
            ORDER BY updated_at DESC
            LIMIT 50
         )",
    )
    .bind(&agent_id)
    .fetch_one(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let task_stats: Option<(i64, i64)> = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN current_receipt = 'failed' THEN 1 ELSE 0 END), 0) 
         FROM (
            SELECT current_receipt FROM agent_tasks 
            WHERE agent_id = ?1
            ORDER BY updated_at DESC
            LIMIT 50
         )",
    )
    .bind(&agent_id)
    .fetch_optional(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let capability_drift = evaluate_capability_drift(pending_reapproval_count);
    let memory_health = evaluate_memory_health(wm_len);
    let rate_limits = evaluate_rate_limits(rate_limit_hits);
    let latency = evaluate_latency(avg_latency);
    let error_rate = evaluate_error_rate(task_stats);
    let budget_headroom = evaluate_budget_headroom(budget_usd, cost_usd);
    let dependency_health = evaluate_dependency_health(declined_count, pending_count);

    let overall_score = (capability_drift.score
        + memory_health.score
        + rate_limits.score
        + latency.score
        + error_rate.score
        + budget_headroom.score
        + dependency_health.score)
        / 7.0;

    let overall_status = if overall_score >= 0.95 {
        "healthy".to_string()
    } else if overall_score >= 0.75 {
        "warning".to_string()
    } else {
        "critical".to_string()
    };

    Ok(Json(AgentMaintenanceReport {
        agent_id,
        overall_score,
        status: overall_status,
        capability_drift,
        memory_health,
        rate_limits,
        latency,
        error_rate,
        budget_headroom,
        dependency_health,
        evaluated_at: now,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_maintenance_dimension_evaluators_happy_failure_and_edge() {
        // (a) Happy Path: Optimal thresholds across dimensions
        assert_eq!(evaluate_capability_drift(0).status, "optimal");
        assert_eq!(evaluate_memory_health(512).status, "optimal");
        assert_eq!(evaluate_rate_limits(0).status, "optimal");
        assert_eq!(evaluate_latency(Some(12.5)).status, "optimal");
        assert_eq!(evaluate_error_rate(Some((50, 2))).status, "optimal");
        assert_eq!(evaluate_budget_headroom(10.0, 2.0).status, "optimal");
        assert_eq!(evaluate_dependency_health(0, 0).status, "optimal");

        // (b) Failure Path: Critical and warning degradation thresholds
        assert_eq!(evaluate_capability_drift(2).status, "warning");
        assert_eq!(evaluate_memory_health(2500).status, "warning");
        assert_eq!(evaluate_rate_limits(3).status, "warning");
        assert_eq!(evaluate_latency(Some(240.0)).status, "critical");
        assert_eq!(evaluate_error_rate(Some((10, 5))).status, "critical");
        assert_eq!(evaluate_budget_headroom(5.0, 5.0).status, "critical");
        assert_eq!(evaluate_dependency_health(1, 0).status, "critical");

        // (c) Edge Case Path: Zero tasks, unlimited (0.0) budget, exact boundary values
        assert_eq!(evaluate_latency(None).status, "optimal");
        assert_eq!(evaluate_error_rate(Some((0, 0))).status, "optimal");
        assert_eq!(evaluate_budget_headroom(0.0, 100.0).status, "optimal");
        assert_eq!(evaluate_memory_health(2000).status, "optimal");
        assert_eq!(evaluate_latency(Some(60.0)).status, "optimal");
        assert_eq!(evaluate_latency(Some(180.0)).status, "warning");
    }
}
