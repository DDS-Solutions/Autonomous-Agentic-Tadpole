//! @docs ARCHITECTURE:Infrastructure
//! 
//! ### AI Assist Note
//! **! @docs ARCHITECTURE:SovereignKernel**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[manifest]` in tracing logs.

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

// Metadata: [manifest]

// Metadata: [manifest]
