//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **Error Classification & Fault Taxonomy Engine**:
//! Provides structured classification of domain, infrastructure, and runtime errors.
//! Replaces brittle string-pattern matching (`if detail.contains("...")`) with a typed
//! classification taxonomy governing retry policies, self-annealing, escalation,
//! and circuit breakers.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[error_classify]` in tracing logs.
//! - **Trace Scope**: `server-rs::error::classify`

use serde::{Deserialize, Serialize};
use crate::error::AppError;

/// High-level operational class of an error, determining the recovery protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorClass {
    /// Transient errors like rate limits (429), gateway timeouts (504), temporary connection drops.
    /// Recovery: Retry with exponential backoff and jitter.
    Transient,

    /// Repairable errors like malformed JSON, schema validation failure, missing parameter,
    /// or command syntax errors that the LLM agent can correct via self-annealing re-prompting.
    /// Recovery: Capture as `OBSERVATION: [TOOL_ERROR]` or run extraction annealer without aborting.
    Repairable,

    /// Terminal/unrecoverable errors like revoked API keys, unauthorized access (401),
    /// hard permission denials (403), or budget exhaustion.
    /// Recovery: Immediately abort execution; do not retry.
    Terminal,

    /// Infrastructure failures like SQLite database lock timeout, out of disk space,
    /// or memory constraint signals (`RESOURCE_GUARD`).
    /// Recovery: Throttle, pause, or suspend mission; alert operator.
    Infrastructure,

    /// Internal software faults, assertion failures, or unhandled invariant violations.
    /// Recovery: Catch, isolate blast radius, record crash diagnostics, return RFC 9457 500.
    Fault,
}

/// Blast radius scope of an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorScope {
    /// Affects only a single HTTP request or tool invocation.
    Call,
    /// Affects a single reasoning turn; can be retried or annealed within the same mission.
    Turn,
    /// Affects the entire active mission; requires suspension or termination of the mission.
    Mission,
    /// Affects the agent actor instance.
    Agent,
    /// Affects an entire system actor or background subsystem (e.g. MemoryActor).
    Subsystem,
    /// Affects the operating system process (requires restart).
    Process,
}

/// Trait implemented by errors to provide recovery classification.
pub trait Classify {
    /// Returns the operational error class.
    fn error_class(&self) -> ErrorClass;

    /// Returns the blast radius scope.
    fn error_scope(&self) -> ErrorScope;

    /// Returns true if the error is transient and can be retried automatically.
    fn is_retryable(&self) -> bool {
        matches!(self.error_class(), ErrorClass::Transient)
    }

    /// Returns true if the error is repairable by the agent (e.g., via re-prompting).
    fn is_repairable(&self) -> bool {
        matches!(self.error_class(), ErrorClass::Repairable)
    }

    /// Returns true if the error is terminal and must abort execution immediately.
    fn is_fatal(&self) -> bool {
        matches!(self.error_class(), ErrorClass::Terminal | ErrorClass::Infrastructure)
    }
}

