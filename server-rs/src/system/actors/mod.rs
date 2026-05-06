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
pub mod memory_actor;
pub mod security_actor;
pub mod skill_actor;

/// ### 🛡️ Protocol: SecurityAction
/// Typed security operations for the Sovereign Kernel.
#[derive(Debug)]
pub enum SecurityAction {
    /// Budget metering: Record a spend amount.
    #[allow(dead_code)]
    Spend { amount: f64 },
    /// Command sanitization: Scan a shell command for malicious intent.
    #[allow(dead_code)]
    Shell { command: String },
    /// PII detection: Scan text for sensitive data.
    #[allow(dead_code)]
    PiiCheck { text: String },
}

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
    #[allow(dead_code)]
    MemoryQuery {
        query: String,
        limit: usize,
        resp: oneshot::Sender<Result<serde_json::Value, AppError>>,
    },

    /// Request to persist a new memory or knowledge unit.
    #[allow(dead_code)]
    MemorySave {
        content: serde_json::Value,
        resp: oneshot::Sender<Result<(), AppError>>,
    },

    /// ### 🧬 Multiversal Protocol
    /// Append a node to the session tree.
    #[allow(dead_code)]
    MemoryAppend {
        mission_id: String,
        parent_id: Option<String>,
        role: String,
        content: String,
        metadata: Option<serde_json::Value>,
        resp: oneshot::Sender<Result<String, AppError>>, // Returns the new Node ID
    },

    /// Traverse upward from a leaf to reconstruct linear history.
    #[allow(dead_code)]
    MemoryTraverse {
        leaf_id: String,
        resp: oneshot::Sender<Result<Vec<serde_json::Value>, AppError>>,
    },

    /// Create a new branch from an existing node.
    #[allow(dead_code)]
    MemoryBranch {
        parent_id: String,
        resp: oneshot::Sender<Result<String, AppError>>, // Returns the new Branch Tip ID
    },

    /// System-wide shutdown signal to drain actors.
    #[allow(dead_code)]
    Shutdown,

    /// Request a security validation (Budget, Shell, PII).
    #[allow(dead_code)]
    SecurityCheck {
        agent_id: String,
        action: SecurityAction,
        resp: oneshot::Sender<Result<bool, AppError>>,
    },
    
    /// Request an autonomous skill scan of the workspace.
    #[allow(dead_code)]
    SkillScan {
        path: String,
        resp: oneshot::Sender<Result<usize, AppError>>,
    },
}

/// Registry of MPSC senders for system actors.
#[derive(Clone)]
pub struct ActorRegistry {
    pub audit: tokio::sync::mpsc::Sender<SystemMessage>,
    pub memory: tokio::sync::mpsc::Sender<SystemMessage>,
    pub security: tokio::sync::mpsc::Sender<SystemMessage>,
    #[allow(dead_code)]
    pub skill: tokio::sync::mpsc::Sender<SystemMessage>,
}
