//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Oversight Subsystem Facade**: Decomposed module providing zero-churn re-exports
//! of oversight principals, settings, pending decision queues, quota guards, permission policies,
//! cryptographic audit ledger, and swarm telemetry.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Re-export resolution failure, module routing failure, or governance state mismatch.
//! - **Telemetry Link**: Search for `[oversight]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::oversight`

pub mod principal;
pub mod settings;
pub mod queue;
pub mod quotas;
pub mod policies;
pub mod audit;
pub mod telemetry;

#[cfg(test)]
mod tests;

// Re-export principal types, bounds, and security guardrails
pub use principal::{
    parse_oversight_decision, redact_json_value, validate_identifier, validate_oversight_settings,
    HEALTHY_FAILURE_LIMIT, INTEGRITY_VERIFY_WINDOW, MAX_AGENTS_CEILING, MAX_CLUSTERS_CEILING,
    MAX_DEFAULT_BUDGET_USD_CEILING, MAX_IDENTIFIER_LEN, MAX_SWARM_DEPTH_CEILING,
    MAX_TASK_LENGTH_CEILING, THROTTLED_FAILURE_THRESHOLD, UNSIGNED_CHAIN_SCORE_MULTIPLIER,
    OversightPrincipal,
};

// Re-export governance settings handlers and payloads
pub use settings::{
    get_settings, persist_governance_settings, update_settings, OversightSettingsPayload,
};

// Re-export pending queue query and decision handlers
pub use queue::{decide_oversight, get_pending, PendingOversightQuery};

// Re-export quota management handlers and payloads
pub use quotas::{
    get_mission_quotas, get_security_quotas, update_agent_quota, update_mission_quota,
    UpdateQuotaPayload,
};

// Re-export permission policy handlers and payloads
pub use policies::{get_policies, update_policy, PolicyMode, UpdatePolicyPayload};

// Re-export audit ledger and Merkle integrity verification handlers and types
pub use audit::{
    get_audit_trail, get_integrity_status, get_ledger, OversightAuditEntry,
    SecurityIntegrityResponse,
};

// Re-export telemetry, health, and token burn handlers and types
pub use telemetry::{
    get_agent_health, get_token_burn, AgentTokenBurn, ProviderTokenBurn, RecentTokenBurn,
    TokenBurnReport,
};

// Metadata: [oversight]

