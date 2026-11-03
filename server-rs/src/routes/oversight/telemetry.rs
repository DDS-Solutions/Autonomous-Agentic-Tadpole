//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Telemetry & Token Burn Subsystem**: Aggregates token usage, burn costs, and agent health
//! metrics across SQLite stores and Prometheus gauges.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Token burn calculation error, database query timeout, or Prometheus metric export failure.
//! - **Telemetry Link**: Search for `[telemetry]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::oversight::telemetry`

use axum::{extract::State, response::IntoResponse, Json};
use serde::Serialize;
use serde_json::json;
use specta::Type;
use sqlx::{Row, SqlitePool};
use std::sync::Arc;

use crate::error::AppError;
use crate::routes::oversight::principal::{HEALTHY_FAILURE_LIMIT, THROTTLED_FAILURE_THRESHOLD};
use crate::state::AppState;

/// GET /v1/oversight/security/health
///
/// Returns health metrics for all registered agents.
///
/// @docs OPERATIONS_MANUAL:AgentHealth
#[tracing::instrument(skip(state), name = "security_governance::get_health")]
pub async fn get_agent_health(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let health_data: Vec<serde_json::Value> = state
        .registry
        .agents
        .iter()
        .map(|entry| {
            let agent = entry.value();
            let is_suspended = agent.health.status.eq_ignore_ascii_case("suspended");
            let is_bankrupt = if agent.economics.budget_usd <= 0.0 {
                agent.economics.cost_usd > 0.0
            } else {
                agent.economics.cost_usd >= agent.economics.budget_usd
            };
            json!({
                "agent_id": agent.identity.id,
                "name": agent.identity.name,
                "status": agent.health.status,
                "failure_count": agent.health.failure_count,
                "last_failure_at": agent.health.last_failure_at,
                "is_healthy": agent.health.failure_count < HEALTHY_FAILURE_LIMIT && !is_suspended,
                "is_suspended": is_suspended,
                "is_throttled": agent.health.failure_count >= THROTTLED_FAILURE_THRESHOLD,
                "is_bankrupt": is_bankrupt,
            })
        })
        .collect();

    Ok(Json(json!({ "agents": health_data })))
}

#[derive(Serialize, Type, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TokenBurnReport {
    pub source: String,
    #[specta(type = f64)]
    pub total_tokens_in: i64,
    #[specta(type = f64)]
    pub total_tokens_out: i64,
    pub total_cost_usd: f64,
    pub providers: Vec<ProviderTokenBurn>,
    pub agents: Vec<AgentTokenBurn>,
    pub recent_burns: Vec<RecentTokenBurn>,
}

#[derive(Serialize, Type, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTokenBurn {
    pub provider_id: String,
    #[specta(type = f64)]
    pub tokens_in: i64,
    #[specta(type = f64)]
    pub tokens_out: i64,
    pub cost_usd: f64,
}

#[derive(Serialize, Type, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentTokenBurn {
    pub agent_id: String,
    pub agent_name: String,
    #[specta(type = f64)]
    pub tokens_in: i64,
    #[specta(type = f64)]
    pub tokens_out: i64,
    pub cost_usd: f64,
}

#[derive(Serialize, Type, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RecentTokenBurn {
    pub task_id: String,
    pub title: String,
    pub agent_id: String,
    pub agent_name: String,
    #[specta(type = f64)]
    pub tokens_in: i64,
    #[specta(type = f64)]
    pub tokens_out: i64,
    pub cost_usd: f64,
    #[specta(type = f64)]
    pub updated_at: i64,
}

fn decode_timestamp_epoch(row: &sqlx::sqlite::SqliteRow, col: &str) -> i64 {
    if let Ok(Some(epoch)) = row.try_get::<Option<i64>, _>(col) {
        return epoch;
    }
    if let Ok(Some(dt)) = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(col) {
        return dt.timestamp();
    }
    if let Ok(Some(raw)) = row.try_get::<Option<String>, _>(col) {
        let trimmed = raw.trim();
        if let Ok(epoch) = trimmed.parse::<i64>() {
            return epoch;
        }
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
            return dt.timestamp();
        }
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
            return naive.and_utc().timestamp();
        }
    }
    0
}

