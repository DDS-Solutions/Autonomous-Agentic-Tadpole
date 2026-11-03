//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **Synthesis Context & Intelligence Pipeline**: Assembles external directives, peer reviews,
//! identity, long-term memory, vector vault intelligence, skill fragments, and swarm protocols.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Vault search failure, unreadable directive database, or progressive disclosure miss.
//! - **Telemetry Link**: Search `[synthesis_context]` in tracing logs.
//! - **Trace Scope**: `server-rs::agent::runner::synthesis::context`

use super::budget::{count_tokens_fast, format_budgeted_items};
use super::prompt::is_orchestrator_identity;
use super::super::{AgentRunner, RunContext};

impl AgentRunner {
    /// Encloses untrusted or external input in security boundaries with secret redaction
    pub(crate) fn fence_untrusted_data(&self, role: &str, content: &str) -> String {
        let redacted = self.state.security.secret_redactor.redact(content);
        format!(
            "<<<UNTRUSTED_EXTERNAL_DATA role=\"{}\">>>\n{}\n<<<END_UNTRUSTED_EXTERNAL_DATA>>>",
            role, redacted
        )
    }

    pub(crate) async fn fetch_external_context(
        &self,
        ctx: &RunContext,
        budget: usize,
    ) -> (String, String) {
        let (pending_directives, pending_reviews) = tokio::join!(
            super::super::swarm_persistence::get_pending_directives(
                &self.state.resources.pool,
                &ctx.agent_id
            ),
            super::super::swarm_persistence::get_pending_reviews(
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
                    |d| {
                        format!(
                            "- [Directive] From {}: {}",
                            d.source_agent_id, d.instruction
                        )
                    },
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
                "⚠️ REVIEWS_UNAVAILABLE: Database error while querying pending peer reviews."
                    .to_string()
            }
        };

        (directives_str, reviews_str)
    }

    pub(crate) async fn fetch_identity(&self) -> Result<String, crate::error::AppError> {
        let id = self.state.resources.get_identity_context().await?;
        Ok(id)
    }

    pub(crate) async fn fetch_memory(&self) -> Result<String, crate::error::AppError> {
        let mem = self.state.resources.get_memory_context().await?;
        Ok(mem)
    }

    pub(crate) async fn fetch_mission_context(
        &self,
        ctx: &RunContext,
    ) -> Result<String, crate::error::AppError> {
        let context =
            crate::agent::mission::get_mission_context(&self.state.resources.pool, &ctx.mission_id)
                .await?;
        Ok(context)
    }

