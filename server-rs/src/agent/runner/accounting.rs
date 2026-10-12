//! @docs ARCHITECTURE:Runner
//! ### AI Assist Note
//! Server-owned usage receipts retained until mission terminal persistence commits.
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Cancellation or unavailable terminal storage.
//! - **Telemetry Link**: Owning runner's AgentExecution span and terminal persistence logs.

use crate::agent::types::TokenUsage;
use std::sync::atomic::{AtomicBool, Ordering};

/// Shared by a mission's contexts; independent of the task running recovery.
#[derive(Default)]
pub struct MissionAccounting {
    pub(crate) task_id: Option<tokio::task::Id>,
    pub(crate) job_run_id: Option<String>,
    usage: parking_lot::Mutex<Option<TokenUsage>>,
    finished: AtomicBool,
    finished_signal: tokio::sync::Notify,
}

impl MissionAccounting {
    pub(crate) fn for_execution(task_id: Option<tokio::task::Id>, job_run_id: Option<String>) -> Self {
        Self { task_id, job_run_id, ..Default::default() }
    }

    pub(crate) fn record(&self, reported: &Option<TokenUsage>) {
        if let Some(reported) = reported {
            let mut receipt = self.usage.lock();
            let total = receipt.get_or_insert_with(TokenUsage::default);
            total.input_tokens = total.input_tokens.saturating_add(reported.input_tokens);
            total.output_tokens = total.output_tokens.saturating_add(reported.output_tokens);
            total.total_tokens = total.total_tokens.saturating_add(reported.total_tokens);
        }
    }

    pub(crate) fn usage(&self) -> Option<TokenUsage> {
        self.usage.lock().clone()
    }

    pub(crate) fn finish(&self) {
        self.finished.store(true, Ordering::Release);
        self.finished_signal.notify_waiters();
    }

    pub(crate) async fn wait_finished(&self) -> Result<(), crate::error::AppError> {
        let signal = self.finished_signal.notified();
        tokio::pin!(signal);
        signal.as_mut().enable();
        if !self.finished.load(Ordering::Acquire) {
            tokio::time::timeout(std::time::Duration::from_secs(10), signal).await
                .map_err(|_| crate::error::AppError::Conflict("Previous mission execution has not stopped".to_string()))?;
        }
        Ok(())
    }
}

// Metadata: [accounting]
