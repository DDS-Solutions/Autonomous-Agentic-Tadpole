//! @docs ARCHITECTURE:Security
//!
//! ### AI Assist Note
//! Cancellation-safe quota transaction and pending-charge settlement.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Storage failure, invalid input, or ownership conflict.
//! - **Telemetry Link**: Trace span `security::budget_flush` in server telemetry.

use super::BudgetGuard;
use anyhow::Result;

impl BudgetGuard {
    /// Flushes all buffered usage metrics to the database.
    ///
    /// This method should be called by a background loop (e.g., every 5-10 seconds)
    /// to ensure eventual consistency while minimizing DB write contention.
    #[tracing::instrument(skip(self), name = "security::budget_flush")]
    pub async fn flush_to_db(&self) -> Result<()> {
        // The owned worker finishes commit and settlement even if its caller is
        // cancelled. No tracking-map registration or runner identity is involved.
        let guard = self.clone();
        tokio::spawn(async move { guard.flush_pending_usage().await }).await?
    }

    async fn flush_pending_usage(&self) -> Result<()> {
        let _settlement = self.settlement.write().await;
        let agent_updates: Vec<_> = self.buffer.iter().filter(|entry| *entry.value() > 0.0)
            .map(|entry| (entry.key().clone(), *entry.value())).collect();
        let mission_updates: Vec<_> = self.mission_buffer.iter().filter(|entry| *entry.value() > 0.0)
            .map(|entry| (entry.key().clone(), *entry.value())).collect();
        if agent_updates.is_empty() && mission_updates.is_empty() { return Ok(()); }
        for (id, _) in &agent_updates { self.get_or_create_quota(id).await?; }
        for (id, _) in &mission_updates { self.get_or_create_mission_quota(id).await?; }

        let mut tx = self.pool.begin().await?;
        for (id, cost) in &agent_updates {
            let updated = sqlx::query("UPDATE agent_quotas SET used_usd = used_usd + ? WHERE entity_id = ?")
                .bind(cost).bind(id).execute(&mut *tx).await?;
            anyhow::ensure!(updated.rows_affected() == 1, "Agent quota disappeared during settlement");
        }
        for (id, cost) in &mission_updates {
            let updated = sqlx::query("UPDATE mission_quotas SET used_usd = used_usd + ? WHERE cluster_id = ?")
                .bind(cost).bind(id).execute(&mut *tx).await?;
            anyhow::ensure!(updated.rows_affected() == 1, "Mission quota disappeared during settlement");
        }
        tx.commit().await?;
        // No await between commit and subtraction. Concurrent new charges remain
        // buffered, and admission cannot observe the transition halfway through.
        for (id, cost) in agent_updates {
            if let Some(mut entry) = self.buffer.get_mut(&id) { *entry = (*entry - cost).max(0.0); }
        }
        for (id, cost) in mission_updates {
            if let Some(mut entry) = self.mission_buffer.get_mut(&id) { *entry = (*entry - cost).max(0.0); }
        }
        Ok(())
    }
}

// Metadata: [settlement]
