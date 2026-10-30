//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Modular Prompt Synthesizer**: Assembles the agent's "consciousness" through a
//! multi-stage modular pipeline. Links identity, memory, codebase maps, and
//! shared swarm findings.
//!
//! ### 🛡️ Hardened Protocols
//! - **Context Pruning** (PERF-07): Precise context-window-aware prompt truncation.
//! - **Hierarchy Enforcement** (SEC-06): Specialized access control (e.g., blocking
//!   Agent 1 from direct filesystem tools to enforce delegation).
//! - **Identity Masking**: Specialists are anonymized in cluster directories for
//!   high-level managers (OVERLORD/ORCHESTRATOR) to prioritize strategic delegation.
//! - **Safe Mode** (SEC-05): Strict capability allow-list for read-only tools.
//! - **Untrusted Data Fencing**: Fences external/peer inputs with secret redaction.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[synthesis]` in tracing logs.
//! - **Failure Path**: Tokenizer failure, mission context retrieval fault,
//!   or identity lookup cache miss.
//! - **Trace Scope**: `server-rs::agent::runner::synthesis`
//!

// Modules/Crates
use std::collections::HashMap;

// External Crates
use once_cell::sync::Lazy;

// Internal Modules
use super::{AgentRunner, RunContext};
use crate::agent::constants::*;
use crate::agent::types::{FunctionDeclaration, ToolDefinition};

/// Global, zero-reinitialization BPE tokenizer cache
static BPE_TOKENIZER: Lazy<Option<tiktoken_rs::CoreBPE>> = Lazy::new(|| {
    tiktoken_rs::cl100k_base().ok()
});

/// Fast token counting helper using the shared cached BPE instance
pub fn count_tokens_fast(text: &str) -> usize {
    if let Some(ref bpe) = *BPE_TOKENIZER {
        bpe.encode_with_special_tokens(text).len()
    } else {
        text.len() / 4
    }
}

/// Static regex lookup table for token compression with strict word-boundary protection.
/// Ensures words like "okay" or identifiers like "temperature_sensor" are never corrupted.
static SYSTEM_TOKEN_REGEXES: Lazy<Vec<(regex::Regex, &'static str)>> = Lazy::new(|| {
    vec![
        (regex::Regex::new(r"\bSTATUS:\s*ok\b").unwrap(), "*ok*"),
        (regex::Regex::new(r"\bSTATUS:\s*success\b").unwrap(), "*ok*"),
        (regex::Regex::new(r"\bSTATUS:\s*failed\b").unwrap(), "*err*"),
        (regex::Regex::new(r"\bSTATUS:\s*error\b").unwrap(), "*err*"),
        (regex::Regex::new(r"\bRESULT:").unwrap(), "RES:"),
        (regex::Regex::new(r"\bFINDING:").unwrap(), "FND:"),
        (regex::Regex::new(r"\bSOURCE:").unwrap(), "SRC:"),
        (regex::Regex::new(r"\bLocation:").unwrap(), "LOC:"),
        (regex::Regex::new(r"\bPrimary Goal:").unwrap(), "GOAL:"),
        (regex::Regex::new(r"\bWeather for zip\b").unwrap(), "WTR|"),
        (regex::Regex::new(r"\bFinding for mission\b").unwrap(), "FND|"),
        (regex::Regex::new(r"\bStrategic Intent:").unwrap(), "INT:"),
        (regex::Regex::new(r"\bMission Complete\b").unwrap(), "*done*"),
        (regex::Regex::new(r"\bTask in progress\b").unwrap(), "*busy*"),
    ]
});

/// Fail-closed allow-list of read-only tools permitted in Safe Mode.
pub const SAFE_MODE_ALLOWED_TOOLS: &[&str] = &[
    "read_file",
    "list_files",
    "grep_search",
    "get_current_time",
    "inspect_health",
    "set_confidence",
    "list_graph",
    "get_symbol",
];

/// Formats a list of items under a token budget, guaranteeing that the appended
/// `truncated_notice` never causes the total output to exceed `token_budget`.
fn format_budgeted_items<T, F>(
    items: Vec<T>,
    token_budget: usize,
    empty_fallback: &str,
    truncated_notice: &str,
    format_item: F,
) -> String
where
    F: Fn(&T) -> String,
{
    if items.is_empty() {
        return empty_fallback.to_string();
    }

    let notice_tokens = count_tokens_fast(truncated_notice);
    let mut lines = Vec::new();
    let mut used_tokens = 0;
    let mut truncated = false;

    for item in &items {
        let line = format_item(item);
        let tokens = count_tokens_fast(&line);
        if used_tokens + tokens + notice_tokens <= token_budget {
            lines.push(line);
            used_tokens += tokens;
        } else if used_tokens + tokens <= token_budget && lines.len() + 1 == items.len() {
            // Exactly last item and fits within total budget without notice
            lines.push(line);
            used_tokens += tokens;
        } else {
            truncated = true;
            break;
        }
    }

    if truncated {
        // Ensure adding the notice does not push total over budget
        while !lines.is_empty() && used_tokens + notice_tokens > token_budget {
            if let Some(removed) = lines.pop() {
                used_tokens = used_tokens.saturating_sub(count_tokens_fast(&removed));
            }
        }
        lines.push(truncated_notice.to_string());
    }

    if lines.is_empty() {
        if truncated {
            truncated_notice.to_string()
        } else {
            empty_fallback.to_string()
        }
    } else {
        lines.join("\n")
    }
}

/// Checks whether an agent ID corresponds to a top-tier orchestrator identity.
#[inline]
fn is_orchestrator_identity(agent_id: &str) -> bool {
    agent_id == AGENT_CEO || agent_id == AGENT_COO || agent_id == AGENT_ALPHA
}