/// Determines the single authoritative token-burn source (`"agent_tasks"` when tasks have
/// recorded usage/cost, falling back to `"agents"` otherwise) so totals and breakdowns reconcile.
async fn query_overall_token_totals(
    pool: &SqlitePool,
) -> Result<(&'static str, i64, i64, f64), AppError> {
    let totals_row = sqlx::query(
        "SELECT 
            COALESCE(SUM(tokens_in), 0) as total_tokens_in,
            COALESCE(SUM(tokens_out), 0) as total_tokens_out,
            COALESCE(SUM(cost_usd), 0.0) as total_cost_usd
         FROM agent_tasks",
    )
    .fetch_one(pool)
    .await
    .map_err(AppError::Sqlx)?;

    let total_tokens_in: i64 = totals_row
        .try_get("total_tokens_in")
        .map_err(AppError::Sqlx)?;
    let total_tokens_out: i64 = totals_row
        .try_get("total_tokens_out")
        .map_err(AppError::Sqlx)?;
    let total_cost_usd: f64 = totals_row
        .try_get("total_cost_usd")
        .map_err(AppError::Sqlx)?;

    if total_tokens_in > 0 || total_tokens_out > 0 || total_cost_usd > 0.0 {
        return Ok((
            "agent_tasks",
            total_tokens_in,
            total_tokens_out,
            total_cost_usd,
        ));
    }

    let agent_totals_row = sqlx::query(
        "SELECT 
            COALESCE(SUM(input_tokens), 0) as total_tokens_in,
            COALESCE(SUM(output_tokens), 0) as total_tokens_out,
            COALESCE(SUM(cost_usd), 0.0) as total_cost_usd
         FROM agents",
    )
    .fetch_one(pool)
    .await
    .map_err(AppError::Sqlx)?;

    let fallback_in: i64 = agent_totals_row
        .try_get("total_tokens_in")
        .map_err(AppError::Sqlx)?;
    let fallback_out: i64 = agent_totals_row
        .try_get("total_tokens_out")
        .map_err(AppError::Sqlx)?;
    let fallback_cost: f64 = agent_totals_row
        .try_get("total_cost_usd")
        .map_err(AppError::Sqlx)?;

    Ok(("agents", fallback_in, fallback_out, fallback_cost))
}

/// Queries provider-grouped token burn metrics from the same authoritative `source` table as totals.
async fn query_provider_token_burns(
    pool: &SqlitePool,
    source: &'static str,
) -> Result<Vec<ProviderTokenBurn>, AppError> {
    let query_sql = if source == "agent_tasks" {
        "SELECT 
            COALESCE(provider_id, 'unknown') as provider_id,
            COALESCE(SUM(tokens_in), 0) as tokens_in,
            COALESCE(SUM(tokens_out), 0) as tokens_out,
            COALESCE(SUM(cost_usd), 0.0) as cost_usd
         FROM agent_tasks
         WHERE COALESCE(tokens_in, 0) > 0 OR COALESCE(tokens_out, 0) > 0 OR COALESCE(cost_usd, 0.0) > 0.0
         GROUP BY provider_id"
    } else {
        "SELECT 
            COALESCE(provider, 'unknown') as provider_id,
            COALESCE(SUM(input_tokens), 0) as tokens_in,
            COALESCE(SUM(output_tokens), 0) as tokens_out,
            COALESCE(SUM(cost_usd), 0.0) as cost_usd
         FROM agents
         WHERE COALESCE(input_tokens, 0) > 0 OR COALESCE(output_tokens, 0) > 0 OR COALESCE(cost_usd, 0.0) > 0.0
         GROUP BY provider"
    };

    let rows = sqlx::query(query_sql)
        .fetch_all(pool)
        .await
        .map_err(AppError::Sqlx)?;

    let mut providers = Vec::with_capacity(rows.len());
    for row in rows {
        providers.push(ProviderTokenBurn {
            provider_id: row.try_get("provider_id").map_err(AppError::Sqlx)?,
            tokens_in: row.try_get("tokens_in").map_err(AppError::Sqlx)?,
            tokens_out: row.try_get("tokens_out").map_err(AppError::Sqlx)?,
            cost_usd: row.try_get("cost_usd").map_err(AppError::Sqlx)?,
        });
    }

    Ok(providers)
}

