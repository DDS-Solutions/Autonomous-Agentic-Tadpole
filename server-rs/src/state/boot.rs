//! @docs ARCHITECTURE:State
//!
//! ### AI Assist Note
//! **State Boot Subsystem**: Orchestrates system initialization, configuration loading,
//! environment validation, database pool initialization, and boot gate coordination.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[boot]` in tracing logs.
//! - **Trace Scope**: `server-rs::state::boot`

use anyhow::Context;
use dashmap::DashMap;
use parking_lot::RwLock;
use sqlx::{Row, SqlitePool};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize};
use std::sync::Arc;
use tokio::sync::{broadcast, OnceCell};

use crate::types::SubsystemStatus;
use crate::error::AppError;
use crate::services::parser::SymbolParser;
use crate::state::hubs::comm::CommunicationHub;
use crate::state::hubs::gov::GovernanceHub;
use crate::state::hubs::reg::RegistryHub;
use crate::state::hubs::res::ResourceHub;
use crate::state::hubs::sec::SecurityHub;
use crate::state::AppState;

/// Validated neural authentication token bundle loaded at boot.
pub(crate) struct BootAuthTokens {
    pub(crate) deploy_token: String,
    pub(crate) deploy_token_old: Option<String>,
    pub(crate) deploy_token_new: Option<String>,
    pub(crate) token_rotated_at: Option<u64>,
    pub(crate) token_grace_secs: Option<u64>,
}

impl AppState {
    /// Mock constructor for unit tests
    #[allow(dead_code)]
    pub async fn new_mock() -> Self {
        Self::new_mock_ext(false).await
    }

    /// Lighter mock constructor that skips database seeding and non-essential subsystems.
    /// Ideal for high-performance unit tests.
    #[allow(dead_code)]
    pub async fn new_minimal_mock() -> Self {
        Self::new_mock_ext(true).await
    }

    /// Creates an AppState using a provided pool. Useful for testing.
    #[allow(dead_code)]
    pub async fn with_pool(pool: SqlitePool) -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(5000);
        let (event_tx, _) = tokio::sync::broadcast::channel(5000);
        let (audio_stream_tx, _) = tokio::sync::broadcast::channel(5000);
        let (telemetry_tx, _) = tokio::sync::broadcast::channel(5000);
        let (pulse_tx, _) = tokio::sync::broadcast::channel(5000);

        // SEC: Use unique temporary directories for test base_dir to avoid shared-state
        // race conditions in parallel unit tests (INFRA-01).
        let base_dir = std::env::temp_dir().join(format!("tadpole-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&base_dir).ok();

        let comms = Arc::new(CommunicationHub {
            tx,
            event_tx,
            telemetry_tx: telemetry_tx.clone(),
            audio_stream_tx,
            pulse_tx,
            oversight_queue: DashMap::new(),
            oversight_resolvers: DashMap::new(),
            oversight_answers: DashMap::new(),
            active_runners: DashMap::new(),
            mission_contexts: DashMap::new(),
            mission_handles: DashMap::new(),
            blackboard: Arc::new(crate::agent::blackboard::SharedBlackboard::new()),
            event_sequence: std::sync::atomic::AtomicU64::new(0),
        });

        let governance = Arc::new(GovernanceHub {
            auto_approve_safe_skills: AtomicBool::new(true),
            max_agents: AtomicU32::new(10),
            max_clusters: AtomicU32::new(5),
            max_swarm_depth: AtomicU32::new(3),
            max_task_length: AtomicUsize::new(4096),
            default_budget_usd: RwLock::new(0.50),
            active_agents: AtomicU32::new(0),
            recruit_count: AtomicU32::new(0),
            tpm_accumulator: AtomicUsize::new(0),
            privacy_mode: AtomicBool::new(false),
            observed_max_depth: AtomicU32::new(0),
        });

        let permission_policy = Arc::new(crate::security::permissions::PermissionPolicy::new(
            pool.clone(),
        ));

        let secret_redactor = Arc::new(crate::secret_redactor::SecretRedactor::noop());

        let registry = Arc::new(RegistryHub {
            lifecycle_locks: crate::state::hubs::reg::lifecycle_locks(),
            agents: DashMap::new(),
            providers: DashMap::new(),
            provider_health: DashMap::new(),
            provider_failures: DashMap::new(),
            models: DashMap::new(),
            nodes: DashMap::new(),
            skills: Arc::new(crate::agent::script_skills::ScriptSkillsRegistry::mock(
                base_dir.clone(),
            )),
            skill_registry: Arc::new(crate::agent::skill_manifest::SkillRegistry::new()),
            mcp_host: Arc::new(crate::agent::mcp::McpHost::new(
                telemetry_tx,
                None,
                permission_policy.clone(),
                secret_redactor.clone(),
            )),
            hooks: Arc::new(crate::agent::hooks::HooksManager::new(&base_dir)),
            tool_registry: Arc::new(
                crate::agent::runner::tools::dispatcher::Dispatcher::new().registry,
            ),
        });

        let _ = sqlx::query(
            "CREATE TABLE IF NOT EXISTS agent_quotas (
                id TEXT PRIMARY KEY NOT NULL,
                entity_id TEXT NOT NULL UNIQUE,
                budget_usd REAL NOT NULL DEFAULT 0.50,
                used_usd REAL NOT NULL DEFAULT 0.0,
                reset_period TEXT NOT NULL DEFAULT 'daily',
                last_reset_at DATETIME NOT NULL,
                next_reset_at DATETIME NOT NULL
            )",
        )
        .execute(&pool)
        .await;
        let _ = sqlx::query(
            "CREATE TABLE IF NOT EXISTS mission_quotas (
                id TEXT PRIMARY KEY NOT NULL,
                cluster_id TEXT NOT NULL UNIQUE,
                budget_usd REAL NOT NULL DEFAULT 5.0,
                used_usd REAL NOT NULL DEFAULT 0.0,
                reset_period TEXT NOT NULL DEFAULT 'daily',
                last_reset_at DATETIME NOT NULL,
                next_reset_at DATETIME NOT NULL
            )",
        )
        .execute(&pool)
        .await;

