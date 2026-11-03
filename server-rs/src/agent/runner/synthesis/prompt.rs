//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Synthesis System Prompt Engine**: Coordinates the 7-way async prompt synthesis pipeline,
//! context-window-aware pruning, variable template assembly, and emergency token compaction.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Context limit exceeded, system failure context injection, or template rendering error.
//! - **Telemetry Link**: Search `[synthesis_prompt]` in tracing logs.
//! - **Trace Scope**: `server-rs::agent::runner::synthesis::prompt`

use std::collections::HashMap;

use super::budget::count_tokens_fast;
use super::super::{AgentRunner, RunContext};
use crate::agent::constants::*;

/// Checks whether an agent ID corresponds to a top-tier orchestrator identity.
#[inline]
pub fn is_orchestrator_identity(agent_id: &str) -> bool {
    agent_id == AGENT_CEO || agent_id == AGENT_COO || agent_id == AGENT_ALPHA
}

impl AgentRunner {
    /// Constructs the final system prompt for an agent run.
    ///
    /// Synthesizes identity, long-term memory, repo-map, and shared swarm
    /// findings into a single prompt, bounded by the model's context window.
    pub async fn build_system_prompt(&self, ctx: &RunContext, payload_message: &str) -> String {
        let prompt_budget = self.compute_prompt_token_budget(ctx);

        let (
            tools_def,
            (directives_str, reviews_str),
            global_intel_res,
            identity_res,
            memory_res,
            mission_ctx_res,
            (_repo_summary, repo_map_text),
        ) = tokio::join!(
            self.build_tools(ctx),
            self.fetch_external_context(ctx, prompt_budget),
            self.gather_global_intelligence(ctx, payload_message),
            self.fetch_identity(),
            self.fetch_memory(),
            self.fetch_mission_context(ctx),
            self.generate_repo_map(ctx, payload_message),
        );

        let global_intel_str = match global_intel_res {
            Ok(intel) => intel,
            Err(e) => {
                tracing::warn!("⚠️ [Global Intelligence] Vault search failed: {}.", e);
                "GLOBAL_INTEL_FAILURE: Knowledge vault was unreachable.".to_string()
            }
        };

        let mut criticality_score = 0;
        let mut failure_context = Vec::new();

        let identity = match identity_res {
            Ok(id) => id,
            Err(e) => {
                criticality_score += 1;
                failure_context.push(format!(
                    "IDENTITY_FAILURE: Core identity directives could not be loaded: {}.",
                    e
                ));
                tracing::error!("🚨 [Runner] Identity lookup failed: {}.", e);
                String::new()
            }
        };

        let memory = match memory_res {
            Ok(mem) => mem,
            Err(e) => {
                criticality_score += 1;
                failure_context.push(format!(
                    "MEMORY_FAILURE: Long-term memory context could not be retrieved: {}.",
                    e
                ));
                tracing::error!("🚨 [Runner] Memory lookup failed: {}.", e);
                String::new()
            }
        };

        let swarm_context_str = match mission_ctx_res {
            Ok(context) => context,
            Err(e) => {
                criticality_score += 1;
                failure_context.push(format!(
                    "SWARM_CONTEXT_FAILURE: Failed to retrieve mission context: {:?}.",
                    e
                ));
                tracing::error!(
                    "🚨 [Runner] Failed to retrieve mission context for {}: {:?}.",
                    ctx.mission_id,
                    e
                );
                String::from("⚠️ WARNING: Failed to load shared mission context.")
            }
        };

        let (pruned_repo_map, pruned_swarm_context_str) =
            self.prune_context(ctx, &identity, &memory, &repo_map_text, &swarm_context_str);

        let sovereign_manifest =
            crate::system::manifest::SovereignStateManifest::generate(&self.state).await;

        let tool_directory_str =
            Self::format_tool_directory_display(&tools_def.function_declarations);

        let vars = self.assemble_prompt_variables(
            ctx,
            &directives_str,
            &reviews_str,
            &identity,
            &memory,
            &pruned_repo_map,
            &pruned_swarm_context_str,
            criticality_score,
            failure_context,
            sovereign_manifest,
            global_intel_str,
            tool_directory_str,
        );

        let mut system_prompt = self.state.resources.renderer.render(
            self.state.resources.renderer.default_system_template(),
            &vars,
        );

        // Enforce total rendered prompt context budget
        let total_tokens = count_tokens_fast(&system_prompt);
        if total_tokens > prompt_budget {
            tracing::warn!(
                "⚠️ [Prompt Synthesis] Rendered prompt ({} tokens) exceeds budget ({} tokens). Applying emergency compaction.",
                total_tokens,
                prompt_budget
            );
            let mut compact_vars = vars;
            compact_vars.insert(
                "repo_map",
                "⚠️ Repo Map omitted due to context window limits. Use 'list_files' for discovery."
                    .to_string(),
            );
            compact_vars.insert(
                "history",
                "⚠️ History summarized due to context window limits.".to_string(),
            );
            compact_vars.insert(
                "swarm_context",
                self.safe_truncate(&pruned_swarm_context_str, 400),
            );

            system_prompt = self.state.resources.renderer.render(
                self.state.resources.renderer.default_system_template(),
                &compact_vars,
            );

            let compact_tokens = count_tokens_fast(&system_prompt);
            if compact_tokens > prompt_budget {
                system_prompt = self.safe_truncate(&system_prompt, prompt_budget * 4);
            }
        }

        // Optional debug prompt logging strictly gated by environment variable with secret redaction
        if std::env::var_os("TADPOLE_DUMP_PROMPTS").is_some() {
            let _ = std::fs::create_dir_all(".tmp");
            let safe_mission = ctx
                .mission_id
                .replace(|c: char| !c.is_alphanumeric() && c != '-', "_");
            let safe_agent = ctx
                .agent_id
                .replace(|c: char| !c.is_alphanumeric() && c != '-', "_");
            let redacted_prompt = self.state.security.secret_redactor.redact(&system_prompt);
            let _ = std::fs::write(
                format!(".tmp/prompt_{}_{}.txt", safe_mission, safe_agent),
                &redacted_prompt,
            );
        }

        tracing::info!(
            "🏁 [Runner] Synthesizing system prompt for mission: {} (Depth: {})",
            ctx.mission_id,
            ctx.depth
        );
        system_prompt
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble_prompt_variables(
        &self,
        ctx: &RunContext,
        directives_str: &str,
        reviews_str: &str,
        identity: &str,
        memory: &str,
        pruned_repo_map: &str,
        pruned_swarm_context_str: &str,
        criticality_score: i32,
        failure_context: Vec<String>,
        sovereign_manifest: String,
        global_intel: String,
        tool_directory_str: String,
    ) -> HashMap<&'static str, String> {
        let hierarchy_label = self.get_hierarchy_label(ctx.authority_level);
        let compressed_swarm_context = self.shorten_system_tokens(pruned_swarm_context_str);

        let priority_str = if ctx.agent_id == AGENT_CEO {
            "STRATEGIC OVERLORD: You are the final authority. Your priority is mission synthesis and high-level routing."
        } else if ctx.agent_id == AGENT_ALPHA {
            "ALPHA MISSION COMMANDER: Your priority is tactical execution and specialist coordination."
        } else {
            "TACTICAL SPECIALIST: Your priority is focused task completion."
        };

        let raw_findings = ctx
            .recent_findings
            .as_deref()
            .unwrap_or("No recent findings inherited.");
        let compressed_findings = if raw_findings == "No recent findings inherited." {
            raw_findings.to_string()
        } else {
            self.fence_untrusted_data("peer_findings", &self.shorten_system_tokens(raw_findings))
        };

        let swarm_str = self.generate_swarm_protocols(ctx);
        let skill_fragments_str = self.generate_skill_fragments(ctx);
        let workflow_fragments_str = self.generate_workflow_fragments(ctx);

        let (
            cluster_directory,
            filesystem_bias_mandate,
            lineage_display,
            _is_orchestrator,
            safe_mode_prefix,
            tool_mode_prefix,
        ) = self.generate_structural_components(ctx);

        let working_memory_str =
            serde_json::to_string_pretty(&ctx.working_memory).unwrap_or_else(|_| "{}".to_string());

        let mut final_safe_mode_prefix = safe_mode_prefix.clone();
        if count_tokens_fast(&working_memory_str) > 1000 {
            final_safe_mode_prefix.push_str("\n⚠️ SYSTEM WARNING: Your working_memory JSON block is large (>1,000 tokens). Please clean up or summarize old keys in your working memory to avoid prompt bloat.\n");
        }

        if criticality_score > 0 {
            let warning = format!(
                "\n\n!!! SYSTEM WARNING: Critical services failed (Score: {}/3) !!!\n- {}\n!!! ACTION: Acknowledge these failures in your internal reasoning and attempt to recover missing context manually if possible. !!!\n",
                criticality_score,
                failure_context.join("\n- ")
            );
            final_safe_mode_prefix.push_str(&warning);
        }

        let mut vars = HashMap::new();
        vars.insert("name", ctx.name.clone());
        vars.insert("agent_id", ctx.agent_id.clone());
        vars.insert("role", ctx.role.clone());
        vars.insert("department", ctx.department.clone());
        vars.insert("description", ctx.description.clone());
        vars.insert("hierarchy_label", hierarchy_label.to_string());
        vars.insert("directives", directives_str.to_string());
        vars.insert("reviews", reviews_str.to_string());
        vars.insert("global_intelligence", global_intel);
        vars.insert("priority", priority_str.to_string());
        vars.insert(
            "personality",
            ctx.model_config
                .system_prompt
                .as_deref()
                .unwrap_or("No specific personality instructions.")
                .to_string(),
        );
        vars.insert("skill_fragments", skill_fragments_str);
        vars.insert("workflow_fragments", workflow_fragments_str);
        vars.insert("swarm_context", compressed_swarm_context);
        vars.insert("breadcrumbs", self.generate_breadcrumbs_display(ctx));
        vars.insert("findings", compressed_findings);
        vars.insert(
            "primary_goal",
            ctx.primary_goal
                .clone()
                .unwrap_or_else(|| "See mission scope for details.".to_string()),
        );
        vars.insert("cluster_directory", cluster_directory);
        vars.insert("lineage", lineage_display);
        vars.insert(
            "skills",
            if ctx.skills.is_empty() {
                "None".to_string()
            } else {
                ctx.skills.join(", ")
            },
        );
        vars.insert(
            "workflows",
            if ctx.workflows.is_empty() {
                "None".to_string()
            } else {
                ctx.workflows.join(", ")
            },
        );
        vars.insert("filesystem_bias", filesystem_bias_mandate);
        vars.insert("swarm_protocols", swarm_str);
        vars.insert("repo_map", pruned_repo_map.to_string());
        vars.insert("identity", identity.to_string());
        vars.insert("sovereign_manifest", sovereign_manifest);
        vars.insert("memory", memory.to_string());
        vars.insert("working_memory", working_memory_str);
        vars.insert(
            "history",
            self.shorten_system_tokens(
                ctx.summarized_history
                    .as_deref()
                    .unwrap_or("No previous history recorded for this mission."),
            ),
        );
        vars.insert("safe_mode_prefix", final_safe_mode_prefix);
        vars.insert("tool_mode_prefix", tool_mode_prefix);
        vars.insert("tool_directory", tool_directory_str);

        let local_now = chrono::Local::now();
        let utc_now = chrono::Utc::now();
        let temporal_anchor = format!(
            "Local: {} | UTC: {}",
            local_now.format("%Y-%m-%d %H:%M:%S %Z"),
            utc_now.to_rfc3339()
        );
        vars.insert("temporal_anchor", temporal_anchor);

        vars
    }

