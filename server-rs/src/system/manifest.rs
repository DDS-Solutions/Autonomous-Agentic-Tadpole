//! @docs ARCHITECTURE:Infrastructure
//! 
//! ### AI Assist Note
//! **! @docs ARCHITECTURE:SovereignKernel**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[manifest]` in tracing logs.
//! 
//! @docs ARCHITECTURE:SovereignKernel
//!
//! ### Sovereign State Manifest
//! Generates a high-density, LLM-readable summary of the swarm's current state.
//! Used to anchor agent reasoning and history compaction to physical reality.

use crate::state::AppState;
use std::sync::Arc;

pub struct SovereignStateManifest;

impl SovereignStateManifest {
    /// Compiles system-wide telemetry into a dense manifest string.
    pub async fn generate(state: &Arc<AppState>) -> String {
        // 1. Swarm Vitality
        let active_agents = state.governance.active_agents.load(std::sync::atomic::Ordering::Relaxed);
        let registered_agents = state.registry.agents.len();
        let idle_agents = registered_agents.saturating_sub(active_agents as usize);
        
        // 2. Financial Governance
        let (total_budget, total_cost) = state.security.budget_guard.get_global_stats();
        let remaining_budget = total_budget - total_cost;

        // 3. Capability Pulse
        let snapshot = state.registry.skills.snapshot();
        let script_skills = snapshot.skills.len();
        let workflow_skills = snapshot.workflows.len();
        let hook_skills = snapshot.hooks.len();
        let native_skills = state.registry.skill_registry.manifests.len();
        let mcp_tools = state.registry.mcp_host.list_tools(&[], &snapshot.skills).await.len();

        // 4. Security Audit Trail
        let last_audit = state.security.audit_trail.get_latest_entry().await;
        let audit_summary = match last_audit {
            Ok(Some(entry)) => format!("Agent [{}] performed [{}] at {}", entry.agent_id, entry.action, entry.timestamp),
            _ => "No recent security events.".to_string(),
        };

        // 5. Workspace Context
        let workspace_root = state.base_dir.to_string_lossy();

        format!(
            "### SOVEREIGN STATE MANIFEST\n\
             - **Swarm Vitality**: {} Active, {} Idle agents currently provisioned.\n\
             - **Financial Governance**: ${:.4} remaining in global mission budget.\n\
             - **Capability Density**: {} scripts, {} workflows, {} hooks, {} native, {} MCP tools operational.\n\
             - **Security Pulse**: Last Audit: {}\n\
             - **Environment**: sovereign_root=\"{}\"\n\
             - **Policy**: Auto-Approve Safe Skills: {}\n\
             ---",
            active_agents, 
            idle_agents, 
            remaining_budget, 
            script_skills, 
            workflow_skills,
            hook_skills,
            native_skills, 
            mcp_tools, 
            audit_summary,
            workspace_root,
            state.governance.auto_approve_safe_skills.load(std::sync::atomic::Ordering::Relaxed)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{AgentHealth, AgentIdentity, EngineAgent};

    #[tokio::test]
    async fn test_sovereign_state_manifest_generation_full() {
        let state = Arc::new(AppState::new_mock().await);

        // Add 2 mock agents
        state.registry.agents.insert("agent_1".to_string(), EngineAgent {
            identity: AgentIdentity { id: "agent_1".to_string(), name: "Agent One".to_string(), ..Default::default() },
            health: AgentHealth { status: "active".to_string(), ..Default::default() },
            ..Default::default()
        });
        state.registry.agents.insert("agent_2".to_string(), EngineAgent {
            identity: AgentIdentity { id: "agent_2".to_string(), name: "Agent Two".to_string(), ..Default::default() },
            health: AgentHealth { status: "idle".to_string(), ..Default::default() },
            ..Default::default()
        });

        let manifest = SovereignStateManifest::generate(&state).await;

        assert!(manifest.contains("### SOVEREIGN STATE MANIFEST"));
        assert!(manifest.contains("Swarm Vitality"));
        assert!(manifest.contains("Financial Governance"));
        assert!(manifest.contains("Capability Density"));
        assert!(manifest.contains("Security Pulse"));
        assert!(manifest.contains("Environment"));
        assert!(manifest.contains("Auto-Approve Safe Skills"));
    }

    #[test]
    fn test_sovereign_state_manifest_idle_calculation() {
        let registered_agents = 5usize;
        let active_agents = 2u32;
        let idle_agents = registered_agents.saturating_sub(active_agents as usize);
        assert_eq!(idle_agents, 3);

        // Saturating subtraction prevents overflow if active > registered
        let overflow_active = 10u32;
        let idle_saturated = registered_agents.saturating_sub(overflow_active as usize);
        assert_eq!(idle_saturated, 0);
    }
}

// Metadata: [manifest]