        let security = Arc::new(SecurityHub {
            audit_trail: Arc::new(
                crate::security::audit::MerkleAuditTrail::with_pool(pool.clone()).await,
            ),
            budget_guard: Arc::new(crate::security::metering::BudgetGuard::new(pool.clone())),
            shell_scanner: Arc::new(crate::security::scanner::ShellScanner::new(
                secret_redactor.clone(),
            )),
            secret_redactor,
            system_monitor: Arc::new(crate::security::monitoring::SecurityMonitor::new())
                as Arc<dyn crate::agent::runner::service_traits::SystemMonitorTrait>,
            permission_policy,
            verification_gate: Arc::new(
                crate::agent::verification_gate::VerificationGate::default(),
            ),
            deploy_token: "test-token".to_string(),
            deploy_token_old: None,
            deploy_token_new: None,
            token_rotated_at: None,
            token_grace_secs: None,
            conflict: Arc::new(crate::security::conflict::ConflictManager::new()),
        });

        let inference_permits = if std::env::var("PRIVACY_MODE")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false)
        {
            1
        } else {
            std::env::var("INFERENCE_CONCURRENCY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(if cfg!(test) { 32 } else { 4 })
        };

        let (boot_tx, boot_rx) = tokio::sync::watch::channel(false);

        Self {
            comms,
            governance,
            registry,
            security,
            resources: Arc::new(ResourceHub {
                pool,
                http_client: Arc::new(reqwest::Client::new()),
                audio_engine: OnceCell::new(),
                audio_cache: Arc::new(crate::agent::audio_cache::BunkerCache::mock()),
                code_graph: OnceCell::new(),
                symbol_graph: OnceCell::new(),
                tokenizer_bpe: OnceCell::new(),
                identity_context: OnceCell::new(),
                memory_context: OnceCell::new(),
                knowledge_store: OnceCell::new(),
                #[cfg(feature = "vector-memory")]
                vector_memory: OnceCell::new(),
                #[cfg(feature = "vector-memory")]
                swarm_vault: OnceCell::new(),
                rate_limiters: DashMap::new(),
                initialization_registry: DashMap::new(),
                hardware_profiler: Arc::new(crate::system::profiler::HardwareProfiler::new()),
                acl: Arc::new(crate::services::acl_service::AclService),
                renderer: Arc::new(crate::agent::runner::prompt_renderer::PromptRenderer),
                base_dir: base_dir.clone(),
                arbiter: Arc::new(tokio::sync::Semaphore::new(4)),
                continuity_arbiter: Arc::new(crate::agent::continuity::ContextArbiter::new(
                    Arc::new(crate::agent::continuity::SSDManager::new(
                        base_dir.join("data").join("cache").join("context"),
                    )),
                )),
                parser: Arc::new(SymbolParser::new()),
                obfuscation_salt: uuid::Uuid::new_v4().to_string(),
                git_path: which::which("git").ok(),
                is_rebuilding_symbol_graph: AtomicBool::new(false),
                tool_cache: Arc::new(parking_lot::Mutex::new(
                    crate::agent::runner::tools::cache::SharedToolCache::new(),
                )),
                inference_semaphore: Arc::new(tokio::sync::Semaphore::new(inference_permits)),
            }),
            base_dir,
            actors: OnceCell::new(),
            boot_gate: (boot_tx, boot_rx),
            hot_reload_tx: tokio::sync::watch::channel(false).0,
            event_bus: Arc::new(crate::system::event_bus::SystemEventBus::new()),
            start_time: chrono::Utc::now(),
        }
    }

    async fn new_mock_ext(minimal: bool) -> Self {
        let database_url = if minimal {
            "sqlite::memory:?skip_seed=true"
        } else {
            "sqlite::memory:"
        };

        let pool = crate::db::init_db(database_url)
            .await
            .map_err(|e| {
                eprintln!("🚨 CRITICAL: Failed to init test DB: {:?}", e);
                e
            })
            .unwrap_or_else(|_| panic!("Failed to initialize in-memory test database"));

        let state = Self::with_pool(pool).await;
        state
            .resources
            .set_subsystem_status("Database", SubsystemStatus::Ready);
        state
            .resources
            .set_subsystem_status("Agents", SubsystemStatus::Ready);
        state
            .resources
            .set_subsystem_status("MCP", SubsystemStatus::Ready);
        state
    }

    pub(crate) fn validate_boot_token(token: &str, var_name: &str) -> Result<(), AppError> {
        if cfg!(test) {
            return Ok(());
        }
        let lower = token.to_lowercase();
        if lower.contains("your-")
            || lower.contains("secret-token")
            || lower.contains("placeholder")
            || lower.contains("previous-token")
            || lower.contains("pending-token")
        {
            return Err(AppError::Unauthorized(format!(
                "🚨 FATAL: {} contains insecure placeholder value. Set a cryptographically secure token (openssl rand -hex 32).",
                var_name
            )));
        }
        if token.len() < 32 {
            return Err(AppError::Unauthorized(format!(
                "🚨 FATAL: {} is too short ({} chars). Cryptographic tokens must be at least 32 characters.",
                var_name,
                token.len()
            )));
        }
        Ok(())
    }

    pub(crate) fn load_auth_tokens_from_env() -> Result<BootAuthTokens, AppError> {
        tracing::info!("🔑 [Auth] Loading Neural Token...");
        let deploy_token = match std::env::var("NEURAL_ENGINE_ACCESS_TOKEN")
            .or_else(|_| std::env::var("NEURAL_TOKEN"))
        {
            Ok(token) => {
                let trimmed = token.trim().to_string();
                Self::validate_boot_token(&trimmed, "NEURAL_TOKEN")?;
                trimmed
            }
            Err(_) if cfg!(test) => "ci-test-token-placeholder".to_string(),
            Err(_) => {
                return Err(AppError::Unauthorized(
                    "🚨 FATAL: NEURAL_TOKEN or NEURAL_ENGINE_ACCESS_TOKEN environment variable MUST be set for the engine to start.".to_string(),
                ))
            }
        };

        let deploy_token_old = match std::env::var("NEURAL_TOKEN_OLD")
            .ok()
            .map(|s| s.trim().to_string())
        {
            Some(t) if !t.is_empty() => {
                Self::validate_boot_token(&t, "NEURAL_TOKEN_OLD")?;
                Some(t)
            }
            _ => None,
        };

        let deploy_token_new = match std::env::var("NEURAL_TOKEN_NEW")
            .ok()
            .map(|s| s.trim().to_string())
        {
            Some(t) if !t.is_empty() => {
                Self::validate_boot_token(&t, "NEURAL_TOKEN_NEW")?;
                Some(t)
            }
            _ => None,
        };

        let token_rotated_at = std::env::var("NEURAL_TOKEN_ROTATED_AT")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok());
        let token_grace_secs = std::env::var("NEURAL_TOKEN_GRACE_SECS")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok());

        Ok(BootAuthTokens {
            deploy_token,
            deploy_token_old,
            deploy_token_new,
            token_rotated_at,
            token_grace_secs,
        })
    }

    pub(crate) fn build_governance_from_env() -> Arc<GovernanceHub> {
        Arc::new(GovernanceHub {
            auto_approve_safe_skills: AtomicBool::new(
                std::env::var("AUTO_APPROVE_SAFE_SKILLS")
                    .map(|s| s == "true")
                    .unwrap_or(false),
            ),
            max_agents: AtomicU32::new(
                std::env::var("MAX_AGENTS")
                    .map(|s| s.parse().unwrap_or(50))
                    .unwrap_or(50),
            ),
            max_clusters: AtomicU32::new(
                std::env::var("MAX_CLUSTERS")
                    .map(|s| s.parse().unwrap_or(10))
                    .unwrap_or(10),
            ),
            max_swarm_depth: AtomicU32::new(
                std::env::var("MAX_SWARM_DEPTH")
                    .map(|s| s.parse().unwrap_or(5))
                    .unwrap_or(5),
            ),
            max_task_length: AtomicUsize::new(
                std::env::var("MAX_TASK_LENGTH")
                    .map(|s| s.parse().unwrap_or(32768))
                    .unwrap_or(32768),
            ),
            default_budget_usd: RwLock::new(
                std::env::var("DEFAULT_AGENT_BUDGET_USD")
                    .map(|s| s.parse().unwrap_or(1.0))
                    .unwrap_or(1.0),
            ),
            active_agents: AtomicU32::new(0),
            recruit_count: AtomicU32::new(0),
            tpm_accumulator: AtomicUsize::new(0),
            privacy_mode: AtomicBool::new(
                std::env::var("PRIVACY_MODE")
                    .map(|v| v.to_lowercase() == "true")
                    .unwrap_or(false),
            ),
            observed_max_depth: AtomicU32::new(0),
        })
    }

    /// ### 🏁 Boot Sequence: Engine Initialization (new)
    pub async fn new() -> Result<Self, AppError> {
        dotenvy::dotenv().ok();
        let (boot_tx, boot_rx) = tokio::sync::watch::channel(false);

        let (tx, _) = broadcast::channel(5000);
        let (event_tx, _) = broadcast::channel(5000);
        let (audio_stream_tx, _) = broadcast::channel(5000);
        let (pulse_tx, _) = broadcast::channel(5000);
        let telemetry_tx = crate::telemetry::TELEMETRY_TX.clone();

        let base_dir = std::env::var("WORKSPACE_ROOT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                let current = std::env::current_dir().unwrap_or_default();
                if current.ends_with("server-rs") {
                    current.parent().unwrap_or(&current).to_path_buf()
                } else {
                    current
                }
            });

        tracing::info!("🏁 [Engine] Starting AppState initialization...");

        // 1. Security: Load Neural Token
        let auth_tokens = Self::load_auth_tokens_from_env()?;

        // 2. Initialize DB
        let database_url = if cfg!(test) {
            "sqlite::memory:".to_string()
        } else {
            std::env::var("DATABASE_URL").unwrap_or_else(|_| {
                let db_path = base_dir.join("data").join("tadpole.db");
                format!("sqlite:{}", db_path.display())
            })
        };

        tracing::info!("🗄️ [Database] Connecting to: {}", database_url);
        let pool = match crate::db::init_db(&database_url).await {
            Ok(p) => {
                tracing::info!("✅ [Database] Pool established successfully.");
                p
            }
            Err(e) => {
                tracing::error!(
                    "🚨 [Database] FATAL: Failed to initialize database pool at {}: {:?}",
                    database_url,
                    e
                );
                return Err(AppError::from(e));
            }
        };

        // 3. Load Registries
        let (providers, models, agents) = crate::state::persist::hydrate_registries(&base_dir, &pool).await;

        let http_client = Arc::new(
            reqwest::Client::builder()
                .user_agent("TadpoleOS/1.1.59")
                .pool_max_idle_per_host(10)
                .pool_idle_timeout(std::time::Duration::from_secs(60))
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(600))
                .tcp_nodelay(true)
                .build()
                .context("Failed to build HTTP client")?,
        );

        let audio_cache_path = base_dir.join("data").join("audio_cache.db");
        tracing::info!(
            "🚀 [Engines] Initializing Audio Cache at {}...",
            audio_cache_path.display()
        );
        let audio_cache =
            match crate::agent::audio_cache::BunkerCache::new(audio_cache_path.clone()).await {
                Ok(cache) => Arc::new(cache),
                Err(e) => {
                    tracing::warn!(
                        "⚠️ [Engines] Audio Cache failed to initialize at {}: {:?}. Falling back to no-op mode.",
                        audio_cache_path.display(),
                        e
                    );
                    Arc::new(crate::agent::audio_cache::BunkerCache::new_noop().await)
                }
            };

        let secret_redactor = Arc::new(crate::secret_redactor::SecretRedactor::from_env());

        // 4. Assemble Hubs
        tracing::info!("💠 [Hubs] Assembling Communication Hub...");
        let comms = Arc::new(CommunicationHub {
            tx: tx.clone(),
            event_tx: event_tx.clone(),
            telemetry_tx,
            audio_stream_tx,
            pulse_tx,
            oversight_queue: DashMap::new(),
            oversight_resolvers: DashMap::new(),
            oversight_answers: DashMap::new(),
            active_runners: DashMap::new(),
            mission_contexts: DashMap::new(),
            mission_handles: DashMap::new(),
            blackboard: Arc::new(crate::agent::blackboard::SharedBlackboard::new()),
            event_sequence: std::sync::atomic::AtomicU64::new(0),
        });

        tracing::info!("💠 [Hubs] Assembling Governance Hub...");
        let governance = Self::build_governance_from_env();

        // 🏛️ [Hydration] Hydrate governance settings from SQLite if persisted
        match sqlx::query(
            "SELECT auto_approve_safe_skills, privacy_mode, max_agents, max_clusters, max_swarm_depth, max_task_length, default_budget_usd \
             FROM governance_settings WHERE id = 1",
        )
        .fetch_optional(&pool)
        .await
        {
            Ok(Some(row)) => {
                if let Ok(val) = row.try_get::<i64, _>("auto_approve_safe_skills") {
                    governance.auto_approve_safe_skills.store(val != 0, std::sync::atomic::Ordering::SeqCst);
                }
                if let Ok(val) = row.try_get::<i64, _>("privacy_mode") {
                    governance.privacy_mode.store(val != 0, std::sync::atomic::Ordering::SeqCst);
                }
                if let Ok(val) = row.try_get::<i64, _>("max_agents") {
                    if val > 0 {
                        let clamped = (val as u64).min(crate::routes::oversight::MAX_AGENTS_CEILING as u64) as u32;
                        governance.max_agents.store(clamped, std::sync::atomic::Ordering::SeqCst);
                    }
                }
                if let Ok(val) = row.try_get::<i64, _>("max_clusters") {
                    if val > 0 {
                        let clamped = (val as u64).min(crate::routes::oversight::MAX_CLUSTERS_CEILING as u64) as u32;
                        governance.max_clusters.store(clamped, std::sync::atomic::Ordering::SeqCst);
                    }
                }
                if let Ok(val) = row.try_get::<i64, _>("max_swarm_depth") {
                    if val > 0 {
                        let clamped = (val as u64).min(crate::routes::oversight::MAX_SWARM_DEPTH_CEILING as u64) as u32;
                        governance.max_swarm_depth.store(clamped, std::sync::atomic::Ordering::SeqCst);
                    }
                }
                if let Ok(val) = row.try_get::<i64, _>("max_task_length") {
                    if val > 0 {
                        let clamped = (val as u64).min(crate::routes::oversight::MAX_TASK_LENGTH_CEILING as u64) as usize;
                        governance.max_task_length.store(clamped, std::sync::atomic::Ordering::SeqCst);
                    }
                }
                if let Ok(val) = row.try_get::<f64, _>("default_budget_usd") {
                    if val.is_finite() && val >= 0.0 && val <= crate::routes::oversight::MAX_DEFAULT_BUDGET_USD_CEILING {
                        *governance.default_budget_usd.write() = val;
                    }
                }
                tracing::info!("🏛️ [Governance] Hydrated governance settings from SQLite");
            }
            Ok(None) => {
                tracing::info!("🏛️ [Governance] No persisted governance settings row found (using boot defaults)");
            }
            Err(e) => {
                tracing::warn!("⚠️ [Governance] Could not hydrate governance settings from SQLite: {:?}", e);
            }
        }

        let mcp_config_path = base_dir.join(".agent").join("mcp_config.json");
        let mcp_config_opt = if mcp_config_path.exists() {
            Some(mcp_config_path)
        } else {
            None
        };

        tracing::info!("🛰️ [Registry] Initializing Script Skills Registry...");
        let script_skills = Arc::new(
            crate::agent::script_skills::ScriptSkillsRegistry::new()
                .await
                .map_err(|e| {
                    AppError::InternalServerError(format!(
                        "Failed to initialize script skills registry: {}",
                        e
                    ))
                })?,
        );

        tracing::info!("🛰️ [Registry] Loading Skill Manifests...");
        let skill_registry = Arc::new(crate::agent::skill_manifest::SkillRegistry::load_all());

        let permission_policy = Arc::new(crate::security::permissions::PermissionPolicy::new(
            pool.clone(),
        ));

        tracing::info!(
            "🛰️ [Registry] Initializing MCP Host (Config: {:?})...",
            mcp_config_opt
        );
        let mcp_host = Arc::new(crate::agent::mcp::McpHost::new(
            event_tx.clone(),
            mcp_config_opt,
            permission_policy.clone(),
            secret_redactor.clone(),
        ));

        tracing::info!("🛰️ [Registry] Initializing Hooks Manager...");
        let hooks_dir = base_dir.join("data");
        let hooks = Arc::new(crate::agent::hooks::HooksManager::new(&hooks_dir));

        tracing::info!("🛰️ [Registry] Initializing Tool Registry...");
        let dispatcher = crate::agent::runner::tools::dispatcher::Dispatcher::new();
        let tool_registry = Arc::new(dispatcher.registry);

        let registry = Arc::new(RegistryHub {
            lifecycle_locks: crate::state::hubs::reg::lifecycle_locks(),
            agents,
            providers,
            provider_health: DashMap::new(),
            provider_failures: DashMap::new(),
            models,
            nodes: DashMap::new(),
            skills: script_skills,
            skill_registry,
            mcp_host,
            hooks,
            tool_registry,
        });

        registry.reconcile_agents();

        let audit_trail = Arc::new(crate::security::audit::MerkleAuditTrail::new(pool.clone()));
        if let Err(e) = audit_trail.verify_head().await {
            tracing::warn!("⚠️ [SecurityAudit] Failed to reconcile head with DB: {:?}", e);
        }

        let security = Arc::new(SecurityHub {
            audit_trail,
            budget_guard: Arc::new(crate::security::metering::BudgetGuard::new(pool.clone())),
            shell_scanner: Arc::new(crate::security::scanner::ShellScanner::new(
                secret_redactor.clone(),
            )),
            secret_redactor,
            system_monitor: Arc::new(crate::security::monitoring::SecurityMonitor::new())
                as Arc<dyn crate::agent::runner::service_traits::SystemMonitorTrait>,
            permission_policy,
            verification_gate: Arc::new(
                crate::agent::verification_gate::VerificationGate::default(),
            ),
            deploy_token: auth_tokens.deploy_token,
            deploy_token_old: auth_tokens.deploy_token_old,
            deploy_token_new: auth_tokens.deploy_token_new,
            token_rotated_at: auth_tokens.token_rotated_at,
            token_grace_secs: auth_tokens.token_grace_secs,
            conflict: Arc::new(crate::security::conflict::ConflictManager::new()),
        });

        let cache_dir = std::env::var("TADPOLE_CACHE_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| base_dir.join("data").join("cache").join("context"));

        let inference_permits = if std::env::var("PRIVACY_MODE")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false)
        {
            1
        } else {
            std::env::var("INFERENCE_CONCURRENCY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(if cfg!(test) { 32 } else { 4 })
        };

        let (hot_reload_tx, mut hot_reload_rx) = tokio::sync::watch::channel(false);

        let state = Self {
            comms,
            governance,
            registry,
            security,
            resources: Arc::new(ResourceHub {
                pool: pool.clone(),
                http_client,
                audio_engine: OnceCell::new(),
                audio_cache,
                code_graph: OnceCell::new(),
                symbol_graph: OnceCell::new(),
                tokenizer_bpe: OnceCell::new(),
                identity_context: OnceCell::new(),
                memory_context: OnceCell::new(),
                knowledge_store: OnceCell::new(),
                #[cfg(feature = "vector-memory")]
                vector_memory: OnceCell::new(),
                #[cfg(feature = "vector-memory")]
                swarm_vault: OnceCell::new(),
                rate_limiters: DashMap::new(),
                initialization_registry: DashMap::new(),
                hardware_profiler: Arc::new(crate::system::profiler::HardwareProfiler::new()),
                acl: Arc::new(crate::services::acl_service::AclService),
                renderer: Arc::new(crate::agent::runner::prompt_renderer::PromptRenderer),
                base_dir: base_dir.clone(),
                arbiter: Arc::new(tokio::sync::Semaphore::new(16)),
                continuity_arbiter: Arc::new(crate::agent::continuity::ContextArbiter::new(
                    Arc::new(crate::agent::continuity::SSDManager::new(cache_dir)),
                )),
                parser: Arc::new(SymbolParser::new()),
                obfuscation_salt: uuid::Uuid::new_v4().to_string(),
                git_path: which::which("git").ok(),
                is_rebuilding_symbol_graph: AtomicBool::new(false),
                tool_cache: Arc::new(parking_lot::Mutex::new(
                    crate::agent::runner::tools::cache::SharedToolCache::new(),
                )),
                inference_semaphore: Arc::new(tokio::sync::Semaphore::new(inference_permits)),
            }),
            base_dir,
            actors: OnceCell::new(),
            boot_gate: (boot_tx, boot_rx),
            hot_reload_tx,
            event_bus: Arc::new(crate::system::event_bus::SystemEventBus::new()),
            start_time: chrono::Utc::now(),
        };

        // 🧬 [Evolution] Passive Hot-Reloading Loop
        if !cfg!(test) {
            let registry_handle = state.registry.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            if let Err(e) = registry_handle.skills.reload_all().await {
                                tracing::error!("🚨 [Evolution] Passive hot-reload failure: {:?}", e);
                            }
                        }
                        _ = hot_reload_rx.changed() => {
                            tracing::info!("🛑 [Evolution] Hot-reload loop received shutdown signal");
                            break;
                        }
                    }
                }
            });
        }

        // Initial Statuses
        state
            .resources
            .set_subsystem_status("Database", SubsystemStatus::Ready);
        state
            .resources
            .set_subsystem_status("Agents", SubsystemStatus::Ready);
        state
            .resources
            .set_subsystem_status("MCP", SubsystemStatus::Ready);
        state
            .resources
            .set_subsystem_status("Network", SubsystemStatus::NotStarted);
        state
            .resources
            .set_subsystem_status("CodeGraph", SubsystemStatus::NotStarted);
        state
            .resources
            .set_subsystem_status("Audio", SubsystemStatus::NotStarted);

        Ok(state)
    }

    /// Blocks until the system boot sequence is officially complete.
    pub async fn wait_for_boot(&self) {
        let mut rx = self.boot_gate.1.clone();
        if *rx.borrow() {
            return;
        }
        let _ = rx.changed().await;
    }

    /// ### 🏁 Boot: Notify Boot Complete (notify_boot_complete)
    #[allow(dead_code)]
    pub fn notify_boot_complete(&self) {
        let _ = self.boot_gate.0.send(true);
        tracing::info!("🏁 [Engine] Boot sequence complete. System is MISSION-READY.");
    }
}