/// Queries per-agent token consumption from the same authoritative `source` table so parts sum to the total.
async fn query_agent_token_burns(
    pool: &SqlitePool,
    source: &'static str,
) -> Result<Vec<AgentTokenBurn>, AppError> {
    let query_sql = if source == "agent_tasks" {
        "SELECT 
            t.agent_id as agent_id,
            COALESCE(a.name, 'Unknown Agent') as agent_name,
            COALESCE(SUM(t.tokens_in), 0) as tokens_in,
            COALESCE(SUM(t.tokens_out), 0) as tokens_out,
            COALESCE(SUM(t.cost_usd), 0.0) as cost_usd
         FROM agent_tasks t
         LEFT JOIN agents a ON t.agent_id = a.id
         WHERE COALESCE(t.tokens_in, 0) > 0 OR COALESCE(t.tokens_out, 0) > 0 OR COALESCE(t.cost_usd, 0.0) > 0.0
         GROUP BY t.agent_id, a.name"
    } else {
        "SELECT 
            id as agent_id,
            name as agent_name,
            COALESCE(input_tokens, 0) as tokens_in,
            COALESCE(output_tokens, 0) as tokens_out,
            COALESCE(cost_usd, 0.0) as cost_usd
         FROM agents
         WHERE COALESCE(input_tokens, 0) > 0 OR COALESCE(output_tokens, 0) > 0 OR COALESCE(cost_usd, 0.0) > 0.0"
    };

    let rows = sqlx::query(query_sql)
        .fetch_all(pool)
        .await
        .map_err(AppError::Sqlx)?;

    let mut agents = Vec::with_capacity(rows.len());
    for row in rows {
        agents.push(AgentTokenBurn {
            agent_id: row.try_get("agent_id").map_err(AppError::Sqlx)?,
            agent_name: row.try_get("agent_name").map_err(AppError::Sqlx)?,
            tokens_in: row.try_get("tokens_in").map_err(AppError::Sqlx)?,
            tokens_out: row.try_get("tokens_out").map_err(AppError::Sqlx)?,
            cost_usd: row.try_get("cost_usd").map_err(AppError::Sqlx)?,
        });
    }

    Ok(agents)
}