    /// Handles context pruning based on token count and context window limits.
    pub(crate) fn prune_context(
        &self,
        ctx: &RunContext,
        identity: &str,
        memory: &str,
        repo_map: &str,
        swarm_context_str: &str,
    ) -> (String, String) {
        let budget_limit = self.compute_prompt_token_budget(ctx);
        let safe_limit = (budget_limit as f32 * 0.85) as usize;

        let mut pruned_repo_map = if ctx.model_config.provider
            == crate::agent::types::ModelProvider::Ollama
        {
            "⚠️ Repo Map pruned to optimize local model performance. Use 'list_files' for discovery.".to_string()
        } else {
            repo_map.to_string()
        };
        let mut pruned_swarm_context = swarm_context_str.to_string();

        let get_total_tokens = |repo: &str, swarm: &str| {
            let combined = format!("{}{}{}{}", identity, memory, repo, swarm);
            count_tokens_fast(&combined)
        };

        let mut current_tokens = get_total_tokens(&pruned_repo_map, &pruned_swarm_context);

        if current_tokens > safe_limit {
            tracing::warn!(
                "⚠️ [Pruning] Context size {} exceeds safe limit {}. Applying semantic weights.",
                current_tokens,
                safe_limit
            );

            // 1. Prune Repo Map (Weight: 0.8)
            if current_tokens > safe_limit {
                let weight = ctx.resource_weights.get("repo_map").cloned().unwrap_or(0.8);
                if weight < 1.0 {
                    pruned_repo_map =
                        "⚠️ Repo Map pruned due to context limits. Use 'list_files' for discovery."
                            .to_string();
                    current_tokens = get_total_tokens(&pruned_repo_map, &pruned_swarm_context);
                }
            }

            // 2. Prune Swarm Context (Weight: 1.0)
            if current_tokens > safe_limit {
                let target_len = (pruned_swarm_context.len() as f32 * 0.5) as usize;
                if target_len > 500 {
                    pruned_swarm_context = self.safe_truncate(&pruned_swarm_context, target_len);
                    current_tokens = get_total_tokens(&pruned_repo_map, &pruned_swarm_context);
                }
            }

            // 3. Emergency Truncation
            if current_tokens > budget_limit {
                tracing::error!(
                    "🚨 [Pruning] EMERGENCY: Context still exceeds budget limit ({}). Hard truncating swarm context.",
                    current_tokens
                );
                pruned_swarm_context = self.safe_truncate(&pruned_swarm_context, 500);
            }
        }

        (pruned_repo_map, pruned_swarm_context)
    }