impl Classify for AppError {
    fn error_class(&self) -> ErrorClass {
        match self {
            AppError::RateLimit(_) => ErrorClass::Transient,

            AppError::Unauthorized(_) | AppError::Forbidden(_) => ErrorClass::Terminal,

            AppError::BadRequest(_) | AppError::ContextOverflow(_) => ErrorClass::Repairable,

            AppError::NotFound(_) | AppError::Conflict(_) => ErrorClass::Repairable,

            AppError::Skill(_) | AppError::Graph(_) => ErrorClass::Repairable,

            AppError::Runner(ref r) => match r {
                crate::agent::runner::error::RunnerError::BudgetExhausted(_) => ErrorClass::Terminal,
                crate::agent::runner::error::RunnerError::RecursionBlocked(_) => ErrorClass::Terminal,
                crate::agent::runner::error::RunnerError::SentinelGate(_) => ErrorClass::Terminal,
                crate::agent::runner::error::RunnerError::Compression(_) => ErrorClass::Repairable,
            },

            AppError::Reqwest(ref e) => {
                if e.is_timeout() || e.is_connect() {
                    ErrorClass::Transient
                } else if let Some(status) = e.status() {
                    if status.as_u16() == 429 || status.is_server_error() {
                        ErrorClass::Transient
                    } else if status.as_u16() == 401 || status.as_u16() == 403 {
                        ErrorClass::Terminal
                    } else {
                        ErrorClass::Repairable
                    }
                } else {
                    ErrorClass::Transient
                }
            }

            AppError::Sqlx(ref e) => match e {
                sqlx::Error::PoolTimedOut | sqlx::Error::Database(_) => ErrorClass::Transient,
                _ => ErrorClass::Infrastructure,
            },

            AppError::Io(ref e) => match e.kind() {
                std::io::ErrorKind::NotFound => ErrorClass::Repairable,
                std::io::ErrorKind::PermissionDenied => ErrorClass::Terminal,
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::ConnectionReset => ErrorClass::Transient,
                _ => ErrorClass::Infrastructure,
            },

            AppError::Serde(_) => ErrorClass::Repairable,

            AppError::InfrastructureError { .. } => ErrorClass::Infrastructure,

            AppError::QuantizationFallback { .. } => ErrorClass::Transient,

            AppError::InternalServerError(ref msg) => {
                let lower = msg.to_lowercase();
                if lower.contains("resource_guard") {
                    ErrorClass::Infrastructure
                } else if lower.contains("dns error")
                    || lower.contains("connectionrefused")
                    || lower.contains("connection reset")
                    || lower.contains("broken pipe")
                {
                    ErrorClass::Transient
                } else if lower.contains("model is required")
                    || lower.contains("invalid_request_error")
                    || lower.contains("invalid api key")
                {
                    ErrorClass::Terminal
                } else {
                    ErrorClass::Fault
                }
            }

            AppError::IntelBoundaryViolation(_) => ErrorClass::Terminal,

            AppError::IntelPathUnknown(_) => ErrorClass::Repairable,

            AppError::DomainError { .. } => ErrorClass::Repairable,

            AppError::NotImplemented(_) => ErrorClass::Terminal,

            AppError::WalkDir(_) => ErrorClass::Repairable,

            AppError::Anyhow(_) => ErrorClass::Fault,
        }
    }

    fn error_scope(&self) -> ErrorScope {
        match self {
            AppError::RateLimit(_) => ErrorScope::Call,

            AppError::Unauthorized(_) | AppError::Forbidden(_) => ErrorScope::Mission,

            AppError::BadRequest(_) => ErrorScope::Turn,

            AppError::NotFound(_) | AppError::Conflict(_) | AppError::ContextOverflow(_) => ErrorScope::Turn,

            AppError::Skill(_) | AppError::Graph(_) => ErrorScope::Turn,

            AppError::Runner(ref r) => match r {
                crate::agent::runner::error::RunnerError::BudgetExhausted(_) => ErrorScope::Mission,
                crate::agent::runner::error::RunnerError::RecursionBlocked(_) => ErrorScope::Mission,
                crate::agent::runner::error::RunnerError::SentinelGate(_) => ErrorScope::Turn,
                crate::agent::runner::error::RunnerError::Compression(_) => ErrorScope::Turn,
            },

            AppError::Reqwest(ref e) => {
                if let Some(status) = e.status() {
                    if status.as_u16() == 401 || status.as_u16() == 403 {
                        return ErrorScope::Mission;
                    }
                }
                ErrorScope::Call
            }

            AppError::Sqlx(_) => ErrorScope::Subsystem,

            AppError::Io(_) => ErrorScope::Call,

            AppError::Serde(_) => ErrorScope::Turn,

            AppError::InfrastructureError { .. } => ErrorScope::Subsystem,

            AppError::QuantizationFallback { .. } => ErrorScope::Call,

            AppError::InternalServerError(ref msg) => {
                let lower = msg.to_lowercase();
                if lower.contains("model is required") || lower.contains("invalid api key") {
                    ErrorScope::Mission
                } else if lower.contains("resource_guard") {
                    ErrorScope::Turn
                } else {
                    ErrorScope::Call
                }
            }

            AppError::IntelBoundaryViolation(_) => ErrorScope::Turn,

            AppError::IntelPathUnknown(_) => ErrorScope::Turn,

            AppError::DomainError { .. } => ErrorScope::Turn,

            AppError::NotImplemented(_) => ErrorScope::Call,

            AppError::WalkDir(_) => ErrorScope::Call,

            AppError::Anyhow(_) => ErrorScope::Call,
        }
    }
}

impl AppError {
    /// Logs structured telemetry for classified errors.
    pub fn log_classified(&self) {
        tracing::debug!(
            target: "server-rs::error::classify",
            "[error_classify] class={:?} scope={:?} error={}",
            self.error_class(),
            self.error_scope(),
            self
        );
    }
}

