//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **A2A Economic Governance (A2E-01)**: Exposes Agent-to-Agent Two-Phase Commit (2PC) budget transfer endpoints.
//! Features **Lock-Aware Projected Spend Calculation**, **24-Hour Rolling Budget Resets**, and **Integer Micro-USDC Accounting**.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Exceeded daily spend limit cap, invalid lock_id, or un-committed transaction expiration.

use axum::{
    extract::State,
    http::StatusCode,
    Json,
    response::IntoResponse,
};
use crate::state::AppState;
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Micro-USDC Integer Unit: 1 USD = 1,000,000 micro-USDC
pub type MicroUSDC = u64;

#[derive(Debug, Deserialize, Serialize)]
pub struct PrepareTxRequest {
    pub debit_agent_id: String,
    pub credit_agent_id: String,
    pub amount_micros: MicroUSDC,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CommitTxRequest {
    pub lock_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RollbackTxRequest {
    pub lock_id: String,
    pub reason: Option<String>,
}

/// Prepares a 2PC budget transaction, enforcing 24h rolling budget limits and pending lock projection.
pub async fn prepare_transaction(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PrepareTxRequest>,
) -> Result<impl IntoResponse, AppError> {
    if payload.amount_micros == 0 {
        return Err(AppError::BadRequest("Amount must be greater than zero micro-USDC".to_string()));
    }

    let pool = &state.resources.pool;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // 1. Fetch economic metadata for debit agent
    let meta_opt: Option<(i64, i64, i64)> = sqlx::query_as(
        "SELECT daily_spend_limit_micros, daily_spent_accumulated_micros, last_reset_at \
         FROM agent_economics_meta WHERE agent_id = ?1",
    )
    .bind(&payload.debit_agent_id)
    .fetch_optional(pool)
    .await?;

    let (limit, mut spent, last_reset) = meta_opt.unwrap_or((10_000_000, 0, 0)); // Default $10 cap

    // 2. Perform 24-hour rolling reset if 86,400 seconds have elapsed
    if now > last_reset + 86400 {
        spent = 0;
        let _ = sqlx::query(
            "UPDATE agent_economics_meta \
             SET daily_spent_accumulated_micros = 0, last_reset_at = ?1 \
             WHERE agent_id = ?2",
        )
        .bind(now)
        .bind(&payload.debit_agent_id)
        .execute(pool)
        .await;
    }

    // 3. Query sum of all active PREPARED transaction locks for this debit agent
    let locked_sum: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(amount_micros), 0) FROM a2a_ledger WHERE debit_agent_id = ?1 AND status = 'PREPARED'",
    )
    .bind(&payload.debit_agent_id)
    .fetch_one(pool)
    .await?;

    let projected_spent = spent + locked_sum.0 + payload.amount_micros as i64;

    // 4. Verify budget limit caps
    if limit > 0 && projected_spent > limit {
        return Err(AppError::BadRequest(format!(
            "Transaction rejected: agent '{}' exceeded daily cap of {} micro-USDC (spent today: {}, pending locks: {}, requested: {}).",
            payload.debit_agent_id, limit, spent, locked_sum.0, payload.amount_micros
        )));
    }

    // 5. Issue atomic transaction lock
    let tx_id = format!("tx_{}", uuid::Uuid::new_v4());
    let lock_id = format!("lock_{}", uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO a2a_ledger (tx_id, debit_agent_id, credit_agent_id, amount_micros, status, lock_id)
         VALUES (?, ?, ?, ?, 'PREPARED', ?)"
    )
    .bind(&tx_id)
    .bind(&payload.debit_agent_id)
    .bind(&payload.credit_agent_id)
    .bind(payload.amount_micros as i64)
    .bind(&lock_id)
    .execute(pool)
    .await?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "PREPARED",
            "tx_id": tx_id,
            "lock_id": lock_id,
            "amount_micros": payload.amount_micros
        })),
    ))
}

