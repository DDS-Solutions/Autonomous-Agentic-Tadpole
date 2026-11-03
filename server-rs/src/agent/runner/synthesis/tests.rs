//! @docs ARCHITECTURE:Testing
//!
//! ### AI Assist Note
//! **Synthesis Test Suite**: Validates token boundary regex compression, context pruning,
//! CEO/Specialist/Observer tool access control isolation, system failure alerts, and untrusted data fencing.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Tool capability leakage, tokenizer boundary corruption, or failing failure context injection.
//! - **Telemetry Link**: Search `[synthesis_tests]` in tracing logs.
//! - **Trace Scope**: `server-rs::agent::runner::synthesis::tests`

#[cfg(test)]
mod tests {
    use super::super::budget::format_budgeted_items;
    use super::super::prompt::is_orchestrator_identity;
    use crate::agent::constants::*;
    use crate::agent::runner::{AgentRunner, RunContext};
    use crate::agent::types::ModelConfig;
    use crate::state::AppState;
    use dashmap::DashMap;
    use std::sync::Arc;

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
        assert!(
            encoded_boundary.contains("STATUS: okay"),
            "Word boundary must protect 'okay'"
        );
        assert!(
            encoded_boundary.contains("temperature_sensor"),
            "Identifiers must not be corrupted"
        );
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

        let (pruned_map, pruned_swarm) =
            runner.prune_context(&ctx, &identity, &memory, &repo_map, &swarm_context);

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
        let has_read_file = tools
            .function_declarations
            .iter()
            .any(|f| f.name == "read_file");
        assert!(!has_read_file);

        // CEO SHOULD have issue_alpha_directive
        let has_directive = tools
            .function_declarations
            .iter()
            .any(|f| f.name == "issue_alpha_directive");
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
        let has_read = tools
            .function_declarations
            .iter()
            .any(|f| f.name == "read_file");
        let has_write = tools
            .function_declarations
            .iter()
            .any(|f| f.name == "write_file");
        assert!(has_read);
        assert!(has_write);

        // Specialist should NOT have issue_alpha_directive
        let has_directive = tools
            .function_declarations
            .iter()
            .any(|f| f.name == "issue_alpha_directive");
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

        let (pruned_map, pruned_swarm) =
            runner.prune_context(&ctx, &identity, &memory, &repo_map, &swarm_context);

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

        let has_read = tools
            .function_declarations
            .iter()
            .any(|f| f.name == "read_file");
        let has_write = tools
            .function_declarations
            .iter()
            .any(|f| f.name == "write_file");

        assert!(has_read, "Observer should still be able to read");
        assert!(
            !has_write,
            "Observer should NOT be able to write even if skill is assigned"
        );
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
        let permission_policy = Arc::new(crate::security::permissions::PermissionPolicy::new(
            pool.clone(),
        ));
        let (boot_tx, boot_rx) = tokio::sync::watch::channel(true);

        let state = Arc::new(AppState {
            start_time: chrono::Utc::now(),
            boot_gate: (boot_tx, boot_rx),
            comms: Arc::new(crate::state::hubs::comm::CommunicationHub {
                tx,
                event_tx: etx,
                telemetry_tx: ttx,
                audio_stream_tx: atx,
                pulse_tx: ptx,
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
                skills: Arc::new(crate::agent::script_skills::ScriptSkillsRegistry::mock(
                    base_dir.clone(),
                )),
                skill_registry: Arc::new(crate::agent::skill_manifest::SkillRegistry::new()),
                tool_registry: Arc::new(
                    crate::agent::runner::tools::dispatcher::Dispatcher::new().registry,
                ),
                mcp_host: Arc::new(crate::agent::mcp::McpHost::new(
                    tokio::sync::broadcast::channel(1).0,
                    None,
                    permission_policy.clone(),
                    Arc::new(crate::secret_redactor::SecretRedactor::noop()),
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
                verification_gate: Arc::new(
                    crate::agent::verification_gate::VerificationGate::default(),
                ),
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
                continuity_arbiter: Arc::new(crate::agent::continuity::ContextArbiter::new(
                    Arc::new(crate::agent::continuity::SSDManager::new(base_dir.clone())),
                )),
                parser: Arc::new(crate::services::parser::SymbolParser::new()),
                obfuscation_salt: uuid::Uuid::new_v4().to_string(),
                git_path: which::which("git").ok(),
                is_rebuilding_symbol_graph: std::sync::atomic::AtomicBool::new(false),
                tool_cache: Arc::new(parking_lot::Mutex::new(
                    crate::agent::runner::tools::cache::SharedToolCache::new(),
                )),
                inference_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
                #[cfg(feature = "vector-memory")]
                vector_memory: tokio::sync::OnceCell::new(),
                #[cfg(feature = "vector-memory")]
                swarm_vault: tokio::sync::OnceCell::new(),
            }),
            base_dir,
            actors: tokio::sync::OnceCell::new(),
            hot_reload_tx: tokio::sync::watch::channel(false).0,
            event_bus: Arc::new(crate::system::event_bus::SystemEventBus::new()),
        });

        let runner = AgentRunner::new(state);
        let ctx = RunContext::default();
        let payload = crate::agent::types::TaskPayload {
            message: "Test failure".to_string(),
            ..Default::default()
        };

        let prompt = runner.build_system_prompt(&ctx, &payload.message).await;

        assert!(
            prompt.contains("SYSTEM WARNING"),
            "Prompt should contain system warning alert"
        );
        assert!(
            prompt.contains("IDENTITY_FAILURE"),
            "Prompt should mention IDENTITY_FAILURE"
        );
        assert!(
            prompt.contains("MEMORY_FAILURE"),
            "Prompt should mention MEMORY_FAILURE"
        );
    }

    #[test]
    fn test_format_budgeted_items_three_paths() {
        // (a) Happy Path: all items fit within token budget
        let items = vec!["Alpha directive", "Beta directive"];
        let formatted = format_budgeted_items(items.clone(), 500, "EMPTY", "TRUNCATED", |item| {
            format!("- {item}")
        });
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
        let fallback =
            format_budgeted_items(empty, 500, "No active directives.", "TRUNCATED", |s| {
                s.to_string()
            });
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
        ctx.skills = vec![
            "read_file".to_string(),
            "write_file".to_string(),
            "execute_shell".to_string(),
        ];
        ctx.model_config.provider = crate::agent::types::ModelProvider::Gemini;

        let tools = runner.build_tools(&ctx).await;

        // In safe mode, mutating tools MUST be blocked
        let has_write = tools
            .function_declarations
            .iter()
            .any(|f| f.name == "write_file");
        let has_shell = tools
            .function_declarations
            .iter()
            .any(|f| f.name == "execute_shell");
        assert!(!has_write, "write_file must be blocked in safe mode");
        assert!(!has_shell, "execute_shell must be blocked in safe mode");

        // Read-only tools in the allow-list MUST be permitted
        let has_read = tools
            .function_declarations
            .iter()
            .any(|f| f.name == "read_file");
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

        assert!(
            !tools_a
                .function_declarations
                .iter()
                .any(|f| f.name == "write_file"),
            "Specialist A must not have write_file"
        );
        assert!(
            tools_b
                .function_declarations
                .iter()
                .any(|f| f.name == "write_file"),
            "Specialist B must have write_file"
        );
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

// Metadata: [synthesis_tests]
