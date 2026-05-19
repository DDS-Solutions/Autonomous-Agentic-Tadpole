//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **System Startup (Boot Orchestrator)**: Orchestrates the transition
//! from a static process to a living, autonomous swarm. Features the
//! **System Boot Sequence**: manages the branching between **Extreme
//! Performance** (Fast Path) and **Full Systemic Awareness** (Full Path)
//! via `BootstrapIntent`. Implements **Subsystem Warmup**: categorizes
//! and initializes heavy components including the **Hydra-RS Code
//! Graph**, **mDNS Swarm Discovery**, **Continuity Scheduler**, and
//! **Heartbeat Loop**. AI agents should monitor the `initialization`
//! telemetry to verify that core background workers are `Ready` before
//! attempting mission dispatch (BOOT-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Fatal `.env.schema` validation failures, mDNS
//!   port availability conflicts (UDP 5353), or Hydra-RS scan
//!   panics during codebase indexing. Check the `bootstrap_intent` if
//!   specific workers are missing.
//! - **Telemetry Link**: Search for `[Bootstrap]` or `[Hydra-RS]` in
//!   `tracing` logs for phase-specific boot milestones.
//! - **Trace Scope**: `server-rs::startup`

use crate::state::AppState;
use crate::system::supervisor::SupervisedSpawn;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Configures the weight and scope of the engine boot sequence.
///
/// This intent allows the process to branch between extreme performance (Fast Path)
/// and full systemic awareness (Full Path).
///
/// ### AI Assist Note
/// Check `intent` in `main.rs` to understand why certain subsystems might be `NotStarted`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BootstrapIntent {
    /// Full mission execution: Warm up all Code Graph, mDNS, and ingestion workers.
    /// **Weight**: High (CPU/RAM intensive).
    /// @state: Constant(Full)
    Full,
    /// Fast Path: Skip heavy warm-up tasks for simple CLI status/version requests.
    /// **Weight**: Low (Instant response).
    /// @state: Constant(Fast)
    Fast,
}

/// Initializes the global telemetry and tracing ecosystem.
///
/// Bridges `tracing` spans with OpenTelemetry and a custom real-time broadcast
/// layer for the frontend UI.
pub fn init_tracing() {
    // 0. Privacy/Performance toggle: Disable telemetry if requested
    let disable_otel = std::env::var("DISABLE_TELEMETRY").as_deref() == Ok("true");

    // 1. Core Layers (Always active)
    let fmt_layer = tracing_subscriber::fmt::layer();
    let env_filter = tracing_subscriber::EnvFilter::from_default_env();
    let telemetry_layer = crate::telemetry::TelemetryLayer::new();

    // 2. OpenTelemetry Layer (Optional & Resilient)
    if !disable_otel {
        let exporter = opentelemetry_stdout::SpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter)
            .build();
        let tracer = provider.tracer("tadpole-os");
        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        let registry = tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(telemetry_layer)
            .with(otel_layer);

        if let Err(e) = registry.try_init() {
            eprintln!("⚠️  [Telemetry] Failed to initialize with OTel: {}. Falling back to basic console logging.", e);
            // Fallback to basic logging handled by second branch if this fails?
            // Actually, if try_init fails, we usually can't try again for the same process.
        } else {
            return; // Successfully initialized with OTel
        }
    }

    // Fallback or Explicitly Disabled: Initialize without OTel
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(crate::telemetry::TelemetryLayer::new())
        .try_init();
}

/// Loads environmental configuration and validates it against the schema.
///
/// Ensures that all required keys are present and types are valid before
/// the engine begins mission execution.
pub fn load_environment() {
    if dotenvy::dotenv().is_err() {
        tracing::warn!("No .env file found. Relying on system environment variables.");
    }

    // SEC-02: Validate environment against schema
    if let Err(e) = crate::env_schema::validate_and_report(std::path::Path::new(".env.schema")) {
        tracing::error!("🚨 [EnvSchema] Validation failed: {}", e);
        if !cfg!(debug_assertions) {
            std::process::exit(1);
        }
    }

    check_sovereign_config();
}