    /// Dynamic progressive disclosure skill check helper
    pub(crate) fn should_load_skill(&self, skill_name: &str, ctx: &RunContext) -> bool {
        // Core skills that are ALWAYS loaded
        let core_skills = [
            "systematic-debugging",
            "plan-writing",
            "verify-changes",
            "context-compression",
        ];
        if core_skills.contains(&skill_name) {
            return true;
        }

        // Small skill sets (< 6) are loaded in full to ensure all tools have operational instructions
        if ctx.skills.len() <= 6 {
            return true;
        }

        // Database/Migration skills trigger rules
        let db_keywords = [
            "db",
            "database",
            "migration",
            "rls",
            "sql",
            "schema",
            "prisma",
            "postgres",
            "query",
            "table",
        ];
        let is_db_skill = skill_name.contains("database")
            || skill_name.contains("migration")
            || skill_name.contains("rls")
            || skill_name.contains("api-patterns");

        // UI/Frontend skills trigger rules
        let ui_keywords = [
            "ui",
            "frontend",
            "css",
            "html",
            "react",
            "component",
            "styling",
            "layout",
            "page",
            "button",
            "view",
        ];
        let is_ui_skill = skill_name.contains("frontend")
            || skill_name.contains("web-design")
            || skill_name.contains("mobile-design")
            || skill_name.contains("react");

        // Vulnerability/Security skills trigger rules
        let sec_keywords = [
            "security",
            "vulnerability",
            "audit",
            "cve",
            "auth",
            "permission",
            "token",
            "jwt",
            "hacker",
            "attack",
        ];
        let is_sec_skill = skill_name.contains("security")
            || skill_name.contains("vulnerability")
            || skill_name.contains("scanner");

        // Inspect Primary Goal / Task prompt
        let goal_text = ctx.primary_goal.as_deref().unwrap_or("").to_lowercase();

        // Inspect recently accessed files
        let accessed_files = ctx.last_accessed_files.lock();
        let has_db_files = accessed_files.iter().any(|f| {
            let f_lower = f.to_lowercase();
            f_lower.ends_with(".sql")
                || f_lower.ends_with(".prisma")
                || f_lower.contains("migration")
                || f_lower.contains("schema")
                || f_lower.contains("db.rs")
        });
        let has_ui_files = accessed_files.iter().any(|f| {
            let f_lower = f.to_lowercase();
            f_lower.ends_with(".tsx")
                || f_lower.ends_with(".jsx")
                || f_lower.ends_with(".css")
                || f_lower.ends_with(".html")
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
    pub(crate) fn generate_skill_fragments(&self, ctx: &RunContext) -> String {
        let mut skill_fragments = Vec::new();
        let snapshot = self.state.registry.skills.snapshot();
        let mut tokens_accum = 0;
        const MAX_SKILL_TOKENS: usize = 2000;

        for skill_name in &ctx.skills {
            if !self.should_load_skill(skill_name, ctx) {
                tracing::debug!(
                    "🧠 [ContextManager] Skipped loading skill '{}' (progressive disclosure)",
                    skill_name
                );
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
                    skill_fragments.push(
                        "- ... [Additional skill instructions truncated due to token budget]"
                            .to_string(),
                    );
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
    pub(crate) fn generate_workflow_fragments(&self, ctx: &RunContext) -> String {
        let mut workflow_fragments = Vec::new();
        let snapshot = self.state.registry.skills.snapshot();
        let mut tokens_accum = 0;
        const MAX_WORKFLOW_TOKENS: usize = 1500;

        for workflow_name in &ctx.workflows {
            if let Some(wf) = snapshot.workflows.get(workflow_name) {
                let fragment = format!("### [{}] Workflow Procedure:\n{}", wf.name, wf.content);
                let fragment_tokens = count_tokens_fast(&fragment);
                if tokens_accum + fragment_tokens > MAX_WORKFLOW_TOKENS {
                    workflow_fragments.push(
                        "- ... [Additional workflow procedures truncated due to token budget]"
                            .to_string(),
                    );
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
    pub(crate) async fn gather_global_intelligence(
        &self,
        ctx: &RunContext,
        query: &str,
    ) -> Result<String, crate::error::AppError> {
        tracing::debug!(
            "🧠 [Global Intelligence] Agent {} querying global vault (len: {})",
            ctx.agent_id,
            query.len()
        );

        #[cfg(feature = "vector-memory")]
        {
            let api_key = ctx.model_config.api_key.clone().unwrap_or_else(|| {
                self.state
                    .registry
                    .providers
                    .get(&ctx.model_config.provider.to_string())
                    .and_then(|p| p.api_key.clone())
                    .unwrap_or_default()
            });
            let http_client = self.state.resources.http_client.clone();

            let vec =
                crate::agent::memory::get_gemini_embedding(&http_client, &api_key, query).await?;
            let vault = self.state.resources.get_swarm_vault().await?;
            let results = vault.search_knowledge(vec, 3).await?;

            if !results.is_empty() {
                let formatted = format!(
                    "Relevant global intelligence retrieved:\n- {}",
                    results.join("\n- ")
                );
                return Ok(self.fence_untrusted_data("global_vault", &formatted));
            }
        }

        Ok("No relevant global intelligence found in the vault.".to_string())
    }

    /// Generates the File Path Breadcrumbs context.
    pub(crate) fn generate_breadcrumbs_display(&self, ctx: &RunContext) -> String {
        let breadcrumbs = ctx.last_accessed_files.lock();
        if breadcrumbs.is_empty() {
            "None available.".to_string()
        } else {
            breadcrumbs.join("\n- ")
        }
    }

    /// Generates the shared Swarm Protocol rules using the ACL Service.
    pub(crate) fn generate_swarm_protocols(&self, ctx: &RunContext) -> String {
        let mut swarm_protocols = self.state.resources.acl.get_role_protocols(
            &ctx.agent_id,
            &ctx.role,
            ctx.authority_level,
        );
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
}

// Metadata: [synthesis_context]