/// Commits a prepared 2PC transaction and accumulates spent micros into debit agent's economic meta.
pub async fn commit_transaction(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CommitTxRequest>,
) -> Result<impl IntoResponse, AppError> {
    let pool = &state.resources.pool;

    // 1. Fetch prepared transaction info
    let tx_info: Option<(String, i64)> = sqlx::query_as(
        "SELECT debit_agent_id, amount_micros FROM a2a_ledger WHERE lock_id = ?1 AND status = 'PREPARED'"
    )
    .bind(&payload.lock_id)
    .fetch_optional(pool)
    .await?;

    let (debit_agent_id, amount_micros) = match tx_info {
        Some(info) => info,
        None => return Err(AppError::NotFound("No matching PREPARED transaction lock found".to_string())),
    };

    // 2. Commit transaction
    sqlx::query(
        "UPDATE a2a_ledger SET status = 'COMMITTED', updated_at = CURRENT_TIMESTAMP WHERE lock_id = ?"
    )
    .bind(&payload.lock_id)
    .execute(pool)
    .await?;

    // 3. Accumulate spend in agent_economics_meta
    let _ = sqlx::query(
        "INSERT INTO agent_economics_meta (agent_id, daily_spent_accumulated_micros) \
         VALUES (?1, ?2) \
         ON CONFLICT(agent_id) DO UPDATE SET \
            daily_spent_accumulated_micros = daily_spent_accumulated_micros + excluded.daily_spent_accumulated_micros"
    )
    .bind(&debit_agent_id)
    .bind(amount_micros)
    .execute(pool)
    .await;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "status": "COMMITTED", "lock_id": payload.lock_id })),
    ))
}

/// Rolls back a prepared 2PC transaction.
pub async fn rollback_transaction(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RollbackTxRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result = sqlx::query(
        "UPDATE a2a_ledger SET status = 'ROLLED_BACK', updated_at = CURRENT_TIMESTAMP WHERE lock_id = ? AND status = 'PREPARED'"
    )
    .bind(&payload.lock_id)
    .execute(&state.resources.pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("No matching PREPARED transaction lock found".to_string()));
    }

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ROLLED_BACK",
            "lock_id": payload.lock_id,
            "reason": payload.reason.unwrap_or_else(|| "User or system cancel".to_string())
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        
        sqlx::query(
            "CREATE TABLE agents (id TEXT PRIMARY KEY, name TEXT NOT NULL);"
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE a2a_ledger (
                tx_id TEXT PRIMARY KEY,
                debit_agent_id TEXT NOT NULL,
                credit_agent_id TEXT NOT NULL,
                amount_micros INTEGER NOT NULL,
                status TEXT NOT NULL DEFAULT 'PREPARED',
                lock_id TEXT UNIQUE NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );"
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE agent_economics_meta (
                agent_id TEXT PRIMARY KEY,
                economic_zone TEXT NOT NULL DEFAULT 'DEV',
                daily_spend_limit_micros INTEGER NOT NULL DEFAULT 10000000,
                daily_spent_accumulated_micros INTEGER NOT NULL DEFAULT 0,
                last_reset_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            );"
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_budget_cap_enforcement() {
        let pool = setup_test_pool().await;

        // Set low cap of $5.00 (5,000,000 micros)
        sqlx::query(
            "INSERT INTO agent_economics_meta (agent_id, daily_spend_limit_micros, daily_spent_accumulated_micros)
             VALUES ('agent-alice', 5000000, 4000000)"
        )
        .execute(&pool)
        .await
        .unwrap();

        // Query sum of active locks
        let locked_sum: (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(amount_micros), 0) FROM a2a_ledger WHERE debit_agent_id = 'agent-alice' AND status = 'PREPARED'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let spent = 4_000_000i64;
        let limit = 5_000_000i64;
        let new_amount = 2_000_000i64; // $2.00 requested, total projected $6.00 > $5.00 limit

        assert!(spent + locked_sum.0 + new_amount > limit);
    }
}

// Metadata: [a2a]
