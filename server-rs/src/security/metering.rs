//! Resource Consumption Metering & Quota Enforcement
//!
//! Orchestrates the high-fidelity tracking of USD consumption across agents
//! and missions, enforcing hard-budget boundaries to prevent runaway costs.
//!
//! @docs ARCHITECTURE:ResourceMetering
//!
//! ### AI Assist Note
//! **Resource Metering & Quota Enforcement**: Orchestrates the
//! high-fidelity tracking of **USD Consumption** across agents and
//! missions. Enforces **Sovereign Budget Boundaries** by checking
//! available credits before permitting LLM inference. features
//! **Debounced Persistence**: usage is recorded in high-speed
//! thread-safe buffers (`DashMap`) and flushed asynchronously to
//! SQLite (`agent_quotas`, `mission_quotas`) via the `flush_to_db`
//! background loop to minimize write contention (MET-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[metering]` in tracing logs.
//! - **Failure Path**: Budget exhaustion causing 429 errors,
//!   flush-to-db latency causing temporary metric discrepancies,
//!   or incorrect USD-to-token conversions for local models.
//! - **Trace Scope**: `server-rs::security::metering`

use anyhow::Result;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::RwLock;

mod settlement;

/// Defines the resource allocation and consumption limits for an entity (agent or user).
///
/// Tadpole OS uses a prepaid-style credit system where `budget_usd` represents the
/// maximum allowed cost before the entity is throttled or paused.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Quota {
    /// Unique database primary key for the quota entry.
    pub id: String,
    /// The unique identifier for the agent or user being metered.
    pub entity_id: String,
    /// Total allowed budget in USD for the current period.
    pub budget_usd: f64,
    /// Cumulative cost consumed in the current period.
    pub used_usd: f64,
    /// How often the budget resets to zero (e.g., Daily, Monthly, Never).
    pub reset_period: ResetPeriod,
    /// Timestamp of the last successful reset.
    pub last_reset_at: DateTime<Utc>,
    /// Scheduled timestamp for the next automatic reset.
    pub next_reset_at: DateTime<Utc>,
}

/// Specifies the frequency for budget replenishment.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ResetPeriod {
    Daily,
    Monthly,
    /// Budget never resets; essentially a lifetime cap.
    Never,
}

/// A security guard responsible for enforcing LLM cost constraints.
///
/// BudgetGuard prevents model extraction attacks or runaway loops by checking
/// quotas before permitting LLM calls. Actual costs are recorded post-execution
/// into the SQLite persistence layer to ensure high-fidelity financial governance.
#[derive(Clone)]
pub struct BudgetGuard {
    /// Database pool for persistence of usage metrics.
    pool: SqlitePool,
    /// Thread-safe buffer for debounced agent usage updates (entity_id -> accumulated_cost).
    buffer: Arc<DashMap<String, f64>>,
    /// Thread-safe buffer for reserved in-flight cost allocations (A2E-01 2PC lock).
    reserved_buffer: Arc<DashMap<String, f64>>,
    /// Thread-safe buffer for debounced mission usage updates (cluster_id -> accumulated_cost).
    mission_buffer: Arc<DashMap<String, f64>>,
    /// Excludes admission while buffered charges move into committed storage.
    settlement: Arc<RwLock<()>>,
}