impl AgentRunner {
    /// 📟 [Token Compression]
    /// Shortens system tokens using strict boundary regexes to prevent identifier corruption.
    fn shorten_system_tokens(&self, text: &str) -> String {
        let mut encoded = text.to_string();
        for (re, replacement) in SYSTEM_TOKEN_REGEXES.iter() {
            if re.is_match(&encoded) {
                encoded = re.replace_all(&encoded, *replacement).to_string();
            }
        }
        encoded
    }

    /// Evaluates whether a registered tool is authorized and eligible for the given `RunContext`.
    /// Shared between `format_tool_directory_display` and `build_tools` to guarantee 1:1 parity.
    pub(crate) fn is_registered_tool_eligible(&self, ctx: &RunContext, tool_name: &str) -> bool {
        if !self
            .state
            .resources
            .acl
            .is_tool_allowed(&ctx.agent_id, &ctx.role, ctx.authority_level, tool_name)
        {
            return false;
        }

        // Strict Membership Filter: Only include the tool if it's in the agent's context skills or is a system core tool.
        let is_core_tool = tool_name == "complete_mission"
            || tool_name == "issue_alpha_directive"
            || tool_name == "set_confidence";
        if !is_core_tool && !ctx.skills.iter().any(|s| s == tool_name) {
            return false;
        }

        // Specialized Skill Checks
        if tool_name == "execute_shell"
            && !ctx
                .skills
                .iter()
                .any(|s| s == "shell" || s == "terminal")
        {
            return false;
        }

        // Safe Mode Restrictions (Fail-Closed Deny-by-Default Capability Allow-List)
        if ctx.safe_mode && !SAFE_MODE_ALLOWED_TOOLS.contains(&tool_name) {
            return false;
        }

        // Filesystem Capability Check
        if matches!(
            tool_name,
            "read_file" | "write_file" | "list_files" | "delete_file"
        ) && !self.has_file_system_capability(ctx)
        {
            return false;
        }

        true
    }

    /// Encloses untrusted or external input in security boundaries with secret redaction
    fn fence_untrusted_data(&self, role: &str, content: &str) -> String {
        let redacted = self.state.security.secret_redactor.redact(content);
        format!(
            "<<<UNTRUSTED_EXTERNAL_DATA role=\"{}\">>>\n{}\n<<<END_UNTRUSTED_EXTERNAL_DATA>>>",
            role, redacted
        )
    }

    /// Computes the usable prompt token budget derived from model context window
    pub(crate) fn compute_prompt_token_budget(&self, ctx: &RunContext) -> usize {
        if let Some(cw) = ctx.model_config.context_window {
            let max_resp = ctx.model_config.max_tokens.unwrap_or(4096) as usize;
            (cw as usize).saturating_sub(max_resp).max(20)
        } else if let Some(tpm) = ctx.model_config.tpm {
            tpm as usize
        } else {
            let cw = 32_768;
            let max_resp = ctx.model_config.max_tokens.unwrap_or(4096) as usize;
            cw - max_resp
        }
    }

    // ─────────────────────────────────────────────────────────
    //  SYSTEM PROMPT CONSTRUCTION (MODULARIZED)
    // ─────────────────────────────────────────────────────────

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

        let (pruned_repo_map, pruned_swarm_context_str) = self.prune_context(
            ctx,
            &identity,
            &memory,
            &repo_map_text,
            &swarm_context_str,
        );

        let sovereign_manifest =
            crate::system::manifest::SovereignStateManifest::generate(&self.state).await;

        let tool_directory_str = Self::format_tool_directory_display(&tools_def.function_declarations);

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

