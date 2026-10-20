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

/// Trait implemented by errors to provide recovery classification and actionable remediation.
pub trait Classify {
    /// Returns the operational error class.
    fn error_class(&self) -> ErrorClass;

    /// Returns the blast radius scope.
    fn error_scope(&self) -> ErrorScope;

    /// Returns an actionable, human- and agent-interpretable remediation hint.
    fn remediation_hint(&self) -> &'static str;

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

    fn remediation_hint(&self) -> &'static str {
        match self {
            AppError::RateLimit(_) => "Rate limit exceeded. Wait for cooldown or switch to an alternate provider model.",
            AppError::Unauthorized(_) => "Authentication failed. Verify NEURAL_TOKEN in .env or Settings matches the server bearer token.",
            AppError::Forbidden(_) => "Access denied by security policy. Verify agent authority level or request executive clearance.",
            AppError::BadRequest(_) => "Invalid request parameters. Inspect argument types, required fields, and JSON schema.",
            AppError::ContextOverflow(_) => "Context window exceeded. Compact conversation history, summarize findings, or offload observations.",
            AppError::NotFound(_) => "Target resource not found. Verify file path, workflow ID, or agent ID before retrying.",
            AppError::Conflict(_) => "State conflict detected. Refresh active state or retry with updated concurrency version.",
            AppError::Skill(ref s) => match s {
                crate::agent::skill_error::SkillError::ValidationError(_) => "Skill parameter validation error. Verify required parameter names and types.",
                crate::agent::skill_error::SkillError::RecruitmentFailure { .. } => "Agent recruitment failed. Ensure target specialist is registered and unblocked.",
                crate::agent::skill_error::SkillError::SanitizationViolation(_) => "Command violated safety filter. Remove shell escape characters or unescaped variables.",
            },
            AppError::Graph(_) => "Symbol graph traversal error. Verify symbol name and index status with IntelligenceService.",
            AppError::Runner(ref r) => match r {
                crate::agent::runner::error::RunnerError::BudgetExhausted(_) => "Mission financial budget exhausted. Increase budget limit in task config or request quota extension.",
                crate::agent::runner::error::RunnerError::RecursionBlocked(_) => "Tool loop detected. Change strategy, use a different tool, or request operator guidance.",
                crate::agent::runner::error::RunnerError::SentinelGate(_) => "Sentinel safety gate blocked mutation. Submit mutation proposal for oversight approval.",
                crate::agent::runner::error::RunnerError::Compression(_) => "Context compression failure. Clear transient conversation buffer and re-prompt.",
            },
            AppError::Reqwest(ref e) => {
                if e.is_timeout() {
                    "HTTP request timed out. Verify network latency or increase endpoint timeout."
                } else if e.is_connect() {
                    "Failed to connect to endpoint. Verify host is running and reachable."
                } else {
                    "HTTP request failed. Check remote endpoint status and status code."
                }
            }
            AppError::Sqlx(ref e) => {
                let msg = e.to_string();
                if msg.contains("FOREIGN KEY") {
                    "Foreign key constraint failed. Ensure referenced parent record exists or pass NULL."
                } else if msg.contains("UNIQUE") {
                    "Unique constraint violated. Record with this identifier already exists; use update or select."
                } else if msg.contains("locked") || msg.contains("busy") {
                    "Database is locked by another transaction. Wait 250ms and retry."
                } else {
                    "Database operation failed. Verify SQL schema integrity and connection pool."
                }
            }
            AppError::Io(ref e) => match e.kind() {
                std::io::ErrorKind::NotFound => "File or directory not found. Verify path exists relative to workspace root.",
                std::io::ErrorKind::PermissionDenied => "File permission denied. Ensure file is not locked and has write permissions.",
                std::io::ErrorKind::TimedOut => "IO operation timed out. Check storage subsystem latency and retry.",
                _ => "Filesystem IO failure. Verify disk space, path validity, and process locks.",
            },
            AppError::Serde(_) => "JSON serialization/deserialization failed. Verify that payload is valid, well-formed JSON.",
            AppError::InfrastructureError { .. } => "Underlying infrastructure service failed. Check provider connectivity and daemon health.",
            AppError::QuantizationFallback { .. } => "Out-of-memory detected during inference. Fall back to efficient quantization (q4_K_M).",
            AppError::InternalServerError(ref msg) => {
                let lower = msg.to_lowercase();
                if lower.contains("resource_guard") {
                    "Resource guard tripped. Free system memory or reduce concurrency."
                } else if lower.contains("dns error") || lower.contains("connectionrefused") {
                    "Connection refused. Verify local service daemon is running on expected port."
                } else if lower.contains("invalid api key") {
                    "Provider API key rejected. Update provider credential in .env or Settings."
                } else {
                    "Internal server error. Inspect tracing logs with [error] filter for root cause."
                }
            }
            AppError::IntelBoundaryViolation(_) => "Module boundary violation. Access restricted to approved dependency paths.",
            AppError::IntelPathUnknown(_) => "Intelligence path unknown. Index workspace symbols with symbol graph before querying.",
            AppError::DomainError { .. } => "Domain policy violated. Check domain rules and required prerequisites.",
            AppError::NotImplemented(_) => "Feature is not implemented in this build. Compile with required Cargo feature flags.",
            AppError::WalkDir(_) => "Directory traversal error. Check directory accessibility and symbolic link loops.",
            AppError::Anyhow(_) => "Execution error encountered. Inspect error details and adjust tool inputs.",
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

