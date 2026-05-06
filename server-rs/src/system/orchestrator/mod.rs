//! @docs ARCHITECTURE:Autonomy
//!
//! ### Autonomous Orchestrator
//! The primary intelligence loop of the Sovereign Kernel.
//! Periodically scans the environment, evaluates system health, 
//! and dispatches agents to fulfill the Sovereign Charter.

use crate::state::AppState;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{info, warn, error, debug};

pub struct Orchestrator {
    app_state: Arc<AppState>,
}

impl Orchestrator {
    pub fn new(app_state: Arc<AppState>) -> Self {
        Self { app_state }
    }

    /// Primary execution loop for the Orchestrator.
    pub async fn run(self) {
        info!("👁️ [Orchestrator] Autonomous loop started. Governance: Aletheia Protocol active.");

        let mut ticker = interval(Duration::from_secs(60)); // Scan every minute

        loop {
            ticker.tick().await;
            
            if let Err(e) = self.scan_and_act().await {
                error!("🚨 [Orchestrator] Scan failed: {:?}", e);
            }
        }
    }

    /// Scans the system for tasks, faults, or optimization opportunities.
    async fn scan_and_act(&self) -> Result<(), crate::error::AppError> {
        info!("🔍 [Orchestrator] Initiating autonomous system scan...");

        // 1. Discovery: Autonomous Skill Scan
        match self.app_state.scan_workspace_skills_sovereign().await {
            Ok(count) if count > 0 => {
                info!("✨ [Orchestrator] Discovered {} new capabilities.", count);
            },
            Ok(_) => debug!("[Orchestrator] No new capabilities found."),
            Err(e) => error!("🚨 [Orchestrator] Autonomous skill scan failed: {:?}", e),
        }

        // 2. Health & Governance: Reap stale agents/missions
        self.reap_and_audit_stale_missions().await?;

        // 3. System Awareness: Generate Manifest
        let manifest = crate::system::manifest::SovereignStateManifest::generate(&self.app_state).await;
        debug!("📊 [Orchestrator] System Manifest: {}", manifest);

        Ok(())
    }

    /// Identifies and reaps missions or agents that have stalled.
    async fn reap_and_audit_stale_missions(&self) -> Result<(), crate::error::AppError> {
        let pool = &self.app_state.resources.pool;
        
        // Reap agents who haven't sent a heartbeat in 5 minutes (300s)
        let reaped_count = crate::agent::persistence::reap_stale_agents(pool, 300).await?;
        if reaped_count > 0 {
            warn!("♻️ [Orchestrator] Harvested {} stale agents. Logging systemic drift to AuditActor.", reaped_count);
            // Future: Push to FAULT_REGISTRY table here
        }

        // Also fail missions that have been active but have no associated "busy" agents
        // (i.e., the agent process died without the mission finishing)
        let ghost_missions: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM mission_history 
             WHERE status = 'active' 
             AND id NOT IN (SELECT active_mission FROM agents WHERE status = 'busy')"
        )
        .fetch_all(pool)
        .await
        .map_err(crate::error::AppError::Sqlx)?;

        for mid in ghost_missions {
            warn!("👻 [Orchestrator] Detected ghost mission: {}. Marking as failed.", mid);
            sqlx::query("UPDATE mission_history SET status = 'failed' WHERE id = ?")
                .bind(&mid)
                .execute(pool)
                .await
                .map_err(crate::error::AppError::Sqlx)?;
        }

        Ok(())
    }
}