    pub(crate) fn get_hierarchy_label(
        &self,
        level: crate::agent::types::RoleAuthorityLevel,
    ) -> &str {
        match level {
            crate::agent::types::RoleAuthorityLevel::Executive => "OVERLORD (Tier 0)",
            crate::agent::types::RoleAuthorityLevel::Management => "ORCHESTRATOR (Tier 1)",
            crate::agent::types::RoleAuthorityLevel::Specialist => "SPECIALIST (Tier 2)",
            crate::agent::types::RoleAuthorityLevel::Observer => "AUDITOR (Tier 3)",
        }
    }

    /// Compiles the structural components of the prompt.
    pub(crate) fn generate_structural_components(
        &self,
        ctx: &RunContext,
    ) -> (
        String, // Cluster Directory
        String, // File System Bias Mandate
        String, // Lineage Display
        bool,   // Is Orchestrator
        String, // Safe Mode Prefix
        String, // Tool Mode Prefix
    ) {
        let cluster_specialists = self.state.registry.list_active_specialists();
        let is_orchestrator = is_orchestrator_identity(&ctx.agent_id)
            || ctx.authority_level >= crate::agent::types::RoleAuthorityLevel::Management;

        // --- Cluster Directory ---
        let cluster_directory = if cluster_specialists.is_empty() {
            "No specialized agents currently registered in the cluster pool. You are encouraged to recruit a NEW specialist ID (e.g. 'researcher', 'coder') if needed.".to_string()
        } else {
            cluster_specialists
                .into_iter()
                .map(|id| {
                    if is_orchestrator {
                        id
                    } else {
                        match id.as_str() {
                            AGENT_CEO => "[SYSTEM_OVERLORD]".to_string(),
                            AGENT_COO => "[SYSTEM_ORCHESTRATOR]".to_string(),
                            AGENT_ALPHA => "[SYSTEM_COMMANDER]".to_string(),
                            other => other.to_string(),
                        }
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        // --- Mission Bias ---
        let filesystem_bias_mandate = if self.has_file_system_capability(ctx) {
            "DATA_ACCESS: PERMITTED. You represent a specialist with localized I/O authority."
                .to_string()
        } else {
            "DATA_ACCESS: RESTRICTED. Direct filesystem mutation is disabled. Use swarm recruitment for I/O.".to_string()
        };

        // --- Lineage Display ---
        let lineage_display = if ctx.lineage.is_empty() {
            "None (You are the root node)".to_string()
        } else {
            let path_parts: Vec<String> = ctx
                .lineage
                .iter()
                .map(|id| {
                    if is_orchestrator {
                        id.clone()
                    } else {
                        match id.as_str() {
                            AGENT_CEO => "OVERLORD".to_string(),
                            AGENT_COO => "ORCHESTRATOR".to_string(),
                            AGENT_ALPHA => "COMMANDER".to_string(),
                            _ => id.clone(),
                        }
                    }
                })
                .collect();
            path_parts.join(" -> ")
        };

        // --- Prefixes ---
        let safe_mode_prefix = if ctx.safe_mode {
            "🔒 SAFE MODE ACTIVE: You are in READ-ONLY / INSPECTION mode. State mutations (file writing/deletion, shell execution) and agent recruitment are strictly disabled. You may query repository status, inspect files, or converse with the user.\n".to_string()
        } else {
            "🔓 SECURITY: ACTIVE. You have authority to propose and execute mutations within your workspace scope.\n".to_string()
        };

        let mut tool_mode_prefix = if ctx.analysis {
            "🛠️ TOOL EVOLUTION: You are in ANALYSIS mode. Propose architectural changes."
                .to_string()
        } else {
            "⚙️ TOOL REFINEMENT: Standard operation.".to_string()
        };

        // Model-Specific Protocol Injection for Local Models
        if ctx.model_config.provider == crate::agent::types::ModelProvider::Ollama
            || ctx.model_config.provider == crate::agent::types::ModelProvider::Local
        {
            tool_mode_prefix.push_str("\n\n=== 🛠️ LOCAL MODEL TOOL CALLING MANDATE ===\n\
            Since you are a local model, native JSON schemas can sometimes be parsed incorrectly by the runtime wrapper. \
            To guarantee 100% tool execution reliability, you MUST invoke tools using XML tags in your response content!\n\
            Format your tool calls exactly like this:\n\
            <invoke_tool>{\n\
              \"name\": \"read_file\",\n\
              \"arguments\": {\n\
                \"path\": \"execution/db_health_check.py\"\n\
              }\n\
            }</invoke_tool>\n\
            \n\
            Example of completing the mission:\n\
            <invoke_tool>{\n\
              \"name\": \"complete_mission\",\n\
              \"arguments\": {\n\
                \"output\": \"Audit successfully completed...\"\n\
              }\n\
            }</invoke_tool>\n\
            \n\
            CRITICAL RULES:\n\
            1. You MUST call tools to gather facts or modify files. Narrative planning without tool calls is MISSION FAILURE.\n\
            2. Never write raw JSON tool calls outside of the <invoke_tool>...</invoke_tool> XML tags.\n\
            3. Always supply all required parameters within the JSON 'arguments' object.\n\
            4. If you have finished, call 'complete_mission' with the final results.\n");
        }

        (
            cluster_directory,
            filesystem_bias_mandate,
            lineage_display,
            is_orchestrator,
            safe_mode_prefix,
            tool_mode_prefix,
        )
    }
}

// Metadata: [synthesis_prompt]