/// Queries the 20 most recent tasks with non-zero token or cost burn.
/// Falls back to agent-level aggregations when task records are not available (§4.2).
async fn query_recent_token_burns(
    pool: &SqlitePool,
    source: &str,
) -> Result<Vec<RecentTokenBurn>, AppError> {
    let recent_rows = sqlx::query(
        "SELECT 
            t.id as task_id,
            t.title,
            t.agent_id,
            COALESCE(a.name, 'Unknown Agent') as agent_name,
            COALESCE(t.tokens_in, 0) as tokens_in,
            COALESCE(t.tokens_out, 0) as tokens_out,
            COALESCE(t.cost_usd, 0.0) as cost_usd,
            t.updated_at
         FROM agent_tasks t
         LEFT JOIN agents a ON t.agent_id = a.id
         WHERE COALESCE(t.tokens_in, 0) > 0 OR COALESCE(t.tokens_out, 0) > 0 OR COALESCE(t.cost_usd, 0.0) > 0.0
         ORDER BY t.updated_at DESC
         LIMIT 20",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Sqlx)?;

    if !recent_rows.is_empty() || source == "agent_tasks" {
        let mut recent = Vec::with_capacity(recent_rows.len());
        for row in recent_rows {
            recent.push(RecentTokenBurn {
                task_id: row.try_get("task_id").map_err(AppError::Sqlx)?,
                title: row.try_get("title").map_err(AppError::Sqlx)?,
                agent_id: row.try_get("agent_id").map_err(AppError::Sqlx)?,
                agent_name: row.try_get("agent_name").map_err(AppError::Sqlx)?,
                tokens_in: row.try_get("tokens_in").map_err(AppError::Sqlx)?,
                tokens_out: row.try_get("tokens_out").map_err(AppError::Sqlx)?,
                cost_usd: row.try_get("cost_usd").map_err(AppError::Sqlx)?,
                updated_at: decode_timestamp_epoch(&row, "updated_at"),
            });
        }
        return Ok(recent);
    }

    // Fallback: If no tasks exist and source is Agents, synthesize from agents table
    let agent_rows = sqlx::query(
        "SELECT 
            id as agent_id,
            name as agent_name,
            COALESCE(input_tokens, 0) as tokens_in,
            COALESCE(output_tokens, 0) as tokens_out,
            COALESCE(cost_usd, 0.0) as cost_usd
         FROM agents
         WHERE COALESCE(input_tokens, 0) > 0 OR COALESCE(output_tokens, 0) > 0 OR COALESCE(cost_usd, 0.0) > 0.0
         ORDER BY id DESC
         LIMIT 20",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Sqlx)?;

    let now_ts = chrono::Utc::now().timestamp();
    let mut recent = Vec::with_capacity(agent_rows.len());
    for row in agent_rows {
        let aid: String = row.try_get("agent_id").map_err(AppError::Sqlx)?;
        recent.push(RecentTokenBurn {
            task_id: format!("agent-aggregate-{}", aid),
            title: "Agent Aggregate Burn".to_string(),
            agent_id: aid,
            agent_name: row.try_get("agent_name").map_err(AppError::Sqlx)?,
            tokens_in: row.try_get("tokens_in").map_err(AppError::Sqlx)?,
            tokens_out: row.try_get("tokens_out").map_err(AppError::Sqlx)?,
            cost_usd: row.try_get("cost_usd").map_err(AppError::Sqlx)?,
            updated_at: now_ts,
        });
    }

    Ok(recent)
}

/// GET /v1/oversight/token-burn
///
/// Returns an aggregated report of token usage and costs across the swarm.
/// Also updates the Prometheus gauges `oversight_tokens_in`, `oversight_tokens_out`, and `oversight_cost_usd`.
///
/// @docs API_REFERENCE:GetTokenBurn
#[tracing::instrument(skip(state), name = "governance::get_token_burn")]
pub async fn get_token_burn(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let pool = &state.resources.pool;
    let (source, total_tokens_in, total_tokens_out, total_cost_usd) =
        query_overall_token_totals(pool).await?;

    // Update Prometheus gauges
    crate::telemetry::OVERSIGHT_TOKENS_IN.set(total_tokens_in as f64);
    crate::telemetry::OVERSIGHT_TOKENS_OUT.set(total_tokens_out as f64);
    crate::telemetry::OVERSIGHT_COST_USD.set(total_cost_usd);

    let (providers, agents, recent_burns) = tokio::try_join!(
        query_provider_token_burns(pool, source),
        query_agent_token_burns(pool, source),
        query_recent_token_burns(pool, source),
    )?;

    Ok(Json(TokenBurnReport {
        source: source.to_string(),
        total_tokens_in,
        total_tokens_out,
        total_cost_usd,
        providers,
        agents,
        recent_burns,
    }))
}

// Metadata: [telemetry]


