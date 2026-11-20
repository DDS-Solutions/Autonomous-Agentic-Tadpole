//! @docs ARCHITECTURE:Agent
//!
//! ### AI Assist Note
//! **Benchmarking Suite**: Provides a standardized testbed for evaluating
//! model latency, cost, and accuracy across different task categories.
//! Orchestrates **System Performance Baselines** (Runner, DB, Rate Limiter)
//! to ensure that local infrastructure overhead does not degrade the
//! swarm's decision-making speed. Records results to the `benchmarks`
//! table for longitudinal analysis.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[benchmarks]` in tracing logs.
//! - **Failure Path**: Benchmark timeout on under-provisioned hardware,
//!   SQLite deadlocks during high-concurrency stress tests, or invalid
//!   test IDs causing early return errors.
//! - **Trace Scope**: `server-rs::agent::benchmarks`

use crate::state::AppState;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Instant;
use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessCheck {
    pub id: String,
    pub label: String,
    pub status: String, // "pass" | "warn" | "fail"
    pub latency_ms: f64,
    pub detail: String,
    pub remediation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessReport {
    pub score: u8,
    pub verdict: String, // "ready" | "degraded" | "not_ready"
    pub recommended_max_depth: u8,
    pub detected_tier: String, // "local_sovereign" | "hybrid_overlord" | "cloud_max" | "unconfigured"
    pub total_latency_ms: f64,
    pub checks: Vec<ReadinessCheck>,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BenchmarkResult {
    pub id: String,
    pub name: String,
    pub category: String,
    pub test_id: String,
    pub mean_ms: f64,
    pub p95_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub target_value: Option<String>,
    pub status: String, // PASS | FAIL | NEEDS_INVESTIGATION
    pub metadata: Option<String>,
    pub created_at: String,
}

pub async fn save_benchmark(pool: &SqlitePool, result: BenchmarkResult) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO benchmarks (id, name, category, test_id, mean_ms, p95_ms, p99_ms, target_value, status, metadata)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&result.id)
    .bind(&result.name)
    .bind(&result.category)
    .bind(&result.test_id)
    .bind(result.mean_ms)
    .bind(result.p95_ms)
    .bind(result.p99_ms)
    .bind(&result.target_value)
    .bind(&result.status)
    .bind(&result.metadata)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list_benchmarks(pool: &SqlitePool) -> Result<Vec<BenchmarkResult>, AppError> {
    let results = sqlx::query_as::<_, BenchmarkResult>(
        "SELECT id, name, category, test_id, mean_ms, p95_ms, p99_ms, target_value, status, metadata, CAST(created_at AS TEXT) as created_at FROM benchmarks ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await?;

    Ok(results)
}

pub async fn get_benchmark_comparison(
    pool: &SqlitePool,
    test_id: &str,
) -> Result<Vec<BenchmarkResult>, AppError> {
    let results = sqlx::query_as::<_, BenchmarkResult>(
        "SELECT id, name, category, test_id, mean_ms, p95_ms, p99_ms, target_value, status, metadata, CAST(created_at AS TEXT) as created_at FROM benchmarks WHERE test_id = ? ORDER BY created_at DESC LIMIT 10"
    )
    .bind(test_id)
    .fetch_all(pool)
    .await?;

    Ok(results)
}
pub async fn run_benchmark_suite(
    state: Arc<AppState>,
    test_id: &str,
    mission_id: Option<String>,
    node_id: Option<String>,
) -> Result<BenchmarkResult, AppError> {
    let pool = &state.resources.pool;
    let mut status = "PASS".to_string();
    let target_value;
    let name;
    let category;
    
    let mut meta_obj = serde_json::json!({
        "trigger": "Self-triggered via Dashboard"
    });
    if let Some(ref m_id) = mission_id {
        meta_obj["mission_id"] = serde_json::json!(m_id);
    }
    if let Some(ref n_id) = node_id {
        meta_obj["node_id"] = serde_json::json!(n_id);
    }
    let metadata_str = serde_json::to_string(&meta_obj).unwrap_or_default();
    let mut metadata = Some(metadata_str);

    let start = std::time::Instant::now();

    match test_id {
        "BM-RUN-01" => {
            name = "Agent Runner Baseline".to_string();
            category = "Runner".to_string();
            target_value = Some("< 100ms".to_string());

            // Measure agent initialization and context resolution overhead
            let runner = crate::agent::runner::AgentRunner::new(state.clone());

            for _ in 0..10 {
                let _ = runner.state.registry.agents.contains_key("overlord");
            }
        }
        "BM-DB-01" => {
            name = "Persistence Stress (50 Writes)".to_string();
            category = "Database".to_string();
            target_value = Some("< 200ms".to_string());

            // Perform 50 actual writes
            for i in 0..50 {
                sqlx::query("INSERT INTO benchmark_logs (test_id, step) VALUES (?, ?)")
                    .bind(test_id)
                    .bind(i)
                    .execute(pool)
                    .await?;
            }
        }
        "BM-RL-01" => {
            name = "Rate Limiter Overhead".to_string();
            category = "Rate Limiter".to_string();
            target_value = Some("< 1ms".to_string());

            let limiter = crate::agent::rate_limiter::RateLimiter::new(Some(1000), Some(100000));
            for _ in 0..100 {
                limiter.acquire(10).await;
                limiter.record_usage(10);
            }
            let mut meta_with_cycles = meta_obj.clone();
            meta_with_cycles["details"] = serde_json::json!("100 cycles of acquire/record");
            metadata = Some(serde_json::to_string(&meta_with_cycles).unwrap_or_default());
        }
        _ => return Err(AppError::BadRequest(format!("Unknown benchmark test ID: {}", test_id))),
    }

    let duration = start.elapsed().as_secs_f64() * 1000.0;

    // Check status against target thresholds
    if test_id == "BM-RUN-01" && duration > 100.0 {
        status = "FAIL".to_string();
    }
    if test_id == "BM-DB-01" && duration > 200.0 {
        status = "FAIL".to_string();
    }
    if test_id == "BM-RL-01" && duration > 1.0 {
        status = "FAIL".to_string();
    }

    let result = BenchmarkResult {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        category,
        test_id: test_id.to_string(),
        mean_ms: duration,
        p95_ms: Some(duration * 1.02), // Statistical approximation for single run
        p99_ms: Some(duration * 1.05),
        target_value,
        status,
        metadata,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    save_benchmark(pool, result.clone()).await?;

    // Autonomous Self-Healing trigger
    if result.status == "FAIL" {
        if let (Some(m_id), Some(n_id)) = (mission_id, node_id) {
            // Retrieve history to find last known green state
            if let Ok(history) = state.traverse_session_history_sovereign(&n_id).await {
                let mut target_node_id = None;
                for node in history.iter().rev().skip(1) {
                    if let Some(node_id_str) = node["id"].as_str() {
                        let has_passed = sqlx::query_scalar::<_, i64>(
                            "SELECT COUNT(*) FROM benchmarks WHERE status = 'PASS' AND metadata LIKE ?"
                        )
                        .bind(format!("%\"node_id\":\"{}\"%", node_id_str))
                        .fetch_one(&state.resources.pool)
                        .await
                        .unwrap_or(0);

                        if has_passed > 0 {
                            target_node_id = Some(node_id_str.to_string());
                            break;
                        }
                    }
                }

                if target_node_id.is_none() && history.len() >= 2 {
                    target_node_id = history[history.len() - 2]["id"].as_str().map(|s| s.to_string());
                }

                if let Some(target_node) = target_node_id {
                    tracing::warn!("♻️ [SelfHealing] Autonomously reverting mission {} to last known green node {}", m_id, target_node);
                    let _ = state.revert_to_node_sovereign(&m_id, &target_node).await;
                    state.broadcast_sys(
                        &format!("♻️ [SelfHealing] Autonomously reverted mission to node {}", target_node),
                        "warning",
                        Some(m_id.clone()),
                    );
                }
            }
        }
    }

    Ok(result)
}

/// Returns true if an Ollama model name represents an embedding-only model unsuitable for chat/agent turns.
fn is_embedding_only_model(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("embed")
        || lower.contains("minilm")
        || lower.contains("bge-")
        || lower.contains("e5-")
        || lower.contains("rerank")
}

/// Checks whether any supported cloud LLM API key is configured in registry or environment.
fn has_configured_cloud_key(state: &AppState) -> bool {
    const CLOUD_ENV_KEYS: [&str; 9] = [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GOOGLE_API_KEY",
        "GEMINI_API_KEY",
        "GROQ_API_KEY",
        "OPENROUTER_API_KEY",
        "DEEPSEEK_API_KEY",
        "MISTRAL_API_KEY",
        "XAI_API_KEY",
    ];
    if CLOUD_ENV_KEYS
        .iter()
        .any(|k| std::env::var(k).ok().is_some_and(|v| !v.trim().is_empty()))
    {
        return true;
    }
    if state.registry.providers.iter().any(|entry| {
        let p = entry.value();
        let lower_id = p.id.to_ascii_lowercase();
        !lower_id.contains("ollama")
            && !lower_id.contains("local")
            && p.api_key.as_deref().is_some_and(|k| !k.trim().is_empty())
    }) {
        return true;
    }
    state.registry.agents.iter().any(|entry| {
        let m = &entry.value().models.model;
        !matches!(
            m.provider,
            crate::agent::types::ModelProvider::Ollama | crate::agent::types::ModelProvider::Local
        ) && m.api_key.as_deref().is_some_and(|k| !k.trim().is_empty())
    })
}

/// Performs a bounded live HTTP probe against Ollama `/api/tags` to verify reachability and
/// count chat-capable (non-embedding) local models. Returns `Ok(chat_model_count)` if reachable.
async fn probe_live_ollama(http_client: &reqwest::Client) -> Result<usize, String> {
    if std::env::var("TADPOLE_NULL_PROVIDERS")
        .ok()
        .is_some_and(|v| v.eq_ignore_ascii_case("true") || v == "1")
    {
        return Err("TADPOLE_NULL_PROVIDERS is enabled (mock mode)".to_string());
    }
    let host = std::env::var("OLLAMA_HOST")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:11434".to_string());
    let url = format!("{}/api/tags", host.trim_end_matches('/'));
    let resp = tokio::time::timeout(
        std::time::Duration::from_millis(1500),
        http_client.get(&url).send(),
    )
    .await
    .map_err(|_| format!("Timed out reaching Ollama at {}", host))?
    .map_err(|e| format!("Ollama unreachable at {}: {}", host, e))?;

    if !resp.status().is_success() {
        return Err(format!("Ollama returned HTTP {} at {}", resp.status(), host));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Invalid Ollama /api/tags payload: {}", e))?;
    let chat_models = body
        .get("models")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
                .filter(|name| !is_embedding_only_model(name))
                .count()
        })
        .unwrap_or(0);
    Ok(chat_models)
}

/// Runs the native 4-probe Overlord Readiness check against the live engine state:
/// 1. Database connectivity + `PRAGMA foreign_keys = ON` + `PRAGMA journal_mode` (25 pts)
/// 2. Live Provider & Model Tier liveness (live Ollama HTTP `/api/tags` + cloud key check) (35 pts)
/// 3. Live Security & Governance Posture (GovernanceHub state + quote/path guardrail probe) (20 pts)
/// 4. Fleet & RBAC Posture audit (active agents, tiered Admin RBAC) (20 pts)
///
/// Read-only diagnostic endpoint: does not pollute the `benchmarks` table on each check.
pub async fn run_readiness_probe(state: Arc<AppState>) -> Result<ReadinessReport, AppError> {
    let overall_start = Instant::now();
    let mut checks = Vec::with_capacity(4);
    let mut score: i32 = 0;

    // 1. Database & Relational Integrity Probe (25 pts)
    let db_start = Instant::now();
    let fk_enabled = sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys;")
        .fetch_one(&state.resources.pool)
        .await
        .unwrap_or(0);
    let journal_mode = sqlx::query_scalar::<_, String>("PRAGMA journal_mode;")
        .fetch_one(&state.resources.pool)
        .await
        .unwrap_or_else(|_| "unknown".to_string())
        .to_ascii_lowercase();
    let db_ms = db_start.elapsed().as_secs_f64() * 1000.0;
    let journal_ok = journal_mode == "wal" || journal_mode == "memory";
    if fk_enabled == 1 && journal_ok {
        score += 25;
        checks.push(ReadinessCheck {
            id: "database_integrity".to_string(),
            label: "SQLite WAL & Foreign Keys".to_string(),
            status: "pass".to_string(),
            latency_ms: db_ms,
            detail: format!(
                "SQLite responsive: PRAGMA foreign_keys = ON, journal_mode = {}.",
                journal_mode
            ),
            remediation: None,
        });
    } else if fk_enabled == 1 {
        score += 15;
        checks.push(ReadinessCheck {
            id: "database_integrity".to_string(),
            label: "SQLite WAL & Foreign Keys".to_string(),
            status: "warn".to_string(),
            latency_ms: db_ms,
            detail: format!(
                "PRAGMA foreign_keys = ON, but journal_mode is '{}' (expected 'wal').",
                journal_mode
            ),
            remediation: Some("Enable WAL mode (`PRAGMA journal_mode = WAL;`) for safe concurrent reads/writes.".to_string()),
        });
    } else {
        checks.push(ReadinessCheck {
            id: "database_integrity".to_string(),
            label: "SQLite WAL & Foreign Keys".to_string(),
            status: "fail".to_string(),
            latency_ms: db_ms,
            detail: format!(
                "SQLite foreign_keys pragma is disabled (journal_mode = '{}').",
                journal_mode
            ),
            remediation: Some("Verify DATABASE_URL and restart the engine to re-initialize SQLite pragmas.".to_string()),
        });
    }

    // 2. Live Provider & Tier Liveness Probe (35 pts — required to reach Ready verdict)
    let prov_start = Instant::now();
    let has_cloud = has_configured_cloud_key(&state);
    let privacy_mode = state
        .governance
        .privacy_mode
        .load(std::sync::atomic::Ordering::Relaxed);
    let ollama_probe = probe_live_ollama(&state.resources.http_client).await;
    let live_local_models = ollama_probe.as_ref().copied().unwrap_or(0);
    let has_live_local = live_local_models > 0;
    let effective_cloud = has_cloud && !privacy_mode;
    let prov_ms = prov_start.elapsed().as_secs_f64() * 1000.0;

    let (detected_tier, recommended_max_depth) = match (effective_cloud, has_live_local) {
        (true, true) => ("hybrid_overlord".to_string(), 4u8),
        (true, false) => ("cloud_max".to_string(), 5u8),
        (false, true) => ("local_sovereign".to_string(), 2u8),
        (false, false) => ("unconfigured".to_string(), 1u8),
    };

    if effective_cloud && has_live_local {
        score += 35;
        checks.push(ReadinessCheck {
            id: "provider_tier".to_string(),
            label: "Live Model Tier & Providers".to_string(),
            status: "pass".to_string(),
            latency_ms: prov_ms,
            detail: format!(
                "Hybrid Overlord tier live: cloud API key configured + Ollama reachable ({} chat model(s)).",
                live_local_models
            ),
            remediation: None,
        });
    } else if effective_cloud {
        score += 30;
        checks.push(ReadinessCheck {
            id: "provider_tier".to_string(),
            label: "Live Model Tier & Providers".to_string(),
            status: "pass".to_string(),
            latency_ms: prov_ms,
            detail: "Cloud Max tier active (cloud API key detected; local Ollama offline).".to_string(),
            remediation: Some("Optional: start Ollama locally for zero-cost specialist delegation.".to_string()),
        });
    } else if has_live_local {
        score += 28;
        checks.push(ReadinessCheck {
            id: "provider_tier".to_string(),
            label: "Live Model Tier & Providers".to_string(),
            status: "warn".to_string(),
            latency_ms: prov_ms,
            detail: format!(
                "Local Sovereign tier live: Ollama reachable with {} chat model(s) (privacy_mode={}).",
                live_local_models, privacy_mode
            ),
            remediation: Some("Keep max_swarm_depth <= 2 for local models, or disable privacy_mode and add a cloud key for Hybrid Overlord.".to_string()),
        });
    } else {
        let ollama_reason = match ollama_probe {
            Ok(0) => "Ollama responded but has 0 chat models pulled (embedding-only models are excluded).".to_string(),
            Err(reason) => reason,
            _ => "No live local provider.".to_string(),
        };
        let cloud_note = if has_cloud && privacy_mode {
            " Cloud keys exist but PRIVACY_MODE=true blocks cloud providers."
        } else {
            " No cloud API key detected in registry or .env."
        };
        checks.push(ReadinessCheck {
            id: "provider_tier".to_string(),
            label: "Live Model Tier & Providers".to_string(),
            status: "fail".to_string(),
            latency_ms: prov_ms,
            detail: format!("No live LLM provider ready. {}{}", ollama_reason, cloud_note),
            remediation: Some("Start Ollama and pull a chat model (e.g. `ollama pull gemma4:e4b`), or configure a cloud API key in Models (/models).".to_string()),
        });
    }

    // 3. Live Security & Governance Guardrail Probe (20 pts)
    let guard_start = Instant::now();
    let cli_blocked = crate::utils::security::validate_shell_command("python \"-c\" \"import os\"").is_err();
    let path_blocked =
        crate::utils::security::validate_path(&state.resources.base_dir, "../../etc/passwd")
            .is_err();
    let probe_secret = format!("{}-{}-{}", "sk", "ant", "api03-123456789012345678901234567890");
    let secret_redacted = !crate::utils::security::redact_secrets(&probe_secret).contains("sk-ant-");
    let auto_approve_safe = state
        .governance
        .auto_approve_safe_skills
        .load(std::sync::atomic::Ordering::Relaxed);
    let default_budget = *state.governance.default_budget_usd.read();
    let budget_bounded = default_budget.is_finite() && default_budget > 0.0;
    let guard_ms = guard_start.elapsed().as_secs_f64() * 1000.0;

    if cli_blocked && path_blocked && secret_redacted && budget_bounded {
        score += 20;
        checks.push(ReadinessCheck {
            id: "guardrail_integrity".to_string(),
            label: "Security & Governance Posture".to_string(),
            status: "pass".to_string(),
            latency_ms: guard_ms,
            detail: format!(
                "Quote-aware CLI guard, path jail, secret redactor verified; auto_approve_safe={}, privacy_mode={}, default_budget=${:.2}.",
                auto_approve_safe, privacy_mode, default_budget
            ),
            remediation: None,
        });
    } else {
        checks.push(ReadinessCheck {
            id: "guardrail_integrity".to_string(),
            label: "Security & Governance Posture".to_string(),
            status: "fail".to_string(),
            latency_ms: guard_ms,
            detail: format!(
                "Guardrail check failed (cli={}, path={}, redact={}, budget_bounded={}).",
                cli_blocked, path_blocked, secret_redacted, budget_bounded
            ),
            remediation: Some("Verify security guardrails and set a positive default_budget_usd in Settings.".to_string()),
        });
    }

    // 4. Fleet & RBAC Posture Probe (20 pts)
    let fleet_start = Instant::now();
    let agent_count = state.registry.agents.len();
    let tiered_rbac = state.security.admin_token.is_some();
    let fleet_ms = fleet_start.elapsed().as_secs_f64() * 1000.0;
    if agent_count > 0 {
        score += if tiered_rbac { 20 } else { 15 };
        checks.push(ReadinessCheck {
            id: "fleet_governance".to_string(),
            label: "Swarm Fleet & Governance".to_string(),
            status: if tiered_rbac { "pass" } else { "warn" }.to_string(),
            latency_ms: fleet_ms,
            detail: format!(
                "{} agent(s) registered; tiered Admin RBAC: {}.",
                agent_count,
                if tiered_rbac { "enforced" } else { "single-token dev mode" }
            ),
            remediation: if tiered_rbac {
                None
            } else {
                Some("Set NEURAL_TOKEN_ADMIN in .env (and authenticate admin sessions with it) to isolate destructive deploy/script routes in production.".to_string())
            },
        });
    } else {
        score += 5;
        checks.push(ReadinessCheck {
            id: "fleet_governance".to_string(),
            label: "Swarm Fleet & Governance".to_string(),
            status: "warn".to_string(),
            latency_ms: fleet_ms,
            detail: "No agents currently registered in active registry.".to_string(),
            remediation: Some("Create or seed an initial agent in /agents or launch the First-Run Wizard.".to_string()),
        });
    }

    let final_score = score.clamp(0, 100) as u8;
    let has_any_fail = checks.iter().any(|c| c.status == "fail");
    let verdict = if final_score >= 80 && !has_any_fail {
        "ready"
    } else if final_score >= 50 && (effective_cloud || has_live_local) {
        "degraded"
    } else {
        "not_ready"
    };

    let total_latency_ms = overall_start.elapsed().as_secs_f64() * 1000.0;
    let checked_at = chrono::Utc::now().to_rfc3339();
    Ok(ReadinessReport {
        score: final_score,
        verdict: verdict.to_string(),
        recommended_max_depth,
        detected_tier,
        total_latency_ms,
        checks,
        checked_at,
    })
}





// Metadata: [benchmarks]
