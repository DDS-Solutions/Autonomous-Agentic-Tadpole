//! @docs ARCHITECTURE:SovereignKernel
//!
//! ### Actor Message Protocol
//! Defines the core communication channels for the decoupled kernel.
//! All mutating or I/O-heavy operations must pass through these channels.

use crate::security::audit::AuditEntry;
use crate::error::AppError;
use tokio::sync::oneshot;

pub mod manager;
pub mod audit_actor;

/// ### 🧬 Protocol: SystemMessage
/// Unified message enumeration for the Sovereign Actor Kernel.
#[derive(Debug)]
pub enum SystemMessage {
    /// Request to record an action in the tamper-evident audit trail.
    AuditRecord {
        agent_id: String,
        mission_id: Option<String>,
        user_id: Option<String>,
        action: String,
        params: String,
        resp: oneshot::Sender<Result<AuditEntry, AppError>>,
    },

    /// Request to perform a vector search or metadata query in Memory.
    MemoryQuery {
        query: String,
        limit: usize,
        resp: oneshot::Sender<Result<serde_json::Value, AppError>>,
    },

    /// Request to persist a new memory or knowledge unit.
    MemorySave {
        content: serde_json::Value,
        resp: oneshot::Sender<Result<(), AppError>>,
    },

    /// System-wide shutdown signal to drain actors.
    Shutdown,
}

/// Registry of MPSC senders for system actors.
#[derive(Clone)]
pub struct ActorRegistry {
    pub audit: tokio::sync::mpsc::UnboundedSender<SystemMessage>,
    pub memory: tokio::sync::mpsc::UnboundedSender<SystemMessage>,
}
