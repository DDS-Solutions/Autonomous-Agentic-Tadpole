//! @docs ARCHITECTURE:SovereignKernel
//!
//! ### SecurityActor
//! Centralized governance engine for the Sovereign Kernel.
//! Handles budget metering, command sanitization, and PII detection.

use crate::system::actors::{SystemMessage, SecurityAction};
use crate::security::metering::BudgetGuard;
use crate::security::scanner::{ShellScanner, ScannerResult};
use crate::error::AppError;
use tokio::sync::mpsc;
use std::sync::Arc;
use tracing::{info, error, debug, warn};

pub struct SecurityActor {
    receiver: mpsc::Receiver<SystemMessage>,
    budget_guard: Arc<BudgetGuard>,
    shell_scanner: Arc<ShellScanner>,
}

impl SecurityActor {
    pub fn new(
        receiver: mpsc::Receiver<SystemMessage>,
        budget_guard: Arc<BudgetGuard>,
        shell_scanner: Arc<ShellScanner>,
    ) -> Self {
        Self {
            receiver,
            budget_guard,
            shell_scanner,
        }
    }

    pub async fn run(mut self) {
        info!("🛡️ [SecurityActor] Governance loop active. Zero-Trust gates armed.");

        while let Some(msg) = self.receiver.recv().await {
            match msg {
                SystemMessage::SecurityCheck { agent_id, action, resp } => {
                    let budget_guard = self.budget_guard.clone();
                    let shell_scanner = self.shell_scanner.clone();

                    tokio::spawn(async move {
                        let result = Self::handle_check(&agent_id, action, budget_guard, shell_scanner).await;
                        if let Err(e) = resp.send(result) {
                            debug!("🛡️ [SecurityActor] Failed to send security result: {:?}", e);
                        }
                    });
                }
                SystemMessage::Shutdown => {
                    info!("🛡️ [SecurityActor] Shutdown received. Flushing security metrics...");
                    if let Err(e) = self.budget_guard.flush_to_db().await {
                        error!("🛡️ [SecurityActor] Failed to flush budget on shutdown: {}", e);
                    }
                    break;
                }
                _ => {
                    debug!("🛡️ [SecurityActor] Ignoring non-security message type.");
                }
            }
        }
    }

    async fn handle_check(
        agent_id: &str,
        action: SecurityAction,
        budget_guard: Arc<BudgetGuard>,
        shell_scanner: Arc<ShellScanner>,
    ) -> Result<bool, AppError> {
        match action {
            SecurityAction::Spend { amount } => {
                if let Err(e) = budget_guard.record_usage(agent_id, amount).await {
                    warn!("🛡️ [SecurityActor] Budget violation for {}: {}", agent_id, e);
                    return Ok(false);
                }
                Ok(true)
            }
            SecurityAction::Shell { command } => {
                match shell_scanner.scan(&command) {
                    ScannerResult::Safe => Ok(true),
                    ScannerResult::Risky(reason) => {
                        warn!("🛡️ [SecurityActor] Malicious command blocked for {}: {}", agent_id, reason);
                        Ok(false)
                    }
                }
            }
            SecurityAction::PiiCheck { text } => {
                debug!("🛡️ [SecurityActor] PII Check requested for {} (Not yet implemented)", agent_id);
                if text.contains("sk-") {
                    warn!("🛡️ [SecurityActor] Possible API key detected for {}", agent_id);
                    return Ok(false);
                }
                Ok(true)
            }
        }
    }
}