        let mut system_prompt = self
            .state
            .resources
            .renderer
            .render(self.state.resources.renderer.default_system_template(), &vars);

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
                "⚠️ Repo Map omitted due to context window limits. Use 'list_files' for discovery.".to_string(),
            );
            compact_vars.insert(
                "history",
                "⚠️ History summarized due to context window limits.".to_string(),
            );
            compact_vars.insert(
                "swarm_context",
                self.safe_truncate(&pruned_swarm_context_str, 400),
            );

            system_prompt = self
                .state
                .resources
                .renderer
                .render(self.state.resources.renderer.default_system_template(), &compact_vars);

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

    // =============================================================
    // PRIVATE HELPER METHODS FOR MODULARITY
    // =============================================================

    async fn fetch_external_context(&self, ctx: &RunContext, budget: usize) -> (String, String) {
        let (pending_directives, pending_reviews) = tokio::join!(
            super::swarm_persistence::get_pending_directives(
                &self.state.resources.pool,
                &ctx.agent_id
            ),
            super::swarm_persistence::get_pending_reviews(
                &self.state.resources.pool,
                &ctx.agent_id
            ),
        );

        let dynamic_budget = (budget / 10).clamp(500, 4000);

        let directives_str = match pending_directives {
            Ok(directives) => {
                let formatted = format_budgeted_items(
                    directives,
                    dynamic_budget,
                    "No active directives. Proceed with mission objectives.",
                    "- ... [Remaining directives truncated due to token budget]",
                    |d| format!("- [Directive] From {}: {}", d.source_agent_id, d.instruction),
                );
                if formatted.starts_with("No active directives") {
                    formatted
                } else {
                    self.fence_untrusted_data("directives", &formatted)
                }
            }
            Err(e) => {
                tracing::warn!("⚠️ [External Context] Directives fetch failed: {}", e);
                "⚠️ DIRECTIVES_UNAVAILABLE: Database error while querying pending directives. Maintain operational caution.".to_string()
            }
        };

        let reviews_str = match pending_reviews {
            Ok(reviews) => {
                let formatted = format_budgeted_items(
                    reviews,
                    dynamic_budget,
                    "No peer reviews pending. Maintain standard quality protocols.",
                    "- ... [Remaining peer reviews truncated due to token budget]",
                    |r| {
                        format!(
                            "- [Review Task] Target: {}. Requirement: {}",
                            r.requester_id, r.content_to_review
                        )
                    },
                );
                if formatted.starts_with("No peer reviews pending") {
                    formatted
                } else {
                    self.fence_untrusted_data("peer_reviews", &formatted)
                }
            }
            Err(e) => {
                tracing::warn!("⚠️ [External Context] Peer reviews fetch failed: {}", e);
                "⚠️ REVIEWS_UNAVAILABLE: Database error while querying pending peer reviews.".to_string()
            }
        };

        (directives_str, reviews_str)
    }

    async fn fetch_identity(&self) -> Result<String, crate::error::AppError> {
        let id = self.state.resources.get_identity_context().await?;
        Ok(id)
    }

    async fn fetch_memory(&self) -> Result<String, crate::error::AppError> {
        let mem = self.state.resources.get_memory_context().await?;
        Ok(mem)
    }

    async fn fetch_mission_context(
        &self,
        ctx: &RunContext,
    ) -> Result<String, crate::error::AppError> {
        let context = crate::agent::mission::get_mission_context(
            &self.state.resources.pool,
            &ctx.mission_id,
        )
        .await?;
        Ok(context)
    }

    /// Formats a list of function declarations into human-readable directory text.
    pub(crate) fn format_tool_directory_display(tools: &[FunctionDeclaration]) -> String {
        if tools.is_empty() {
            return "No tools are currently authorized or available for your security clearance/skills."
                .to_string();
        }

        let mut display = String::new();
        for tool in tools {
            display.push_str(&format!(
                "- Tool Name: {}\n  Description: {}\n  Parameters Schema: {}\n\n",
                tool.name,
                tool.description,
                serde_json::to_string(&tool.parameters).unwrap_or_else(|_| "{}".to_string())
            ));
        }
        display
    }

    /// Generates a human-readable text directory of all available and allowed tools
    /// with 1:1 parity with the dynamically synthesized manifest.
    #[allow(dead_code)]
    pub(crate) async fn generate_tool_directory_display(&self, ctx: &RunContext) -> String {
        let tools = self.build_tools(ctx).await;
        Self::format_tool_directory_display(&tools.function_declarations)
    }

    #[allow(clippy::too_many_arguments)]
    fn assemble_prompt_variables(
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

        let raw_findings = ctx.recent_findings.as_deref().unwrap_or("No recent findings inherited.");
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

        let working_memory_str = serde_json::to_string_pretty(&ctx.working_memory).unwrap_or_else(|_| "{}".to_string());

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
        vars.insert("personality", ctx.model_config.system_prompt.as_deref().unwrap_or("No specific personality instructions.").to_string());
        vars.insert("skill_fragments", skill_fragments_str);
        vars.insert("workflow_fragments", workflow_fragments_str);
        vars.insert("swarm_context", compressed_swarm_context);
        vars.insert("breadcrumbs", self.generate_breadcrumbs_display(ctx));
        vars.insert("findings", compressed_findings);
        vars.insert("primary_goal", ctx.primary_goal.clone().unwrap_or_else(|| "See mission scope for details.".to_string()));
        vars.insert("cluster_directory", cluster_directory);
        vars.insert("lineage", lineage_display);
        vars.insert("skills", if ctx.skills.is_empty() { "None".to_string() } else { ctx.skills.join(", ") });
        vars.insert("workflows", if ctx.workflows.is_empty() { "None".to_string() } else { ctx.workflows.join(", ") });
        vars.insert("filesystem_bias", filesystem_bias_mandate);
        vars.insert("swarm_protocols", swarm_str);
        vars.insert("repo_map", pruned_repo_map.to_string());
        vars.insert("identity", identity.to_string());
        vars.insert("sovereign_manifest", sovereign_manifest);
        vars.insert("memory", memory.to_string());
        vars.insert("working_memory", working_memory_str);
        vars.insert("history", self.shorten_system_tokens(ctx.summarized_history.as_deref().unwrap_or("No previous history recorded for this mission.")));
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

    /// Generates the Repo Map summary, applying pruning logic if needed.
    async fn generate_repo_map(&self, ctx: &RunContext, payload_message: &str) -> (crate::utils::graph::GraphSummary, String) {
        let graph: std::sync::Arc<parking_lot::RwLock<crate::utils::graph::CodeGraph>> =
            self.state.resources.get_code_graph().await;

        let summary = graph.read().generate_summary();

        // Only include repo map for non-trivial tasks or technical roles to save tokens
        if payload_message.len() > 10 || ctx.department.contains("Technical") {
            (summary.clone(), summary.text)
        } else {
            (
                crate::utils::graph::GraphSummary {
                    text: "Architecture Map: Omitted for high-level greeting.".to_string(),
                    hot_paths: vec![],
                },
                "Architecture Map: Omitted for high-level greeting.".to_string(),
            )
        }
    }

    /// Handles context pruning based on token count and context window limits.
    fn prune_context(
        &self,
        ctx: &RunContext,
        identity: &str,
        memory: &str,
        repo_map: &str,
        swarm_context_str: &str,
    ) -> (String, String) {
        let budget_limit = self.compute_prompt_token_budget(ctx);
        let safe_limit = (budget_limit as f32 * 0.85) as usize;

        let mut pruned_repo_map = if ctx.model_config.provider == crate::agent::types::ModelProvider::Ollama {
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
                    pruned_repo_map = "⚠️ Repo Map pruned due to context limits. Use 'list_files' for discovery.".to_string();
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

    pub(crate) fn get_hierarchy_label(&self, level: crate::agent::types::RoleAuthorityLevel) -> &str {
        match level {
            crate::agent::types::RoleAuthorityLevel::Executive => "OVERLORD (Tier 0)",
            crate::agent::types::RoleAuthorityLevel::Management => "ORCHESTRATOR (Tier 1)",
            crate::agent::types::RoleAuthorityLevel::Specialist => "SPECIALIST (Tier 2)",
            crate::agent::types::RoleAuthorityLevel::Observer => "AUDITOR (Tier 3)",
        }
    }

    /// Compiles the structural components of the prompt.
    fn generate_structural_components(
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
           || ctx.model_config.provider == crate::agent::types::ModelProvider::Local {
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

    /// Generates the shared Swarm Protocol rules using the ACL Service.
    fn generate_swarm_protocols(&self, ctx: &RunContext) -> String {
        let mut swarm_protocols = self.state.resources.acl.get_role_protocols(&ctx.agent_id, &ctx.role, ctx.authority_level);
        let is_orchestrator = is_orchestrator_identity(&ctx.agent_id)
            || ctx.authority_level >= crate::agent::types::RoleAuthorityLevel::Management;

        // Add shared/invariant swarm rules
        if !is_orchestrator && !ctx.safe_mode {
            swarm_protocols.push("NO NARRATION: Do not explain strategy or tool options. CALL THE TOOLS. Text-only responses are MISSION FAILURE.".to_string());
        } else if is_orchestrator {
            swarm_protocols.push("AUTONOMOUS DELEGATION: You are the primary interface. If a task requires specialized skills, IMMEDIATELY recruit the relevant agent IDs from the CLUSTER DIRECTORY using the available recruitment tools. Do NOT wait for user approval to delegate.".to_string());
            swarm_protocols.push("CONVERSATIONAL: While you may be professional and conversational, this MUST be accompanied by tool execution if sub-tasks are identified.".to_string());
        }

        swarm_protocols.push("AUTONOMOUS ACTION: You are a sovereign entity. If a tool exists to achieve your goal, CALL IT IMMEDIATELY. Do not explain your intent to the user, do not ask for permission, and do not provide a 'plan' in natural language unless explicitly asked for a report. JUST CALL THE TOOLS.".to_string());
        swarm_protocols.push("OVERSIGHT TRUST: Never ask for permission to execute a tool or recruit an agent. The system handles approval flows and budget gates automatically. YOUR SILENCE OR ASKING FOR PERMISSION IS A PROTOCOL VIOLATION AND MISSION FAILURE.".to_string());
        swarm_protocols.push("RECRUITMENT PRECISION: When recruiting, you MUST use the exact ID from the 'CLUSTER DIRECTORY' (e.g. '2', '12', '99'). Do NOT use generic names like 'Security Specialist' if a specific ID exists.".to_string());
        swarm_protocols.push("SOURCE OF TRUTH: Prioritize codebase tools (read_file, list_files, grep_search) over general knowledge for repository queries.".to_string());
        
        let lineage_repr = if ctx.lineage.is_empty() {
            "None (Root)".to_string()
        } else {
            ctx.lineage.join(" -> ")
        };
        swarm_protocols.push(format!("RECURSION PROTECTION: You are FORBIDDEN from recruiting agents in your LINEAGE ({}) or YOURSELF ({}).", lineage_repr, ctx.agent_id));

        swarm_protocols
            .iter()
            .enumerate()
            .map(|(i, p)| format!("{}. {}", i + 1, p))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Generates the File Path Breadcrumbs context.
    fn generate_breadcrumbs_display(&self, ctx: &RunContext) -> String {
        let breadcrumbs = ctx.last_accessed_files.lock();
        if breadcrumbs.is_empty() {
            "None available.".to_string()
        } else {
            breadcrumbs.join("\n- ")
        }
    }

    /// Dynamic progressive disclosure skill check helper
    fn should_load_skill(&self, skill_name: &str, ctx: &RunContext) -> bool {
        // Core skills that are ALWAYS loaded
        let core_skills = ["systematic-debugging", "plan-writing", "verify-changes", "context-compression"];
        if core_skills.contains(&skill_name) {
            return true;
        }

        // Small skill sets (< 6) are loaded in full to ensure all tools have operational instructions
        if ctx.skills.len() <= 6 {
            return true;
        }

        // Database/Migration skills trigger rules
        let db_keywords = ["db", "database", "migration", "rls", "sql", "schema", "prisma", "postgres", "query", "table"];
        let is_db_skill = skill_name.contains("database") || skill_name.contains("migration") || skill_name.contains("rls") || skill_name.contains("api-patterns");

        // UI/Frontend skills trigger rules
        let ui_keywords = ["ui", "frontend", "css", "html", "react", "component", "styling", "layout", "page", "button", "view"];
        let is_ui_skill = skill_name.contains("frontend") || skill_name.contains("web-design") || skill_name.contains("mobile-design") || skill_name.contains("react");

        // Vulnerability/Security skills trigger rules
        let sec_keywords = ["security", "vulnerability", "audit", "cve", "auth", "permission", "token", "jwt", "hacker", "attack"];
        let is_sec_skill = skill_name.contains("security") || skill_name.contains("vulnerability") || skill_name.contains("scanner");

        // Inspect Primary Goal / Task prompt
        let goal_text = ctx.primary_goal.as_deref().unwrap_or("").to_lowercase();
        
        // Inspect recently accessed files
        let accessed_files = ctx.last_accessed_files.lock();
        let has_db_files = accessed_files.iter().any(|f| {
            let f_lower = f.to_lowercase();
            f_lower.ends_with(".sql") || f_lower.ends_with(".prisma") || f_lower.contains("migration") || f_lower.contains("schema") || f_lower.contains("db.rs")
        });
        let has_ui_files = accessed_files.iter().any(|f| {
            let f_lower = f.to_lowercase();
            f_lower.ends_with(".tsx") || f_lower.ends_with(".jsx") || f_lower.ends_with(".css") || f_lower.ends_with(".html")
        });
        let has_sec_files = accessed_files.iter().any(|f| {
            let f_lower = f.to_lowercase();
            f_lower.contains("security") || f_lower.contains("policy") || f_lower.contains("auth")
        });

        if is_db_skill {
            let matches_goal = db_keywords.iter().any(|kw| goal_text.contains(kw));
            return matches_goal || has_db_files;
        }

        if is_ui_skill {
            let matches_goal = ui_keywords.iter().any(|kw| goal_text.contains(kw));
            return matches_goal || has_ui_files;
        }

        if is_sec_skill {
            let matches_goal = sec_keywords.iter().any(|kw| goal_text.contains(kw));
            return matches_goal || has_sec_files;
        }

        true
    }

    /// Generates bounded instructions for skills.
    fn generate_skill_fragments(&self, ctx: &RunContext) -> String {
        let mut skill_fragments = Vec::new();
        let snapshot = self.state.registry.skills.snapshot();
        let mut tokens_accum = 0;
        const MAX_SKILL_TOKENS: usize = 2000;

        for skill_name in &ctx.skills {
            if !self.should_load_skill(skill_name, ctx) {
                tracing::debug!("🧠 [ContextManager] Skipped loading skill '{}' (progressive disclosure)", skill_name);
                continue;
            }
            if let Some(skill) = snapshot.skills.get(skill_name) {
                let mut fragment = String::new();
                if let Some(instructions) = &skill.full_instructions {
                    fragment.push_str(&format!(
                        "### [{}] Full Instructions:\n{}\n",
                        skill.name, instructions
                    ));
                }
                if let Some(constraints) = &skill.negative_constraints {
                    if !constraints.is_empty() {
                        fragment.push_str(&format!(
                            "### [{}] Negative Constraints (PROHIBITED USES):\n- {}\n",
                            skill.name,
                            constraints.join("\n- ")
                        ));
                    }
                }
                let fragment_tokens = count_tokens_fast(&fragment);
                if tokens_accum + fragment_tokens > MAX_SKILL_TOKENS {
                    skill_fragments.push("- ... [Additional skill instructions truncated due to token budget]".to_string());
                    break;
                }
                tokens_accum += fragment_tokens;
                skill_fragments.push(fragment);
            }
        }
        if skill_fragments.is_empty() {
            String::new()
        } else {
            format!(
                "\nSKILL-SPECIFIC DEEP INSTRUCTIONS:\n{}\n",
                skill_fragments.join("\n\n")
            )
        }
    }

    /// Generates bounded instructions for workflows.
    fn generate_workflow_fragments(&self, ctx: &RunContext) -> String {
        let mut workflow_fragments = Vec::new();
        let snapshot = self.state.registry.skills.snapshot();
        let mut tokens_accum = 0;
        const MAX_WORKFLOW_TOKENS: usize = 1500;

        for workflow_name in &ctx.workflows {
            if let Some(wf) = snapshot.workflows.get(workflow_name) {
                let fragment = format!(
                    "### [{}] Workflow Procedure:\n{}",
                    wf.name, wf.content
                );
                let fragment_tokens = count_tokens_fast(&fragment);
                if tokens_accum + fragment_tokens > MAX_WORKFLOW_TOKENS {
                    workflow_fragments.push("- ... [Additional workflow procedures truncated due to token budget]".to_string());
                    break;
                }
                tokens_accum += fragment_tokens;
                workflow_fragments.push(fragment);
            }
        }
        if workflow_fragments.is_empty() {
            String::new()
        } else {
            format!(
                "\nWORKFLOW-SPECIFIC PROCEDURES:\n{}\n",
                workflow_fragments.join("\n\n")
            )
        }
    }

    /// Performs semantic search across the global swarm intelligence vault,
    /// bounding the query logging to debug level and fencing untrusted vault contents.
    async fn gather_global_intelligence(&self, ctx: &RunContext, query: &str) -> Result<String, crate::error::AppError> {
        tracing::debug!("🧠 [Global Intelligence] Agent {} querying global vault (len: {})", ctx.agent_id, query.len());
        
        #[cfg(feature = "vector-memory")]
        {
            let api_key = ctx.model_config.api_key.clone().unwrap_or_else(|| {
                self.state.registry.providers.get(&ctx.model_config.provider.to_string())
                    .and_then(|p| p.api_key.clone())
                    .unwrap_or_default()
            });
            let http_client = self.state.resources.http_client.clone();

            let vec = crate::agent::memory::get_gemini_embedding(&http_client, &api_key, query).await?;
            let vault = self.state.resources.get_swarm_vault().await?;
            let results = vault.search_knowledge(vec, 3).await?;
            
            if !results.is_empty() {
                let formatted = format!("Relevant global intelligence retrieved:\n- {}", results.join("\n- "));
                return Ok(self.fence_untrusted_data("global_vault", &formatted));
            }
        }

        Ok("No relevant global intelligence found in the vault.".to_string())
    }

    /// Dynamically manifests tool definitions for the LLM provider.
    ///
    /// Manifests the agent's toolbelt, enforcing hierarchy-based access control,
    /// dynamic skills, and MCP tools with strict parameter isolation and zero cross-agent cache leaks.
    pub(crate) async fn build_tools(&self, ctx: &RunContext) -> ToolDefinition {
        if !ctx.model_config.supports_native_tools() {
            return ToolDefinition {
                function_declarations: vec![],
            };
        }

        let mut function_declarations = Vec::new();

        // 1. Inject Registered Tools (Core, Filesystem, Advanced)
        let tool_list = self.state.registry.tool_registry.list_tools();
        for tool in tool_list {
            if self.is_registered_tool_eligible(ctx, &tool.name) {
                function_declarations.push(FunctionDeclaration {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    parameters: tool.parameters.clone(),
                });
            }
        }

        // 2. Dynamic Skills / Execution Scripts
        let snapshot = self.state.registry.skills.snapshot();
        for entry in snapshot.skills.iter() {
            let skill = entry.value();
            if self.state.resources.acl.is_tool_allowed(
                &ctx.agent_id,
                &ctx.role,
                ctx.authority_level,
                &skill.name,
            ) {
                let has_skill = ctx.skills.is_empty()
                    || ctx.skills.contains(&skill.name)
                    || ctx.skills.iter().any(|s| {
                        s == "all" || s == "execution" || s == "operations"
                    });
                if has_skill && !function_declarations.iter().any(|t| t.name == skill.name) {
                    if !ctx.safe_mode || SAFE_MODE_ALLOWED_TOOLS.contains(&skill.name.as_str()) {
                        function_declarations.push(FunctionDeclaration {
                            name: skill.name.clone(),
                            description: skill.description.clone(),
                            parameters: skill.schema.clone(),
                        });
                    }
                }
            }
        }

        // 3. Special Utility: Confidence Halting (Guarded to avoid duplicate declaration)
        if !function_declarations.iter().any(|f| f.name == "set_confidence") {
            function_declarations.push(FunctionDeclaration {
                name: "set_confidence".to_string(),
                description: "Signals your current confidence in the answer. If score >= act_threshold, reasoning halts early.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "score": { "type": "number", "description": "Confidence score between 0.0 and 1.0." }
                    },
                    "required": ["score"]
                }),
            });
        }

        // 4. Dynamic MCP Tools (Strictly disabled in safe mode; enforced by namespace & ACL)
        if !ctx.safe_mode {
            let mcp_tools = self
                .state
                .registry
                .mcp_host
                .list_tools(&ctx.skills, &snapshot.skills)
                .await;
            for tool in mcp_tools {
                if self.state.resources.acl.is_tool_allowed(&ctx.agent_id, &ctx.role, ctx.authority_level, &tool.name) {
                    // Strict Membership Filter (MCP): Only explicit skill assignments or server:<name> prefix
                    let is_allowed = ctx.skills.contains(&tool.name)
                        || (tool.name.starts_with("server:") && ctx.skills.iter().any(|s| s == "mcp" || s == "all" || s == &tool.name));
                    
                    if is_allowed && !function_declarations.iter().any(|f| f.name == tool.name) {
                        function_declarations.push(FunctionDeclaration {
                            name: tool.name,
                            description: tool.description,
                            parameters: tool.input_schema,
                        });
                    }
                }
            }
        }

        ToolDefinition {
            function_declarations,
        }
    }

    /// Central check for filesystem interaction permissions.
    fn has_file_system_capability(&self, ctx: &RunContext) -> bool {
        ctx.skills.iter().any(|s| {
            s == "filesystem"
                || s == "read_file"
                || s == "write_file"
                || s == "list_files"
                || s == "delete_file"
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::agent::runner::{AgentRunner, RunContext};
    use crate::agent::types::ModelConfig;
    use std::sync::Arc;
    use dashmap::DashMap;

    #[tokio::test]
    async fn test_shorten_system_tokens() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        
        let raw = "STATUS: success. RESULT: Finding for mission was successful.";
        let encoded = runner.shorten_system_tokens(raw);
        
        assert!(encoded.contains("*ok*"));
        assert!(encoded.contains("RES:"));
        assert!(encoded.contains("FND|"));
        assert!(!encoded.contains("STATUS: success"));

        // Boundary checks: words like "okay" and identifiers like "temperature_sensor" must remain uncorrupted
        let raw_boundary = "STATUS: okay and temperature_sensor = 25";
        let encoded_boundary = runner.shorten_system_tokens(raw_boundary);
        assert!(encoded_boundary.contains("STATUS: okay"), "Word boundary must protect 'okay'");
        assert!(encoded_boundary.contains("temperature_sensor"), "Identifiers must not be corrupted");
    }

    #[tokio::test]
    async fn test_context_pruning_basic() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        let mut ctx = RunContext::default();
        ctx.model_config = ModelConfig {
            tpm: Some(100), // Tiny limit
            ..Default::default()
        };

        let identity = "I am an agent. ".repeat(20);
        let memory = "I remember things. ".repeat(20);
        let repo_map = "This is a big repo map. ".repeat(100);
        let swarm_context = "This is shared findings. ".repeat(1000);

        let (pruned_map, pruned_swarm) = runner.prune_context(
            &ctx, &identity, &memory, &repo_map, &swarm_context
        );

        // Should have pruned something to fit within the 100 token limit
        assert!(pruned_swarm.len() < swarm_context.len());
        assert!(!pruned_map.is_empty());
    }

    #[tokio::test]
    async fn test_build_tools_ceo_restriction() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        let mut ctx = RunContext::default();
        ctx.agent_id = AGENT_CEO.to_string();
        ctx.skills = vec!["read_file".to_string()];
        ctx.model_config.provider = crate::agent::types::ModelProvider::Gemini; // Enable native tools
        
        let tools = runner.build_tools(&ctx).await;
        
        // CEO should NOT have read_file (it's blocked for him to enforce delegation)
        let has_read_file = tools.function_declarations.iter().any(|f| f.name == "read_file");
        assert!(!has_read_file);
        
        // CEO SHOULD have issue_alpha_directive
        let has_directive = tools.function_declarations.iter().any(|f| f.name == "issue_alpha_directive");
        assert!(has_directive);
    }

    #[tokio::test]
    async fn test_build_tools_specialist_tools() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        let mut ctx = RunContext::default();
        ctx.agent_id = "specialist-1".to_string();
        ctx.skills = vec!["read_file".to_string(), "write_file".to_string()];
        ctx.model_config.provider = crate::agent::types::ModelProvider::Gemini; // Enable native tools
        
        let tools = runner.build_tools(&ctx).await;
        
        // Specialist SHOULD have read_file and write_file
        let has_read = tools.function_declarations.iter().any(|f| f.name == "read_file");
        let has_write = tools.function_declarations.iter().any(|f| f.name == "write_file");
        assert!(has_read);
        assert!(has_write);
        
        // Specialist should NOT have issue_alpha_directive
        let has_directive = tools.function_declarations.iter().any(|f| f.name == "issue_alpha_directive");
        assert!(!has_directive);
    }

    #[tokio::test]
    async fn test_context_pruning_token_boundary() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        let mut ctx = RunContext::default();
        
        ctx.model_config = ModelConfig {
            tpm: Some(20), // Extremely tight
            ..Default::default()
        };

        let identity = "Identity ".repeat(20); 
        let memory = "Memory ".repeat(20);     
        let repo_map = "RepoMap ".repeat(100);
        let swarm_context = "Findings ".repeat(200);

        let (pruned_map, pruned_swarm) = runner.prune_context(
            &ctx, &identity, &memory, &repo_map, &swarm_context
        );

        // repo_map should be significantly truncated or empty to fit the 20 token budget
        assert!(pruned_map.len() < repo_map.len());
        assert!(pruned_swarm.len() < swarm_context.len());
    }

    #[tokio::test]
    async fn test_build_tools_observer_restriction() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        let mut ctx = RunContext::default();
        ctx.agent_id = "observer-1".to_string();
        ctx.authority_level = crate::agent::types::RoleAuthorityLevel::Observer;
        ctx.skills = vec!["read_file".to_string(), "write_file".to_string()];
        ctx.model_config.provider = crate::agent::types::ModelProvider::Gemini;
        
        let tools = runner.build_tools(&ctx).await;
        
        let has_read = tools.function_declarations.iter().any(|f| f.name == "read_file");
        let has_write = tools.function_declarations.iter().any(|f| f.name == "write_file");
        
        assert!(has_read, "Observer should still be able to read");
        assert!(!has_write, "Observer should NOT be able to write even if skill is assigned");
    }

    #[tokio::test]
    async fn test_system_failure_context_injection() {
        let (tx, _) = tokio::sync::broadcast::channel(1);
        let (etx, _) = tokio::sync::broadcast::channel(1);
        let (atx, _) = tokio::sync::broadcast::channel(1);
        let (ttx, _) = tokio::sync::broadcast::channel(1);
        let (ptx, _) = tokio::sync::broadcast::channel(1);

        let base_dir = std::path::PathBuf::from("/non/existent/path/to/trigger/failure");
        let pool = crate::db::init_db("sqlite::memory:").await.unwrap();
        let permission_policy = Arc::new(crate::security::permissions::PermissionPolicy::new(pool.clone()));
        let (boot_tx, boot_rx) = tokio::sync::watch::channel(true);

        let state = Arc::new(AppState {
            start_time: chrono::Utc::now(),
            boot_gate: (boot_tx, boot_rx),
            comms: Arc::new(crate::state::hubs::comm::CommunicationHub {
                tx, event_tx: etx, telemetry_tx: ttx, audio_stream_tx: atx, pulse_tx: ptx,
                oversight_queue: dashmap::DashMap::new(),
                oversight_resolvers: dashmap::DashMap::new(),
                oversight_answers: dashmap::DashMap::new(),
                active_runners: dashmap::DashMap::new(),
                mission_contexts: dashmap::DashMap::new(),
                event_sequence: std::sync::atomic::AtomicU64::new(0),
            }),
            governance: Arc::new(crate::state::hubs::gov::GovernanceHub {
                auto_approve_safe_skills: std::sync::atomic::AtomicBool::new(true),
                max_agents: std::sync::atomic::AtomicU32::new(10),
                max_clusters: std::sync::atomic::AtomicU32::new(5),
                max_swarm_depth: std::sync::atomic::AtomicU32::new(3),
                max_task_length: std::sync::atomic::AtomicUsize::new(4096),
                default_budget_usd: parking_lot::RwLock::new(0.5),
                active_agents: std::sync::atomic::AtomicU32::new(0),
                recruit_count: std::sync::atomic::AtomicU32::new(0),
                tpm_accumulator: std::sync::atomic::AtomicUsize::new(0),
                privacy_mode: std::sync::atomic::AtomicBool::new(false),
                observed_max_depth: std::sync::atomic::AtomicU32::new(0),
            }),
            registry: Arc::new(crate::state::hubs::reg::RegistryHub {
                lifecycle_locks: crate::state::hubs::reg::lifecycle_locks(),
                agents: dashmap::DashMap::new(),
                providers: dashmap::DashMap::new(),
                provider_health: DashMap::new(),
                provider_failures: DashMap::new(),
                models: DashMap::new(),
                nodes: DashMap::new(),
                skills: Arc::new(crate::agent::script_skills::ScriptSkillsRegistry::mock(base_dir.clone())),
                skill_registry: Arc::new(crate::agent::skill_manifest::SkillRegistry::new()),
                tool_registry: Arc::new(crate::agent::runner::tools::dispatcher::Dispatcher::new().registry),
                mcp_host: Arc::new(crate::agent::mcp::McpHost::new(
                    tokio::sync::broadcast::channel(1).0,
                    None,
                    permission_policy.clone(),
                    Arc::new(crate::secret_redactor::SecretRedactor::noop())
                )),
                hooks: Arc::new(crate::agent::hooks::HooksManager::new(&base_dir)),
            }),
            security: Arc::new(crate::state::hubs::sec::SecurityHub {
                audit_trail: Arc::new(crate::security::audit::MerkleAuditTrail::mock()),
                budget_guard: Arc::new(crate::security::metering::BudgetGuard::mock()),
                shell_scanner: Arc::new(crate::security::scanner::ShellScanner::mock()),
                secret_redactor: Arc::new(crate::secret_redactor::SecretRedactor::noop()),
                system_monitor: Arc::new(crate::security::monitoring::SecurityMonitor::new()),
                permission_policy,
                verification_gate: Arc::new(crate::agent::verification_gate::VerificationGate::default()),
                deploy_token: "test".to_string(),
                deploy_token_old: None,
                deploy_token_new: None,
                token_rotated_at: None,
                token_grace_secs: None,
                conflict: Arc::new(crate::security::conflict::ConflictManager::new()),
            }),
            resources: Arc::new(crate::state::hubs::res::ResourceHub {
                pool,
                http_client: Arc::new(reqwest::Client::new()),
                audio_engine: tokio::sync::OnceCell::new(),
                audio_cache: Arc::new(crate::agent::audio_cache::BunkerCache::mock()),
                code_graph: tokio::sync::OnceCell::new(),
                symbol_graph: tokio::sync::OnceCell::new(),
                tokenizer_bpe: tokio::sync::OnceCell::new(),
                identity_context: tokio::sync::OnceCell::new(),
                memory_context: tokio::sync::OnceCell::new(),
                knowledge_store: tokio::sync::OnceCell::new(),
                rate_limiters: dashmap::DashMap::new(),
                initialization_registry: dashmap::DashMap::new(),
                hardware_profiler: Arc::new(crate::system::profiler::HardwareProfiler::new()),
                acl: Arc::new(crate::services::acl_service::AclService),
                renderer: Arc::new(crate::agent::runner::prompt_renderer::PromptRenderer),
                base_dir: base_dir.clone(),
                arbiter: Arc::new(tokio::sync::Semaphore::new(4)),
                continuity_arbiter: Arc::new(crate::agent::continuity::ContextArbiter::new(Arc::new(
                    crate::agent::continuity::SSDManager::new(base_dir.clone()),
                ))),
                parser: Arc::new(crate::services::parser::SymbolParser::new()),
                obfuscation_salt: uuid::Uuid::new_v4().to_string(),
                git_path: which::which("git").ok(),
                is_rebuilding_symbol_graph: std::sync::atomic::AtomicBool::new(false),
                tool_cache: Arc::new(parking_lot::Mutex::new(crate::agent::runner::tools::cache::SharedToolCache::new())),
                inference_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
                #[cfg(feature = "vector-memory")]
                vector_memory: tokio::sync::OnceCell::new(),
                #[cfg(feature = "vector-memory")]
                swarm_vault: tokio::sync::OnceCell::new(),
            }),
            base_dir,
            actors: tokio::sync::OnceCell::new(),
            event_bus: Arc::new(crate::system::event_bus::SystemEventBus::new()),
        });

        let runner = AgentRunner::new(state);
        let ctx = RunContext::default();
        let payload = crate::agent::types::TaskPayload {
            message: "Test failure".to_string(),
            ..Default::default()
        };

        let prompt = runner.build_system_prompt(&ctx, &payload.message).await;

        assert!(prompt.contains("SYSTEM WARNING"), "Prompt should contain system warning alert");
        assert!(prompt.contains("IDENTITY_FAILURE"), "Prompt should mention IDENTITY_FAILURE");
        assert!(prompt.contains("MEMORY_FAILURE"), "Prompt should mention MEMORY_FAILURE");
    }

    #[test]
    fn test_format_budgeted_items_three_paths() {
        // (a) Happy Path: all items fit within token budget
        let items = vec!["Alpha directive", "Beta directive"];
        let formatted = format_budgeted_items(
            items.clone(),
            500,
            "EMPTY",
            "TRUNCATED",
            |item| format!("- {item}"),
        );
        assert_eq!(formatted, "- Alpha directive\n- Beta directive");

        // (b) Failure / Overflow Path: budget exceeded triggers truncation notice
        let truncated = format_budgeted_items(
            items,
            1, // Tiny budget forces immediate truncation
            "EMPTY",
            "- ... [Truncated]",
            |item| format!("- {item}"),
        );
        assert_eq!(truncated, "- ... [Truncated]");

        // (c) Edge Case Path: empty input list returns fallback string
        let empty: Vec<&str> = Vec::new();
        let fallback = format_budgeted_items(empty, 500, "No active directives.", "TRUNCATED", |s| s.to_string());
        assert_eq!(fallback, "No active directives.");
        assert!(is_orchestrator_identity(AGENT_CEO));
        assert!(!is_orchestrator_identity("specialist-42"));
    }

    #[tokio::test]
    async fn test_safe_mode_deny_by_default() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        let mut ctx = RunContext::default();
        ctx.agent_id = "specialist-safe".to_string();
        ctx.safe_mode = true;
        ctx.skills = vec!["read_file".to_string(), "write_file".to_string(), "execute_shell".to_string()];
        ctx.model_config.provider = crate::agent::types::ModelProvider::Gemini;

        let tools = runner.build_tools(&ctx).await;

        // In safe mode, mutating tools MUST be blocked
        let has_write = tools.function_declarations.iter().any(|f| f.name == "write_file");
        let has_shell = tools.function_declarations.iter().any(|f| f.name == "execute_shell");
        assert!(!has_write, "write_file must be blocked in safe mode");
        assert!(!has_shell, "execute_shell must be blocked in safe mode");

        // Read-only tools in the allow-list MUST be permitted
        let has_read = tools.function_declarations.iter().any(|f| f.name == "read_file");
        assert!(has_read, "read_file must be allowed in safe mode");
    }

    #[tokio::test]
    async fn test_tool_definition_isolation_no_leak() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);

        // Specialist A: allowed read_file only
        let mut ctx_a = RunContext::default();
        ctx_a.agent_id = "specialist-a".to_string();
        ctx_a.skills = vec!["read_file".to_string()];
        ctx_a.model_config.provider = crate::agent::types::ModelProvider::Gemini;
        let tools_a = runner.build_tools(&ctx_a).await;

        // Specialist B: allowed read_file AND write_file
        let mut ctx_b = RunContext::default();
        ctx_b.agent_id = "specialist-b".to_string();
        ctx_b.skills = vec!["read_file".to_string(), "write_file".to_string()];
        ctx_b.model_config.provider = crate::agent::types::ModelProvider::Gemini;
        let tools_b = runner.build_tools(&ctx_b).await;

        assert!(!tools_a.function_declarations.iter().any(|f| f.name == "write_file"), "Specialist A must not have write_file");
        assert!(tools_b.function_declarations.iter().any(|f| f.name == "write_file"), "Specialist B must have write_file");
    }

    #[tokio::test]
    async fn test_untrusted_data_fencing() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let runner = AgentRunner::new(state);
        let fenced = runner.fence_untrusted_data("peer_findings", "Malicious injection attempt");
        assert!(fenced.contains("<<<UNTRUSTED_EXTERNAL_DATA role=\"peer_findings\">>>"));
        assert!(fenced.contains("<<<END_UNTRUSTED_EXTERNAL_DATA>>>"));
        assert!(fenced.contains("Malicious injection attempt"));
    }
}

// Metadata: [synthesis]
