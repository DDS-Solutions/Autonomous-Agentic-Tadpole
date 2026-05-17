//! @docs ARCHITECTURE:State
//!
//! ### AI Assist Note
//! **Defense Core**: Manages tamper-evident auditing and preventative safety
//! checks. Orchestrates the **Merkle Hash-Chain** audit trail,
//! **Budget Metering**, **Shell Safety Scanning**, and **Runtime Secret
//! Redaction**. Ensures operational integrity by enforcing **Hard-Coded
//! Security Boundaries** across all agentic activities.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Merkle chain integrity breach (tampered logs), budget
//!   quota exhaustion blocking valid missions, or shell scanner
//!   false-positives on sanitized command strings.
//! - **Trace Scope**: `server-rs::state::hubs::sec`

use crate::agent::runner::service_traits::SystemMonitorTrait;
use crate::secret_redactor::SecretRedactor;
use crate::security::audit::MerkleAuditTrail;
use crate::security::metering::BudgetGuard;
use crate::security::permissions::PermissionPolicy;
use crate::security::scanner::ShellScanner;
use std::sync::Arc;

/// Hub for tamper-evident auditing and preventative security checks.
pub struct SecurityHub {
    /// Tamper-evident audit trail engine (Merkle Hash Chain).
    pub audit_trail: Arc<MerkleAuditTrail>,
    /// Persistent budget governance and metering engine.
    pub budget_guard: Arc<BudgetGuard>,
    /// Proactive shell safety scanner (API key leak protection).
    pub shell_scanner: Arc<ShellScanner>,
    /// Runtime secret redactor for logs and telemetry.
    pub secret_redactor: Arc<SecretRedactor>,
    /// System resource and environment monitor (trait object for testability).
    pub system_monitor: Arc<dyn SystemMonitorTrait>,
    /// Dynamic tool permission and governance policy engine.
    pub permission_policy: Arc<PermissionPolicy>,
    /// Authentication token for administrative/deploy requests.
    pub deploy_token: String,
}

// Metadata: [sec]

// Metadata: [sec]
