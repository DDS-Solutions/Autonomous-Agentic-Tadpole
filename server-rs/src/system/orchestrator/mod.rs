//! @docs ARCHITECTURE:Infrastructure
//! 
//! ### AI Assist Note
//! **! @docs ARCHITECTURE:Autonomy**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[mod]` in tracing logs.

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

        // 2.5. Resource Optimization: Deprovision idle agents
        if let Err(e) = self.deprovision_idle_agents().await {
            error!("🚨 [Orchestrator] Automated agent deprovisioning failed: {:?}", e);
        }

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

        // (i.e., the agent process died without the mission finishing)
        // [Grace Period]: Ignore missions created in the last 30 seconds to allow for 
        // high-concurrency setup/handshake logic (GHOST-02).
        let ghost_missions: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM mission_history 
             WHERE status = 'active' 
             AND created_at < datetime('now', '-30 seconds')
             AND id NOT IN (
                 SELECT json_extract(active_mission, '$.id') 
                 FROM agents 
                 WHERE status IN ('busy', 'active', 'thinking')
             )"
        )
        .fetch_all(pool)
        .await
        .map_err(crate::error::AppError::Sqlx)?;

        for mid in ghost_missions {
            // CRITICAL SECOND CHECK: Verify against live in-memory registry
            // The DB might be lagging during high-load swarm recruitment.
            let mut is_truly_ghost = true;
            for entry in self.app_state.registry.agents.iter() {
                let agent = entry.value();
                if let Some(active_mission) = &agent.state.active_mission {
                    if let Some(agent_mid) = active_mission.get("id").and_then(|v| v.as_str()) {
                        if agent_mid == mid {
                            debug!("🛡️ [Orchestrator] Ghost mission candidate {} is active in registry for agent {}. Skipping reap.", mid, agent.identity.id);
                            is_truly_ghost = false;
                            break;
                        }
                    }
                }
            }

            if is_truly_ghost {
                warn!("👻 [Orchestrator] Detected ghost mission: {}. Marking as failed.", mid);
                sqlx::query("UPDATE mission_history SET status = 'failed' WHERE id = ?")
                    .bind(&mid)
                    .execute(pool)
                    .await
                    .map_err(crate::error::AppError::Sqlx)?;
            }
        }

        Ok(())
    }

    /// Automatically de-provisions idle agents with zero token utilization over a rolling 48-hour window.
    async fn deprovision_idle_agents(&self) -> Result<(), crate::error::AppError> {
        let pool = &self.app_state.resources.pool;
        
        let slept_count = sqlx::query(
            "UPDATE agents 
             SET status = 'offline' 
             WHERE status = 'idle' 
             AND tokens_used = 0 
             AND (created_at IS NULL OR created_at < datetime('now', '-48 hours'))"
        )
        .execute(pool)
        .await
        .map_err(crate::error::AppError::Sqlx)?
        .rows_affected();

        if slept_count > 0 {
            warn!("♻️ [Orchestrator] Auto-slept {} idle agents with zero token utilization over a rolling 48-hour window.", slept_count);
        }

        Ok(())
    }
}

// Metadata: [mod]