impl BudgetGuard {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            buffer: Arc::new(DashMap::new()),
            reserved_buffer: Arc::new(DashMap::new()),
            mission_buffer: Arc::new(DashMap::new()),
            settlement: Arc::new(RwLock::new(())),
        }
    }

    /// Verifies if an entity has sufficient remaining budget to perform a task.
    ///
    /// If the reset time has passed, this method triggers an automatic reset
    /// before performing the check. Returns `true` if (used + buffered + reserved + estimated) <= budget.
    #[tracing::instrument(skip(self), fields(entity_id = %entity_id, cost = estimated_cost), name = "security::budget_check")]
    pub async fn check_budget(&self, entity_id: &str, estimated_cost: f64) -> Result<bool> {
        anyhow::ensure!(estimated_cost.is_finite() && estimated_cost >= 0.0, "Estimated cost must be finite and non-negative");
        let _settlement = self.settlement.read().await;
        let quota = self.get_or_create_quota(entity_id).await?;
        let buffered = self
            .buffer
            .get(entity_id)
            .map(|v| *v.value())
            .unwrap_or(0.0);
        let reserved = self
            .reserved_buffer
            .get(entity_id)
            .map(|v| *v.value())
            .unwrap_or(0.0);

        // Auto-reset if needed
        if Utc::now() >= quota.next_reset_at && quota.reset_period != ResetPeriod::Never {
            self.reset_quota(&quota.id, quota.reset_period).await?;
            // Refresh quota after reset
            let refreshed = self.get_or_create_quota(entity_id).await?;
            return Ok(refreshed.used_usd + buffered + reserved + estimated_cost <= refreshed.budget_usd);
        }

        Ok(quota.used_usd + buffered + reserved + estimated_cost <= quota.budget_usd)
    }

    /// Atomically checks and reserves estimated cost for an in-flight LLM call (A2E-01 2PC Lock).
    pub async fn check_and_reserve_budget(&self, entity_id: &str, estimated_cost: f64) -> Result<bool> {
        anyhow::ensure!(estimated_cost.is_finite() && estimated_cost >= 0.0, "Estimated cost must be finite and non-negative");
        let _settlement = self.settlement.write().await;
        let quota = self.get_or_create_quota(entity_id).await?;
        let buffered = self
            .buffer
            .get(entity_id)
            .map(|v| *v.value())
            .unwrap_or(0.0);

        let mut reserved_entry = self.reserved_buffer.entry(entity_id.to_string()).or_insert(0.0);
        let current_reserved = *reserved_entry;

        if quota.used_usd + buffered + current_reserved + estimated_cost <= quota.budget_usd {
            *reserved_entry += estimated_cost;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Releases a previously reserved cost allocation without committing usage.
    pub fn release_reservation(&self, entity_id: &str, reserved_cost: f64) {
        if let Some(mut entry) = self.reserved_buffer.get_mut(entity_id) {
            *entry.value_mut() = (*entry.value() - reserved_cost).max(0.0);
        }
    }

    /// Records actual cost after an operation completes (Debounced) and releases matching reservation.
    #[tracing::instrument(skip(self), fields(entity_id = %entity_id, cost = cost_usd), name = "security::budget_record")]
    pub async fn record_usage(&self, entity_id: &str, cost_usd: f64) -> Result<()> {
        anyhow::ensure!(cost_usd.is_finite() && cost_usd >= 0.0, "Usage cost must be finite and non-negative");
        *self.buffer.entry(entity_id.to_string()).or_insert(0.0) += cost_usd;
        if let Some(mut entry) = self.reserved_buffer.get_mut(entity_id) {
            *entry.value_mut() = (*entry.value() - cost_usd).max(0.0);
        }
        Ok(())
    }

    /// Atomically reserves budget and returns an RAII guard that automatically releases
    /// the reservation if dropped before being committed.
    pub async fn reserve_scoped(
        &self,
        entity_id: &str,
        estimated_cost: f64,
    ) -> Result<Option<BudgetReservationGuard>> {
        if self.check_and_reserve_budget(entity_id, estimated_cost).await? {
            Ok(Some(BudgetReservationGuard::new(
                self.clone(),
                entity_id,
                estimated_cost,
            )))
        } else {
            Ok(None)
        }
    }

    /// Checks if a mission (cluster) has sufficient remaining budget.
    #[allow(dead_code)]
    pub async fn check_mission_budget(
        &self,
        cluster_id: &str,
        estimated_cost: f64,
    ) -> Result<bool> {
        anyhow::ensure!(estimated_cost.is_finite() && estimated_cost >= 0.0, "Estimated cost must be finite and non-negative");
        let _settlement = self.settlement.read().await;
        let quota = self.get_or_create_mission_quota(cluster_id).await?;
        let buffered = self
            .mission_buffer
            .get(cluster_id)
            .map(|v| *v.value())
            .unwrap_or(0.0);

        // Auto-reset if needed
        if Utc::now() >= quota.next_reset_at && quota.reset_period != ResetPeriod::Never {
            self.reset_mission_quota(&quota.id, quota.reset_period)
                .await?;
            let refreshed = self.get_or_create_mission_quota(cluster_id).await?;
            return Ok(refreshed.used_usd + buffered + estimated_cost <= refreshed.budget_usd);
        }

        Ok(quota.used_usd + buffered + estimated_cost <= quota.budget_usd)
    }

    /// Records usage for a specific mission (Debounced).
    #[allow(dead_code)]
    pub async fn record_mission_usage(&self, cluster_id: &str, cost_usd: f64) -> Result<()> {
        anyhow::ensure!(cost_usd.is_finite() && cost_usd >= 0.0, "Usage cost must be finite and non-negative");
        *self
            .mission_buffer
            .entry(cluster_id.to_string())
            .or_insert(0.0) += cost_usd;
        Ok(())
    }

    /// Calculates global budget and cost stats.
    pub fn get_global_stats(&self) -> (f64, f64) {
        let total_used: f64 = self.buffer.iter().map(|v| *v.value()).sum();
        (100.0, total_used)
    }

    /// Parses a nullable reset period string from SQLite safely without panicking on NULL.
    pub fn parse_reset_period(raw: Option<&str>) -> ResetPeriod {
        match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("daily") => ResetPeriod::Daily,
            Some("monthly") => ResetPeriod::Monthly,
            Some("never") | None | Some("") => ResetPeriod::Never,
            Some(other) => {
                tracing::warn!(
                    reset_period = %other,
                    "Unknown quota reset_period encountered in database; defaulting to Never"
                );
                ResetPeriod::Never
            }
        }
    }

    fn decode_quota_timestamp(row: &sqlx::sqlite::SqliteRow, col: &str) -> Result<DateTime<Utc>> {
        use sqlx::Row;
        if let Ok(Some(dt)) = row.try_get::<Option<DateTime<Utc>>, _>(col) {
            return Ok(dt);
        }
        if let Ok(Some(raw)) = row.try_get::<Option<String>, _>(col) {
            let trimmed = raw.trim();
            if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
                return Ok(dt.with_timezone(&Utc));
            }
            if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
                return Ok(naive.and_utc());
            }
        }
        Ok(Utc::now())
    }

    /// Fetches all registered agent quotas, including in-memory buffered usage.
    #[allow(dead_code)]
    pub async fn get_all_quotas(&self) -> Result<Vec<Quota>> {
        let rows = sqlx::query(
            "SELECT id, entity_id, budget_usd, used_usd, reset_period, last_reset_at, next_reset_at FROM agent_quotas ORDER BY entity_id ASC LIMIT 1000",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::with_capacity(rows.len());
        for r in rows {
            use sqlx::Row;
            let period_opt: Option<String> = r.try_get("reset_period")?;
            let period = Self::parse_reset_period(period_opt.as_deref());
            let entity_id: String = r.try_get("entity_id")?;
            let db_used: f64 = r.try_get::<Option<f64>, _>("used_usd")?.unwrap_or(0.0);
            let buffered = self.buffer.get(&entity_id).map(|v| *v.value()).unwrap_or(0.0);

            results.push(Quota {
                id: r.try_get::<Option<String>, _>("id")?.unwrap_or_else(|| entity_id.clone()),
                entity_id,
                budget_usd: r.try_get::<Option<f64>, _>("budget_usd")?.unwrap_or(0.0),
                used_usd: db_used + buffered,
                reset_period: period,
                last_reset_at: Self::decode_quota_timestamp(&r, "last_reset_at")?,
                next_reset_at: Self::decode_quota_timestamp(&r, "next_reset_at")?,
            });
        }
        Ok(results)
    }

    /// Fetches all registered mission quotas from the authoritative `BudgetGuard` store,
    /// including any debounced in-memory `mission_buffer` usage.
    pub async fn get_all_mission_quotas(&self) -> Result<Vec<Quota>> {
        let rows = sqlx::query(
            "SELECT id, cluster_id, budget_usd, used_usd, reset_period, last_reset_at, next_reset_at FROM mission_quotas ORDER BY cluster_id ASC LIMIT 1000",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::with_capacity(rows.len());
        for r in rows {
            use sqlx::Row;
            let period_opt: Option<String> = r.try_get("reset_period")?;
            let period = Self::parse_reset_period(period_opt.as_deref());
            let cluster_id: String = r.try_get("cluster_id")?;
            let db_used: f64 = r.try_get::<Option<f64>, _>("used_usd")?.unwrap_or(0.0);
            let buffered = self
                .mission_buffer
                .get(&cluster_id)
                .map(|v| *v.value())
                .unwrap_or(0.0);

            results.push(Quota {
                id: r
                    .try_get::<Option<String>, _>("id")?
                    .unwrap_or_else(|| cluster_id.clone()),
                entity_id: cluster_id,
                budget_usd: r.try_get::<Option<f64>, _>("budget_usd")?.unwrap_or(0.0),
                used_usd: db_used + buffered,
                reset_period: period,
                last_reset_at: Self::decode_quota_timestamp(&r, "last_reset_at")?,
                next_reset_at: Self::decode_quota_timestamp(&r, "next_reset_at")?,
            });
        }
        Ok(results)
    }

    /// Updates an entity's quota (inserting the row if it does not yet exist).
    #[allow(dead_code)]
    #[tracing::instrument(skip(self), fields(entity_id = %entity_id, budget = budget_usd), name = "security::budget_update")]
    pub async fn update_quota(
        &self,
        entity_id: &str,
        budget_usd: f64,
        reset_period: Option<ResetPeriod>,
    ) -> Result<()> {
        let updated = if let Some(period) = reset_period {
            let period_str = match period {
                ResetPeriod::Daily => "daily",
                ResetPeriod::Monthly => "monthly",
                ResetPeriod::Never => "never",
            };

            sqlx::query(
                "UPDATE agent_quotas SET budget_usd = ?1, reset_period = ?2 WHERE entity_id = ?3",
            )
            .bind(budget_usd)
            .bind(period_str)
            .bind(entity_id)
            .execute(&self.pool)
            .await?
        } else {
            sqlx::query("UPDATE agent_quotas SET budget_usd = ?1 WHERE entity_id = ?2")
                .bind(budget_usd)
                .bind(entity_id)
                .execute(&self.pool)
                .await?
        };

        if updated.rows_affected() == 0 {
            let period_str = match reset_period.unwrap_or(ResetPeriod::Daily) {
                ResetPeriod::Daily => "daily",
                ResetPeriod::Monthly => "monthly",
                ResetPeriod::Never => "never",
            };
            let now = Utc::now();
            sqlx::query(
                "INSERT INTO agent_quotas (id, entity_id, budget_usd, used_usd, reset_period, last_reset_at, next_reset_at) \
                 VALUES (?1, ?2, ?3, 0.0, ?4, ?5, ?6)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(entity_id)
            .bind(budget_usd)
            .bind(period_str)
            .bind(now)
            .bind(now + chrono::Duration::days(1))
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// Updates a mission's quota.
    #[allow(dead_code)]
    pub async fn update_mission_quota(
        &self,
        cluster_id: &str,
        budget_usd: f64,
        reset_period: Option<ResetPeriod>,
    ) -> Result<()> {
        let period_str = match reset_period.unwrap_or(ResetPeriod::Daily) {
            ResetPeriod::Daily => "daily",
            ResetPeriod::Monthly => "monthly",
            ResetPeriod::Never => "never",
        };

        sqlx::query(
            "INSERT INTO mission_quotas (id, cluster_id, budget_usd, used_usd, reset_period, last_reset_at, next_reset_at) \
             VALUES (?1, ?2, ?3, 0.0, ?4, ?5, ?6) \
             ON CONFLICT(cluster_id) DO UPDATE SET budget_usd = ?3, reset_period = ?4"
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(cluster_id)
        .bind(budget_usd)
        .bind(period_str)
        .bind(Utc::now())
        .bind(Utc::now() + chrono::Duration::days(1))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mock budget guard for tests
    pub fn mock() -> Self {
        Self::new(sqlx::SqlitePool::connect_lazy("sqlite::memory:").expect("Tadpole OS Metering: Failed to connect to in-memory database for mock BudgetGuard."))
    }

    async fn get_or_create_quota(&self, entity_id: &str) -> Result<Quota> {
        let row = sqlx::query(
            "SELECT id, entity_id, budget_usd, used_usd, reset_period, last_reset_at, next_reset_at FROM agent_quotas WHERE entity_id = ?1",
        )
        .bind(entity_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(r) = row {
            use sqlx::Row;
            let period_str: String = r.get("reset_period");
            let period = match period_str.as_str() {
                "daily" => ResetPeriod::Daily,
                "monthly" => ResetPeriod::Monthly,
                _ => ResetPeriod::Never,
            };

            Ok(Quota {
                id: r.get("id"),
                entity_id: r.get("entity_id"),
                budget_usd: r.get("budget_usd"),
                used_usd: r.get("used_usd"),
                reset_period: period,
                last_reset_at: r.get("last_reset_at"),
                next_reset_at: r.get("next_reset_at"),
            })
        } else {
            // Create default daily quota ($0.50)
            let id = uuid::Uuid::new_v4().to_string();
            let now = Utc::now();
            let next_reset = now + chrono::Duration::days(1);

            sqlx::query(
                "INSERT INTO agent_quotas (id, entity_id, budget_usd, used_usd, reset_period, last_reset_at, next_reset_at) \
                 VALUES (?1, ?2, 0.50, 0.0, 'daily', ?3, ?4)"
            )
            .bind(&id)
            .bind(entity_id)
            .bind(now)
            .bind(next_reset)
            .execute(&self.pool)
            .await?;

            Ok(Quota {
                id,
                entity_id: entity_id.to_string(),
                budget_usd: 0.50,
                used_usd: 0.0,
                reset_period: ResetPeriod::Daily,
                last_reset_at: now,
                next_reset_at: next_reset,
            })
        }
    }

    async fn reset_quota(&self, id: &str, period: ResetPeriod) -> Result<()> {
        let now = Utc::now();
        let next_reset = match period {
            ResetPeriod::Daily => now + chrono::Duration::days(1),
            ResetPeriod::Monthly => now + chrono::Duration::days(30),
            ResetPeriod::Never => now + chrono::Duration::days(365 * 100),
        };

        sqlx::query(
            "UPDATE agent_quotas SET used_usd = 0.0, last_reset_at = ?1, next_reset_at = ?2 WHERE id = ?3"
        )
        .bind(now)
        .bind(next_reset)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_or_create_mission_quota(&self, cluster_id: &str) -> Result<Quota> {
        let row = sqlx::query(
            "SELECT id, cluster_id, budget_usd, used_usd, reset_period, last_reset_at, next_reset_at FROM mission_quotas WHERE cluster_id = ?1",
        )
        .bind(cluster_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(r) = row {
            use sqlx::Row;
            let period_str: String = r.get("reset_period");
            let period = match period_str.as_str() {
                "daily" => ResetPeriod::Daily,
                "monthly" => ResetPeriod::Monthly,
                _ => ResetPeriod::Never,
            };

            Ok(Quota {
                id: r.get("id"),
                entity_id: r.get("cluster_id"),
                budget_usd: r.get("budget_usd"),
                used_usd: r.get("used_usd"),
                reset_period: period,
                last_reset_at: r.get("last_reset_at"),
                next_reset_at: r.get("next_reset_at"),
            })
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            let now = Utc::now();
            let next_reset = now + chrono::Duration::days(1);

            sqlx::query(
                "INSERT INTO mission_quotas (id, cluster_id, budget_usd, used_usd, reset_period, last_reset_at, next_reset_at) \
                 VALUES (?1, ?2, 5.0, 0.0, 'daily', ?3, ?4)"
            )
            .bind(&id)
            .bind(cluster_id)
            .bind(now)
            .bind(next_reset)
            .execute(&self.pool)
            .await?;

            Ok(Quota {
                id,
                entity_id: cluster_id.to_string(),
                budget_usd: 5.0,
                used_usd: 0.0,
                reset_period: ResetPeriod::Daily,
                last_reset_at: now,
                next_reset_at: next_reset,
            })
        }
    }

    async fn reset_mission_quota(&self, id: &str, period: ResetPeriod) -> Result<()> {
        let now = Utc::now();
        let next_reset = match period {
            ResetPeriod::Daily => now + chrono::Duration::days(1),
            ResetPeriod::Monthly => now + chrono::Duration::days(30),
            ResetPeriod::Never => now + chrono::Duration::days(365 * 100),
        };

        sqlx::query(
            "UPDATE mission_quotas SET used_usd = 0.0, last_reset_at = ?1, next_reset_at = ?2 WHERE id = ?3"
        )
        .bind(now)
        .bind(next_reset)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

/// RAII Guard that holds an in-flight 2PC budget reservation.
/// If dropped before being explicitly committed, it automatically releases the reservation.
pub struct BudgetReservationGuard {
    guard: BudgetGuard,
    entity_id: String,
    reserved_cost: f64,
    committed: bool,
}

impl BudgetReservationGuard {
    pub fn new(guard: BudgetGuard, entity_id: impl Into<String>, reserved_cost: f64) -> Self {
        Self {
            guard,
            entity_id: entity_id.into(),
            reserved_cost,
            committed: false,
        }
    }

    /// Marks the reservation as committed (e.g., when usage is recorded), preventing rollback on drop.
    pub fn commit(mut self) {
        self.committed = true;
    }

    /// Returns the reserved dollar amount.
    pub fn reserved_cost(&self) -> f64 {
        self.reserved_cost
    }
}

impl Drop for BudgetReservationGuard {
    fn drop(&mut self) {
        if !self.committed && self.reserved_cost > 0.0 {
            self.guard.release_reservation(&self.entity_id, self.reserved_cost);
            tracing::debug!(
                "🔓 [BudgetGuard] RAII auto-released uncommitted reservation of ${:.6} for entity '{}'",
                self.reserved_cost,
                self.entity_id
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn settlement_fixture() -> Result<BudgetGuard> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await?;
        for (table, identity) in [("agent_quotas", "entity_id"), ("mission_quotas", "cluster_id")] {
            sqlx::query(&format!("CREATE TABLE {table} (id TEXT PRIMARY KEY, {identity} TEXT UNIQUE, used_usd REAL, budget_usd REAL, last_reset_at TEXT, next_reset_at TEXT, reset_period TEXT)"))
                .execute(&pool).await?;
            sqlx::query(&format!("INSERT INTO {table} VALUES ('quota', 'settlement', 0.0, 0.5, '2024-01-01T00:00:00Z', '2124-01-01T00:00:00Z', 'never')"))
                .execute(&pool).await?;
        }
        Ok(BudgetGuard::new(pool))
    }

    #[tokio::test]
    async fn finding_regression_settlement_failure_preserves_agent_and_mission_charges_until_retry() -> Result<()> {
        for rejected_table in ["agent_quotas", "mission_quotas"] {
            let guard = settlement_fixture().await?;
            guard.record_usage("settlement", 0.6).await?;
            guard.record_mission_usage("settlement", 0.6).await?;
            sqlx::query(&format!("CREATE TRIGGER reject_settlement BEFORE UPDATE ON {rejected_table} BEGIN SELECT RAISE(ABORT, 'injected settlement failure'); END"))
                .execute(&guard.pool).await?;
            assert!(guard.flush_to_db().await.is_err());
            assert!(!guard.check_budget("settlement", 0.1).await?);
            assert!(!guard.check_mission_budget("settlement", 0.1).await?);
            for table in ["agent_quotas", "mission_quotas"] {
                assert_eq!(sqlx::query_scalar::<_, f64>(&format!("SELECT used_usd FROM {table}"))
                    .fetch_one(&guard.pool).await?, 0.0);
            }
            sqlx::query("DROP TRIGGER reject_settlement").execute(&guard.pool).await?;
            guard.flush_to_db().await?;
            guard.flush_to_db().await?;
            for table in ["agent_quotas", "mission_quotas"] {
                assert_eq!(sqlx::query_scalar::<_, f64>(&format!("SELECT used_usd FROM {table}"))
                    .fetch_one(&guard.pool).await?, 0.6);
            }
            assert_eq!(guard.get_global_stats().1, 0.0);
        }
        Ok(())
    }

    #[tokio::test]
    async fn finding_regression_settlement_caller_cancellation_preserves_new_charges_and_excludes_admission() -> Result<()> {
        let guard = settlement_fixture().await?;
        guard.record_usage("settlement", 0.6).await?;
        let connection = guard.pool.acquire().await?;
        let caller_guard = guard.clone();
        let caller = tokio::spawn(async move { caller_guard.flush_to_db().await });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while guard.settlement.try_read().is_ok() { tokio::task::yield_now().await; }
        }).await?;
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        guard.record_usage("settlement", 0.1).await?;
        let check_guard = guard.clone();
        let admission = tokio::spawn(async move { check_guard.check_budget("settlement", 0.1).await });
        tokio::task::yield_now().await;
        assert!(!admission.is_finished(), "Admission must wait for durable settlement");
        drop(connection);
        assert!(!tokio::time::timeout(std::time::Duration::from_secs(5), admission).await???);
        guard.flush_to_db().await?;
        let used: f64 = sqlx::query_scalar("SELECT used_usd FROM agent_quotas").fetch_one(&guard.pool).await?;
        assert!((used - 0.7).abs() < 1e-12);
        assert_eq!(guard.get_global_stats().1, 0.0);
        Ok(())
    }

    #[tokio::test]
    async fn finding_regression_settlement_competing_flushes_charge_once_and_create_missing_quota() -> Result<()> {
        let guard = settlement_fixture().await?;
        guard.record_usage("new-entity", 0.2).await?;
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let guard = guard.clone();
            tasks.push(tokio::spawn(async move { guard.flush_to_db().await }));
        }
        for task in tasks { task.await??; }
        let used: f64 = sqlx::query_scalar("SELECT used_usd FROM agent_quotas WHERE entity_id = 'new-entity'").fetch_one(&guard.pool).await?;
        assert_eq!(used, 0.2);
        assert_eq!(guard.get_global_stats().1, 0.0);
        Ok(())
    }

    #[tokio::test]
    async fn finding_regression_settlement_rejects_invalid_costs_before_recording_or_admission() {
        let guard = BudgetGuard::mock();
        for cost in [f64::NAN, f64::INFINITY, -0.01] {
            assert!(guard.record_usage("agent", cost).await.is_err());
            assert!(guard.record_mission_usage("mission", cost).await.is_err());
            assert!(guard.check_budget("agent", cost).await.is_err());
            assert!(guard.check_and_reserve_budget("agent", cost).await.is_err());
            assert!(guard.check_mission_budget("mission", cost).await.is_err());
        }
        assert_eq!(guard.get_global_stats().1, 0.0);
    }

    #[tokio::test]
    async fn test_quota_enforcement() -> Result<()> {
        let pool = SqlitePool::connect("sqlite::memory:").await?;
        sqlx::query("CREATE TABLE agent_quotas (id TEXT PRIMARY KEY, entity_id TEXT, used_usd REAL, budget_usd REAL, last_reset_at TEXT, next_reset_at TEXT, reset_period TEXT)")
            .execute(&pool).await?;

        let guard = BudgetGuard::new(pool);

        // 1. Add agent with 1.0 budget manually
        sqlx::query("INSERT INTO agent_quotas (id, entity_id, used_usd, budget_usd, reset_period, last_reset_at, next_reset_at) VALUES ('1', 'agent_1', 0.0, 1.0, 'Never', '2024-01-01 00:00:00', '2124-01-01 00:00:00')")
            .execute(&guard.pool).await?;

        assert!(guard.check_budget("agent_1", 0.01).await?);

        // 3. Record usage nearly hitting budget
        guard.record_usage("agent_1", 0.90).await?;
        assert!(guard.check_budget("agent_1", 0.05).await?);

        // 4. Hit budget
        guard.record_usage("agent_1", 0.10).await?; // Total used: 1.0
        assert!(!guard.check_budget("agent_1", 0.01).await?); // 1.01 > 1.0

        Ok(())
    }

    #[tokio::test]
    async fn test_debounced_flushing() -> Result<()> {
        let pool = SqlitePool::connect("sqlite::memory:").await?;
        sqlx::query("CREATE TABLE agent_quotas (id TEXT PRIMARY KEY, entity_id TEXT, used_usd REAL, budget_usd REAL, last_reset_at TEXT, next_reset_at TEXT, reset_period TEXT)")
            .execute(&pool).await?;
        sqlx::query("CREATE TABLE mission_quotas (id TEXT PRIMARY KEY, cluster_id TEXT, used_usd REAL, budget_usd REAL, last_reset_at TEXT, next_reset_at TEXT, reset_period TEXT)")
            .execute(&pool).await?;

        let guard = BudgetGuard::new(pool);

        // 1. Setup agent & mission
        sqlx::query("INSERT INTO agent_quotas (id, entity_id, used_usd, budget_usd, reset_period, last_reset_at, next_reset_at) VALUES ('1', 'agent_flush', 0.0, 10.0, 'Never', '2024-01-01 00:00:00', '2124-01-01 00:00:00')")
            .execute(&guard.pool).await?;
        sqlx::query("INSERT INTO mission_quotas (id, cluster_id, used_usd, budget_usd, reset_period, last_reset_at, next_reset_at) VALUES ('2', 'cluster_flush', 0.0, 10.0, 'Never', '2024-01-01 00:00:00', '2124-01-01 00:00:00')")
            .execute(&guard.pool).await?;

        // 2. Record usage (should only hit buffers)
        guard.record_usage("agent_flush", 1.5).await?;
        guard.record_usage("agent_flush", 2.5).await?;
        guard.record_mission_usage("cluster_flush", 5.0).await?;

        // Verify DB hasn't changed yet
        let agent_used: f64 =
            sqlx::query_scalar("SELECT used_usd FROM agent_quotas WHERE entity_id = 'agent_flush'")
                .fetch_one(&guard.pool)
                .await?;
        let mission_used: f64 = sqlx::query_scalar(
            "SELECT used_usd FROM mission_quotas WHERE cluster_id = 'cluster_flush'",
        )
        .fetch_one(&guard.pool)
        .await?;
        assert_eq!(agent_used, 0.0);
        assert_eq!(mission_used, 0.0);

        // 3. Flush
        guard.flush_to_db().await?;

        // Verify DB updated
        let agent_used_after: f64 =
            sqlx::query_scalar("SELECT used_usd FROM agent_quotas WHERE entity_id = 'agent_flush'")
                .fetch_one(&guard.pool)
                .await?;
        let mission_used_after: f64 = sqlx::query_scalar(
            "SELECT used_usd FROM mission_quotas WHERE cluster_id = 'cluster_flush'",
        )
        .fetch_one(&guard.pool)
        .await?;

        assert_eq!(agent_used_after, 4.0);
        assert_eq!(mission_used_after, 5.0);

        // Verify buffers are reset
        assert_eq!(*guard.buffer.get("agent_flush").unwrap(), 0.0);
        assert_eq!(*guard.mission_buffer.get("cluster_flush").unwrap(), 0.0);

        Ok(())
    }
}





// Metadata: [metering]
