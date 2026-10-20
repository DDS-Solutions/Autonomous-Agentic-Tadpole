//! @docs ARCHITECTURE:Registry
//! 
//! ### AI Assist Note
//! **! @docs ARCHITECTURE:Registry**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[error]` in tracing logs.
//! 
//! @docs ARCHITECTURE:Registry
//! Standardized Error System for the Tadpole OS Tooling Layer.
//! Features RecoveryAction metadata to guide autonomous agent self-annealing.

use crate::error::{AppError, Classify};

/// Defines the recovery path for an autonomous agent.
#[derive(Debug, Clone, serde::Serialize)]
pub enum RecoveryAction {
    /// The agent should wait and retry the exact same call.
    RetryWithBackoff { seconds: u64 },
    /// The agent should refine the input parameters (e.g., path was wrong).
    RefineInput,
    /// The agent should attempt to recruit a specialist with higher authority.
    Escalate,
    /// The mission cannot proceed; fail immediately.
    Abort,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolExecutionError {
    #[allow(dead_code)]
    #[error("Transient Failure: {message}")]
    Transient {
        message: String,
        retry_after: Option<u64>,
    },
    
    #[error("Security Violation: {0}")]
    SecurityBlocked(String),
    
    #[allow(dead_code)]
    #[error("Hierarchy Violation: {0}")]
    HierarchyBlocked(String),
    
    #[allow(dead_code)]
    #[error("Parameter Validation Error: {0}")]
    Validation(String),
    
    #[error("Runtime Execution Failed: {0}")]
    ExecutionFailed(String),
    
    #[error(transparent)]
    AppError(#[from] AppError),
}

impl ToolExecutionError {
    /// Determines if the error is recoverable and provides a strategy.
    pub fn recovery_strategy(&self) -> RecoveryAction {
        match self {
            Self::Transient { retry_after, .. } => {
                RecoveryAction::RetryWithBackoff { seconds: retry_after.unwrap_or(1) }
            },
            Self::Validation(_) => RecoveryAction::RefineInput,
            Self::SecurityBlocked(_) | Self::HierarchyBlocked(_) => RecoveryAction::Escalate,
            Self::AppError(e) => match e {
                AppError::RateLimit(_) => RecoveryAction::RetryWithBackoff { seconds: 5 },
                AppError::Io(io_e) if io_e.kind() == std::io::ErrorKind::TimedOut => {
                    RecoveryAction::RetryWithBackoff { seconds: 2 }
                }
                _ => RecoveryAction::Abort,
            },
            _ => RecoveryAction::Abort,
        }
    }

    pub fn is_transient(&self) -> bool {
        matches!(self.recovery_strategy(), RecoveryAction::RetryWithBackoff { .. })
    }

    /// Actionable remediation hint for the LLM agent or HITL oversight.
    pub fn remediation_hint(&self) -> &'static str {
        match self {
            Self::Transient { .. } => "Transient tool failure. Wait for cooldown and retry with backoff.",
            Self::SecurityBlocked(_) => "Tool execution blocked by security policy. Check authority level or submit for HITL oversight.",
            Self::HierarchyBlocked(_) => "Tool execution blocked by agent hierarchy. Escalate or recruit specialist with appropriate clearance.",
            Self::Validation(_) => "Tool parameter validation failed. Verify argument schema and correct malformed inputs.",
            Self::ExecutionFailed(_) => "Tool runtime failure. Inspect error detail, verify environment, and refine parameters.",
            Self::AppError(e) => e.remediation_hint(),
        }
    }
}




// Metadata: [error]