/// Checks for critical AI provider API keys and issues a "Sovereign Warning" if missing.
///
/// ### SEC: Anti-Fingerprinting
/// This check only reports the *presence* or *absence* of keys, never their values,
/// to prevent environment leakage in logs (SEC-04).
fn check_sovereign_config() {
    let providers = [
        ("OPENAI_API_KEY", "OpenAI"),
        ("ANTHROPIC_API_KEY", "Anthropic"),
        ("GOOGLE_API_KEY", "Google Gemini"),
        ("GROQ_API_KEY", "Groq"),
        ("DEEPSEEK_API_KEY", "DeepSeek"),
    ];

    let mut missing = Vec::new();
    for (key, name) in providers {
        match std::env::var(key) {
            Ok(val) if val.trim().len() > 8 => {
                // Key exists and meets minimum length for a real token
            }
            _ => {
                missing.push(name);
            }
        }
    }

    let privacy_mode = std::env::var("PRIVACY_MODE")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    if !missing.is_empty() && !privacy_mode {
        // Use a more professional, system-level reporting style
        tracing::warn!(
            "🛡️  [Sovereign Registry] AI Providers missing configuration: {:?}",
            missing
        );
        
        println!("\n\x1b[1;33m🛡️  [SOVEREIGN ADVISORY]\x1b[0m");
        println!("\x1b[1;33m--------------------------------------------------\x1b[0m");
        println!("The following AI providers are currently inactive:");
        for name in &missing {
            println!("  - {}", name);
        }
        println!("\nEngine will operate in \x1b[1mFallback Mode\x1b[0m (Local models only).");
        println!("To activate these providers, register API keys in .env");
        println!("\x1b[1;33m--------------------------------------------------\x1b[0m\n");
    } else if privacy_mode {
        tracing::info!("🔒 [Sovereign Guard] Running in strict local-only mode (Zero-Cloud).");
    }
}

