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
use once_cell::sync::Lazy;
use regex::Regex;

static EMAIL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap());
static SSN_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap());
static CC_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b\d{4}-\d{4}-\d{4}-\d{4}\b").unwrap());

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
        use crate::utils::normalization::SovereignRefiner;

        match action {
            SecurityAction::Spend { amount } => {
                if let Err(e) = budget_guard.record_usage(agent_id, amount).await {
                    warn!("🛡️ [SecurityActor] Budget violation for {}: {}", agent_id, e);
                    return Ok(false);
                }
                Ok(true)
            }
            SecurityAction::Shell { command } => {
                let refined_command = SovereignRefiner::refine(&command);
                match shell_scanner.scan(&refined_command) {
                    ScannerResult::Safe => Ok(true),
                    ScannerResult::Risky(reason) => {
                        warn!("🛡️ [SecurityActor] Malicious command blocked for {}: {}", agent_id, reason);
                        Ok(false)
                    }
                }
            }
            SecurityAction::PiiCheck { text } => {
                let refined_text = SovereignRefiner::refine(&text);
                debug!("🛡️ [SecurityActor] PII Check requested for {}", agent_id);

                // 1. Delegate to ShellScanner for specialized secrets
                match shell_scanner.scan(&refined_text) {
                    ScannerResult::Safe => {}
                    ScannerResult::Risky(reason) => {
                        warn!("🛡️ [SecurityActor] Secret leakage detected in PII check for {}: {}", agent_id, reason);
                        return Ok(false);
                    }
                }

                // 2. Additional PII Patterns (Non-secret)
                if EMAIL_REGEX.is_match(&refined_text) {
                    warn!("🛡️ [SecurityActor] PII Leakage (Email Address) detected for {}", agent_id);
                    return Ok(false);
                }
                if SSN_REGEX.is_match(&refined_text) {
                    warn!("🛡️ [SecurityActor] PII Leakage (SSN) detected for {}", agent_id);
                    return Ok(false);
                }
                if CC_REGEX.is_match(&refined_text) {
                    warn!("🛡️ [SecurityActor] PII Leakage (Credit Card) detected for {}", agent_id);
                    return Ok(false);
                }
                Ok(true)
            }
        }
    }
}