impl Default for AppState {
    /// Creates a mock version of the application state for synchronous default contexts.
    fn default() -> Self {
        let (tx, _) = broadcast::channel(1);
        let (event_tx, _) = broadcast::channel(1);
        let (audio_stream_tx, _) = broadcast::channel(1);
        let (telemetry_tx, _) = broadcast::channel(1);
        let (pulse_tx, _) = broadcast::channel(1);

        let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap_or_else(|_| {
            panic!("🚨 CRITICAL: Failed to connect to lazy in-memory SQLite pool for Default AppState");
        });

        let comms = Arc::new(CommunicationHub {
            tx: tx.clone(),
            event_tx,
            telemetry_tx,
            audio_stream_tx,
            pulse_tx,
            oversight_queue: DashMap::new(),
            oversight_resolvers: DashMap::new(),
            oversight_answers: DashMap::new(),
            active_runners: DashMap::new(),
            mission_contexts: DashMap::new(),
            mission_handles: DashMap::new(),
            blackboard: Arc::new(crate::agent::blackboard::SharedBlackboard::new()),
            event_sequence: std::sync::atomic::AtomicU64::new(0),
        });

        let governance = Arc::new(GovernanceHub {
            auto_approve_safe_skills: AtomicBool::new(true),
            max_agents: AtomicU32::new(50),
            max_clusters: AtomicU32::new(10),
            max_swarm_depth: AtomicU32::new(5),
            max_task_length: AtomicUsize::new(32768),
            default_budget_usd: RwLock::new(1.0),
            active_agents: AtomicU32::new(0),
            recruit_count: AtomicU32::new(0),
            tpm_accumulator: AtomicUsize::new(0),
            privacy_mode: AtomicBool::new(false),
            observed_max_depth: AtomicU32::new(0),
        });

        let permission_policy = Arc::new(crate::security::permissions::PermissionPolicy::new(
            pool.clone(),
        ));

        let registry = Arc::new(RegistryHub {
            lifecycle_locks: crate::state::hubs::reg::lifecycle_locks(),
            agents: DashMap::new(),
            providers: DashMap::new(),
            provider_health: DashMap::new(),
            provider_failures: DashMap::new(),
            models: DashMap::new(),
            nodes: DashMap::new(),
            skills: Arc::new(crate::agent::script_skills::ScriptSkillsRegistry::mock(
                std::path::PathBuf::from("tmp"),
            )),
            skill_registry: Arc::new(crate::agent::skill_manifest::SkillRegistry::new()),
            tool_registry: Arc::new(
                crate::agent::runner::tools::dispatcher::Dispatcher::new().registry,
            ),
            mcp_host: Arc::new(crate::agent::mcp::McpHost::new(
                broadcast::channel(1).0,
                None,
                permission_policy.clone(),
                Arc::new(crate::secret_redactor::SecretRedactor::noop()),
            )),
            hooks: Arc::new(crate::agent::hooks::HooksManager::new(
                &std::path::PathBuf::from("tmp"),
            )),
        });

        let security = Arc::new(SecurityHub {
            audit_trail: Arc::new(crate::security::audit::MerkleAuditTrail::mock()),
            budget_guard: Arc::new(crate::security::metering::BudgetGuard::mock()),
            shell_scanner: Arc::new(crate::security::scanner::ShellScanner::mock()),
            secret_redactor: Arc::new(crate::secret_redactor::SecretRedactor::noop()),
            system_monitor: Arc::new(crate::security::monitoring::SecurityMonitor::new()),
            permission_policy,
            verification_gate: Arc::new(
                crate::agent::verification_gate::VerificationGate::default(),
            ),
            deploy_token: format!("mock-{}", uuid::Uuid::new_v4()),
            deploy_token_old: None,
            deploy_token_new: None,
            token_rotated_at: None,
            token_grace_secs: None,
            conflict: Arc::new(crate::security::conflict::ConflictManager::new()),
        });

        let resources = Arc::new(ResourceHub {
            pool: pool.clone(),
            http_client: Arc::new(reqwest::Client::new()),
            audio_engine: OnceCell::new(),
            audio_cache: Arc::new(crate::agent::audio_cache::BunkerCache::mock()),
            code_graph: OnceCell::new(),
            symbol_graph: OnceCell::new(),
            tokenizer_bpe: OnceCell::new(),
            identity_context: OnceCell::new(),
            memory_context: OnceCell::new(),
            knowledge_store: OnceCell::new(),
            #[cfg(feature = "vector-memory")]
            vector_memory: OnceCell::new(),
            #[cfg(feature = "vector-memory")]
            swarm_vault: OnceCell::new(),
            rate_limiters: DashMap::new(),
            initialization_registry: DashMap::new(),
            hardware_profiler: Arc::new(crate::system::profiler::HardwareProfiler::new()),
            acl: Arc::new(crate::services::acl_service::AclService),
            renderer: Arc::new(crate::agent::runner::prompt_renderer::PromptRenderer),
            base_dir: std::path::PathBuf::from("data"),
            arbiter: Arc::new(tokio::sync::Semaphore::new(4)),
            continuity_arbiter: Arc::new(crate::agent::continuity::ContextArbiter::new(Arc::new(
                crate::agent::continuity::SSDManager::new(
                    std::path::PathBuf::from("data").join("cache").join("context"),
                ),
            ))),
            parser: Arc::new(SymbolParser::new()),
            obfuscation_salt: uuid::Uuid::new_v4().to_string(),
            git_path: which::which("git").ok(),
            is_rebuilding_symbol_graph: AtomicBool::new(false),
            tool_cache: Arc::new(parking_lot::Mutex::new(
                crate::agent::runner::tools::cache::SharedToolCache::new(),
            )),
            inference_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
        });

        let (boot_tx, boot_rx) = tokio::sync::watch::channel(false);

        Self {
            comms,
            governance,
            registry,
            security,
            resources,
            base_dir: std::path::PathBuf::from("data"),
            actors: OnceCell::new(),
            boot_gate: (boot_tx, boot_rx),
            hot_reload_tx: tokio::sync::watch::channel(false).0,
            event_bus: Arc::new(crate::system::event_bus::SystemEventBus::new()),
            start_time: chrono::Utc::now(),
        }
    }
}

// Metadata: [boot]