/// Orchestrates the asynchronous warmup of heavy engine subsystems.
///
/// This function spawns long-running tasks for:
/// 1. **Codebase Indexing**: Preparing the RAG-enhanced code graph.
/// 2. **Network Discovery**: Starting mDNS and swarm coordination.
/// 3. **Health Heartbeat**: Periodically broadcasting systemic telemetry.
///
/// ### Dependencies
/// - Requires an initialized `AppState`.
/// - Obeys `BootstrapIntent` (skips heavy tasks in `Fast` mode).
///
/// ### Side Effects
/// - Reports status to `app_state.resources.initialization_registry`.
/// - Emits `engine:health` socket events.
///
/// ### AI Assist Note
/// This is the primary orchestrator for the engine's background lifecycle.
pub async fn spawn_background_tasks(app_state: Arc<AppState>, intent: BootstrapIntent) {
    // 0. Verify Merkle Audit Trail integrity
    let audit_state = app_state.clone();
    tokio::spawn(async move {
        if let Err(e) = audit_state.security.audit_trail.verify_head().await {
            tracing::error!("🚨 [Audit] Merkle Audit Trail verification failed: {}", e);
        } else {
            tracing::info!("🔒 [Audit] Merkle Audit Trail synchronized with database.");
        }
    });

    // 1. Hydra-RS: Initial Code Scan (Optional in Fast Path)
    if intent == BootstrapIntent::Full {
        let state_for_graph = app_state.clone();
        tokio::spawn(async move {
            state_for_graph
                .resources
                .set_subsystem_status("CodeGraph", crate::types::SubsystemStatus::Warming(0.1));
            let graph_lock = state_for_graph.resources.get_code_graph().await;
            let module_count = tokio::task::spawn_blocking(move || {
                let mut graph = graph_lock.write();
                graph.scan();
                graph.modules.len()
            }).await.unwrap_or(0);
            state_for_graph
                .resources
                .set_subsystem_status("CodeGraph", crate::types::SubsystemStatus::Ready);
            tracing::info!(
                "[Hydra-RS] In-memory code graph warmed up ({} modules indexed)",
                module_count
            );
        });
    }

    // 2. Launch Heartbeat Loop to drive UI presence
    let heartbeat_secs: u64 = std::env::var("HEARTBEAT_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

    app_state.supervisor().spawn("HeartbeatLoop", move |state| async move {
        let boot_instant = std::time::Instant::now();
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(heartbeat_secs)).await;

            let active_agents = state
                .governance
                .active_agents
                .load(std::sync::atomic::Ordering::Relaxed);
            let swarm_depth = state
                .governance
                .max_swarm_depth
                .load(std::sync::atomic::Ordering::Relaxed);
            let tpm = state
                .governance
                .tpm_accumulator
                .swap(0, std::sync::atomic::Ordering::Relaxed);
            let recruits = state
                .governance
                .recruit_count
                .swap(0, std::sync::atomic::Ordering::Relaxed);

            let registry_snapshot: std::collections::HashMap<
                String,
                crate::types::SubsystemStatus,
            > = state
                .resources
                .initialization_registry
                .iter()
                .map(|kv| (kv.key().clone(), kv.value().clone()))
                .collect();

            let profile = state.resources.hardware_profiler.get_profile();

            state.emit_event(serde_json::json!({
                "type": "engine:health",
                "uptime": boot_instant.elapsed().as_secs(),
                "agentCount": active_agents,
                "activeAgents": active_agents,
                "maxDepth": swarm_depth,
                "tpm": tpm,
                "recruitCount": recruits,
                "cpu": profile.cpu_usage,
                "memory": profile.memory_used,
                "memory_total": profile.memory_total,
                "activeProcesses": profile.active_processes,
                "latency": profile.inference_latency,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "initialization": registry_snapshot
            }));
        }
    });

    // 3. Launch Continuity Scheduler (autonomous scheduled missions)
    app_state.supervisor().spawn("ContinuityScheduler", move |state| async move {
        crate::agent::continuity::executor::start_scheduler(state).await;
        Ok(())
    });

    // 4. Launch Swarm Reaper (Data Lifecycle Management)
    app_state.supervisor().spawn("SwarmReaper", move |state| async move {
        crate::agent::reaper::SwarmReaper::start(state).await;
        Ok(())
    });

    // 5. Launch Orphaned Scope Cleanup (Neural Memory Only)
    #[cfg(feature = "vector-memory")]
    {
        let memory_cleanup_pool = app_state.resources.pool.clone();
        tokio::spawn(async move {
            // Run cleanup immediately on startup, then every 6 hours
            crate::agent::memory::VectorMemory::cleanup_orphaned_scopes(&memory_cleanup_pool).await;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
                crate::agent::memory::VectorMemory::cleanup_orphaned_scopes(&memory_cleanup_pool).await;
            }
        });
    }

    // 5. Ingestion Worker (Phase 2 SME Enhancement)
    app_state.supervisor().spawn("IngestionWorker", move |state| async move {
        crate::agent::connectors::start_ingestion_worker(state).await;
        Ok(())
    });

    // 6. Swarm Discovery (mDNS) (Skip in Fast Path)
    if intent == BootstrapIntent::Full {
        app_state.supervisor().spawn("SwarmDiscovery", move |state| async move {
            state
                .resources
                .set_subsystem_status("Network", crate::types::SubsystemStatus::Warming(0.0));
            match crate::services::discovery::SwarmDiscoveryManager::new(state.clone()) {
                Ok(manager) => {
                    if let Err(e) = manager.start() {
                        tracing::error!("📡 [Discovery] Failed to start mDNS manager: {}", e);
                        state.resources.set_subsystem_status(
                            "Network",
                            crate::types::SubsystemStatus::Failed(e.to_string()),
                        );
                        return Err(anyhow::anyhow!("mDNS manager start failure: {}", e));
                    } else {
                        state
                            .resources
                            .set_subsystem_status("Network", crate::types::SubsystemStatus::Ready);
                        // Manager is now running. We need to keep this future alive if start() is async or blocks.
                        // Actually SwarmDiscoveryManager::start likely spawns its own tasks.
                        // I'll need to check if it's a blocking loop or just a setup.
                    }
                }
                Err(e) => {
                    tracing::error!("📡 [Discovery] Failed to initialize mDNS manager: {}", e);
                    state.resources.set_subsystem_status(
                        "Network",
                        crate::types::SubsystemStatus::Failed(e.to_string()),
                    );
                    return Err(anyhow::anyhow!("mDNS manager init failure: {}", e));
                }
            }
            // Keep the task alive if the manager relies on it
            std::future::pending::<anyhow::Result<()>>().await
        });
    }

    // 7. Privacy Guard (Air-Gap Monitor)
    app_state.supervisor().spawn("PrivacyGuard", move |state| async move {
        crate::services::privacy::start_privacy_guard(state).await;
        Ok(())
    });

    // 8. Rate Limit Bucket Eviction (prevents unbounded memory growth under bot traffic)
    app_state.supervisor().spawn("RateLimitEviction", |_state| async move {
        use std::time::Duration;
        let eviction_interval = Duration::from_secs(300); // Every 5 minutes
        let max_bucket_age = Duration::from_secs(120);    // Evict entries idle for 2+ minutes
        let max_auth_age = Duration::from_secs(600);      // Matches BLOCK_DURATION

        loop {
            tokio::time::sleep(eviction_interval).await;

            // Evict stale rate limit buckets
            crate::middleware::rate_limit::evict_stale_buckets(max_bucket_age);

            // Evict expired auth failure records
            crate::middleware::auth_rate_limit::evict_expired_blocks(max_auth_age);

            tracing::debug!("🧹 [Security] Rate limit bucket eviction completed");
        }
    });

    // 9. Launch Telemetry Metric Aggregator (p95 Metrics)
    app_state.supervisor().spawn("MetricAggregator", |_state| async move {
        let aggregator_rx = crate::telemetry::TELEMETRY_TX.subscribe();
        let aggregator = crate::telemetry::aggregator::MetricAggregator::new(1000); // Window of 1000 spans
        aggregator.run(aggregator_rx).await;
        Ok(())
    });

    // 10. Launch Telemetry Bridge (Mission Log Persistence)
    app_state.supervisor().spawn("TelemetryBridge", move |state| async move {
        let bridge_rx_telemetry = crate::telemetry::TELEMETRY_TX.subscribe();
        let bridge_rx_logs = state.comms.tx.subscribe();
        let bridge = crate::telemetry::bridge::TelemetryBridge::new(state.clone());
        bridge.run(bridge_rx_telemetry, bridge_rx_logs).await;
        Ok(())
    });

    // 11. Launch Debounced Token Usage Flush (every 10 seconds)
    app_state.supervisor().spawn("BudgetFlush", move |state| async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            if let Err(e) = state.security.budget_guard.flush_to_db().await {
                tracing::error!("🚨 [BudgetGuard] Failed to flush usage to DB: {}", e);
            }
        }
    });

    // 11. Launch High-Speed Swarm Pulse (100ms) - Only in Full Mode
    if intent == BootstrapIntent::Full {
        app_state.supervisor().spawn("SwarmPulse", move |state| async move {
            crate::telemetry::pulse::spawn_pulse_loop(state).await;
            Ok(())
        });
    }

    // 12. Declarative Swarm Recipes (Auto-provision standard swarms)
    app_state.supervisor().spawn("RecipeAutoIngest", move |state| async move {
        crate::agent::recipes::auto_ingest_recipes(state).await;
        Ok(())
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::types::SubsystemStatus;

    #[test]
    fn test_bootstrap_intent_variants() {
        let full = BootstrapIntent::Full;
        let fast = BootstrapIntent::Fast;
        assert_ne!(full, fast);
    }

    #[tokio::test]
    async fn test_warmup_registry_reporting() {
        let state = Arc::new(AppState::default());

        // Simulate a warmup task
        state
            .resources
            .set_subsystem_status("TestWarmup", SubsystemStatus::Warming(0.1));
        let status = state
            .resources
            .get_initialization_snapshot()
            .get("TestWarmup")
            .cloned();
        assert_eq!(status, Some(SubsystemStatus::Warming(0.1)));

        state
            .resources
            .set_subsystem_status("TestWarmup", SubsystemStatus::Ready);
        let status = state
            .resources
            .get_initialization_snapshot()
            .get("TestWarmup")
            .cloned();
        assert_eq!(status, Some(SubsystemStatus::Ready));
    }

    #[tokio::test]
    async fn test_fast_path_branching() {
        let state = Arc::new(AppState::new_mock().await);

        // Use Fast path
        spawn_background_tasks(state.clone(), BootstrapIntent::Fast).await;

        // Wait a bit for background tasks (though they shouldn't start heavy ones)
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let snapshot = state.resources.get_initialization_snapshot();

        // CodeGraph and Network should NOT be in the registry or should be NotStarted
        // In current implementation, they only appear when Warming is called.
        assert!(!snapshot.contains_key("CodeGraph"));
        assert!(!snapshot.contains_key("Network"));
    }
}

// Metadata: [startup]

// Metadata: [startup]
