//! @docs ARCHITECTURE:State
//!
//! ### AI Assist Note
//! **The Brain Registry**: Centralized directory for swarm identities,
//! models, and skills. Maintains **Provider Configurations** (OpenAI,
//! Ollama), **Agent Role Definitions**, and the **Tool Discovery Catalog**.
//! Features **Dynamic Skill Loading** (JSON manifests) and **MCP Tool
//! Aggregation** for a high-fidelity capability surface.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Registry cache staling due to persistence sync
//!   delays, duplicate skill IDs in the `DashMap`, or `McpHost` tool
//!   enumeration timeouts.
//! - **Trace Scope**: `server-rs::state::hubs::reg`

use crate::agent::hooks::HooksManager;
use crate::agent::mcp::McpHost;
use crate::agent::script_skills::ScriptSkillsRegistry;
use crate::agent::skill_manifest::SkillRegistry;
use crate::agent::types::{EngineAgent, ModelEntry, ProviderConfig, SwarmNode};
use dashmap::DashMap;
use std::sync::Arc;

/// Hub for agent identities, provider configs, and skill discovery.
pub struct RegistryHub {
    /// The live agent registry, synced with persistence.
    pub agents: DashMap<String, EngineAgent>,
    /// Configured LLM providers (e.g., OpenAI, Ollama).
    pub providers: DashMap<String, ProviderConfig>,
    /// Real-time health status of providers (Amber/Red state machine).
    pub provider_health: DashMap<String, crate::agent::types::ProviderStatus>,
    /// Recent failure counts for providers to trigger health transitions.
    pub provider_failures: DashMap<String, std::sync::atomic::AtomicU32>,
    /// Available LLM models catalog.
    pub models: DashMap<String, ModelEntry>,
    /// Discovery registry for infrastructure nodes in the swarm.
    #[allow(dead_code)]
    pub nodes: DashMap<String, SwarmNode>,
    /// Registry for dynamic file-based Skills and Workflows.
    #[allow(dead_code)]
    pub skills: Arc<ScriptSkillsRegistry>,
    /// Manager for dynamic Skill Manifests (skill.json).
    #[allow(dead_code)]
    pub skill_registry: Arc<SkillRegistry>,
    /// Host for Model Context Protocol (MCP) tool aggregation.
    #[allow(dead_code)]
    pub mcp_host: Arc<McpHost>,
    /// Manager for Lifecycle Hooks (Pre/Post tool execution).
    #[allow(dead_code)]
    pub hooks: Arc<HooksManager>,
    /// Unified registry for all builtin and categorical tools.
    pub tool_registry: Arc<crate::agent::runner::tools::registry::ToolRegistry>,
}

impl RegistryHub {
    /// Returns a summarized list of specialized agents available in the cluster.
    /// Filters out generic or internal nodes to provide a clean recruitment directory.
    pub fn list_active_specialists(&self) -> Vec<String> {
        self.agents
            .iter()
            .filter_map(|kv| {
                let agent = kv.value();
                let id = &agent.identity.id;
 
                // Skip only the generic 'general' node for the specialist directory.
                // Root/Orchestrator nodes (1, 2, alpha) are now included to allow
                // cross-hierarchical recruitment by the CEO.
                if id == "general" {
                    return None;
                }
 
                Some(format!(
                    "- {} (Role: {}, Dept: {}): {}",
                    id, agent.identity.role, agent.identity.department, agent.identity.description
                ))
            })
            .collect()
    }

    /// ### 🧬 [Reconciliation] Registry Normalization
    /// Iterates through all registered agents and fixes configuration drift 
    /// (e.g., missing model IDs, invalid slots, or legacy provider strings).
    /// Ensures the backend strictly aligns with the current model registry.
    pub fn reconcile_agents(&self) {
        tracing::info!("🧬 [Registry] Starting agent configuration reconciliation...");
        let mut fixed_count = 0;

        for mut entry in self.agents.iter_mut() {
            let agent = entry.value_mut();
            let mut changed = false;

            // 1. Ensure active model slot is valid
            if agent.models.active_model_slot.is_none() || !matches!(agent.models.active_model_slot, Some(1) | Some(2) | Some(3)) {
                agent.models.active_model_slot = Some(1);
                changed = true;
            }

            // 2. Sync legacy model_id if empty
            if agent.models.model_id.is_none() || agent.models.model_id.as_deref().unwrap_or_default().is_empty() {
                let primary_id = &agent.models.model.model_id;
                if !primary_id.is_empty() {
                    agent.models.model_id = Some(primary_id.clone());
                    changed = true;
                }
            }

            // 3. Validate STT Engine defaults
            if agent.stt_engine.is_none() {
                agent.stt_engine = Some("groq".to_string());
                changed = true;
            }

            if changed {
                fixed_count += 1;
                tracing::debug!(
                    "🔧 [Registry] Normalized agent '{}' (Slot: {:?}, Model: {:?})",
                    agent.identity.id,
                    agent.models.active_model_slot,
                    agent.models.model_id
                );
            }
        }

        if fixed_count > 0 {
            tracing::info!("✅ [Registry] Reconciliation complete. Fixed {} agents.", fixed_count);
        } else {
            tracing::info!("✅ [Registry] Reconciliation complete. No drift detected.");
        }
    }
}

// Metadata: [reg]

// Metadata: [reg]
