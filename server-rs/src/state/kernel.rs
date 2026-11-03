//! @docs ARCHITECTURE:State
//!
//! ### AI Assist Note
//! **State Sovereign Kernel Subsystem**: Dispatches security checks, tamper-evident audit records,
//! and neural memory queries/mutations to system background actors with bounded timeouts.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[kernel]` in tracing logs.
//! - **Trace Scope**: `server-rs::state::kernel`

use crate::error::AppError;
use crate::security::audit::AuditEntry;
use crate::state::AppState;
use crate::system::actors::SecurityAction;

impl AppState {
    /// ### 🛡️ [Kernel] Sovereign Audit Dispatch
    /// Dispatches an audit record request to the background AuditActor.
    /// This is the primary method for non-repudiative logging in the 
    /// Autonomous Tadpole Kernel.
    pub async fn record_audit(
        &self,
        agent_id: &str,
        mission_id: Option<&str>,
        user_id: Option<&str>,
        action: &str,
        params: &str,
    ) -> Result<AuditEntry, AppError> {
        // If actors are initialized, use the async dispatch path with 5s upper bound
        if let Some(actors) = self.actors.get() {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let msg = crate::system::actors::SystemMessage::AuditRecord {
                agent_id: agent_id.to_string(),
                mission_id: mission_id.map(|s| s.to_string()),
                user_id: user_id.map(|s| s.to_string()),
                action: action.to_string(),
                params: params.to_string(),
                resp: tx,
            };

            let timeout_res = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                if let Err(e) = actors.audit.send(msg).await {
                    tracing::error!("🚨 [Kernel] Failed to dispatch audit message: {:?}", e);
                    return Err(AppError::InternalServerError(
                        "Audit dispatcher failure".to_string(),
                    ));
                }

                rx.await
                    .map_err(|_| AppError::InternalServerError("Audit response channel dropped".to_string()))?
            })
            .await;

            match timeout_res {
                Ok(res) => res,
                Err(_) => Err(AppError::InternalServerError(
                    "Audit recording timed out after 5s".to_string(),
                )),
            }
        } else {
            // Fallback to direct synchronous/shared-state recording (Legacy/Boot)
            self.security
                .audit_trail
                .record(agent_id, mission_id, user_id, action, params)
                .await
                .map_err(|e| AppError::InternalServerError(e.to_string()))
        }
    }

    /// ### 🧠 [Kernel] Sovereign Memory Save
    /// Dispatches a memory persistence request to the background MemoryActor.
    #[allow(dead_code)]
    pub async fn save_memory_sovereign(
        &self,
        text: &str,
        mission_id: &str,
        vector: Vec<f32>,
    ) -> Result<(), AppError> {
        if let Some(actors) = self.actors.get() {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let content = serde_json::json!({
                "text": text,
                "mission_id": mission_id,
                "vector": vector
            });
            let msg = crate::system::actors::SystemMessage::MemorySave { content, resp: tx };

            crate::system::actors::dispatch(&actors.memory, msg, rx).await
        } else {
            Err(AppError::InternalServerError(
                "MemoryActor not initialized; memory was not saved".to_string(),
            ))
        }
    }

    /// ### 🧠 [Kernel] Sovereign Memory Query
    /// Dispatches a vector search request to the background MemoryActor.
    #[allow(dead_code)]
    pub async fn query_memory_sovereign(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<serde_json::Value, AppError> {
        if let Some(actors) = self.actors.get() {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let msg = crate::system::actors::SystemMessage::MemoryQuery {
                query: query.to_string(),
                limit,
                resp: tx,
            };

            crate::system::actors::dispatch(&actors.memory, msg, rx).await
        } else {
            Err(AppError::InternalServerError(
                "MemoryActor not initialized".to_string(),
            ))
        }
    }

    /// ### 🛡️ [Kernel] Sovereign Security Check
    /// Dispatches a security validation request to the background SecurityActor.
    #[allow(dead_code)]
    pub async fn check_security_sovereign(
        &self,
        agent_id: &str,
        action_type: &str,
        payload: serde_json::Value,
    ) -> Result<bool, AppError> {
        let action = match action_type {
            "spend" => {
                let amount = payload.get("amount").and_then(|v| v.as_f64()).ok_or_else(|| {
                    AppError::BadRequest(
                        "Missing or invalid 'amount' in spend security check".to_string(),
                    )
                })?;
                SecurityAction::Spend { amount }
            }
            "shell" => {
                let command = payload
                    .get("command")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AppError::BadRequest(
                            "Missing or invalid 'command' in shell security check".to_string(),
                        )
                    })?
                    .to_string();
                SecurityAction::Shell { command }
            }
            "pii" => {
                let text = payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AppError::BadRequest(
                            "Missing or invalid 'text' in PII security check".to_string(),
                        )
                    })?
                    .to_string();
                SecurityAction::PiiCheck { text }
            }
            _ => {
                return Err(AppError::BadRequest(format!(
                    "Unknown security action: {}",
                    action_type
                )))
            }
        };

        if let Some(actors) = self.actors.get() {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let msg = crate::system::actors::SystemMessage::SecurityCheck {
                agent_id: agent_id.to_string(),
                action,
                resp: tx,
            };

            let timeout_res = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                if let Err(e) = actors.security.send(msg).await {
                    tracing::error!("🚨 [Kernel] Failed to dispatch security check: {:?}", e);
                    return Err(AppError::InternalServerError(
                        "Security dispatcher failure".to_string(),
                    ));
                }

                rx.await
                    .map_err(|_| AppError::InternalServerError("Security response channel dropped".to_string()))?
            })
            .await;

            match timeout_res {
                Ok(res) => res,
                Err(_) => Err(AppError::InternalServerError(
                    "Security check timed out after 5s".to_string(),
                )),
            }
        } else {
            // Fallback to direct synchronous/shared-state check (Legacy/Boot)
            match action {
                SecurityAction::Spend { amount } => self
                    .security
                    .budget_guard
                    .record_usage(agent_id, amount)
                    .await
                    .map(|_| true)
                    .map_err(|e| AppError::InternalServerError(e.to_string())),
                SecurityAction::Shell { command } => {
                    match self.security.shell_scanner.scan(&command) {
                        crate::security::scanner::ScannerResult::Safe => Ok(true),
                        crate::security::scanner::ScannerResult::Risky(_) => Ok(false),
                    }
                }
                SecurityAction::PiiCheck { .. } => {
                    tracing::warn!("🛡️ [Security] PII check evaluated in uninitialized fallback mode; failing closed.");
                    Ok(false)
                }
            }
        }
    }
}

// Metadata: [kernel]


