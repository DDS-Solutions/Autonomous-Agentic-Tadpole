//! @docs ARCHITECTURE:State
//!
//! ### AI Assist Note
//! **Signal Hub**: Orchestrates real-time event broadcasting and oversight
//! resolution. Manages low-latency **WebSocket Streams** for telemetry,
//! system logs, and high-speed **Binary Pulses**. Features a
//! **Resolver-based Oversight Queue** to unblock agentic tasks via
//! human-in-the-loop decisions.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[comm]` in tracing logs.
//! - **Failure Path**: Broadcast channel lag causing dropped telemetry,
//!   oversight resolver timeout (leading to task abortion), or memory
//!   leaks in the `DashMap` due to unconsumed oversight entries.
//! - **Trace Scope**: `server-rs::state::hubs::comm`

use crate::agent::types::OversightEntry;
use crate::types::LogEntry;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot};

/// Handle representing an active agent runner task with its associated TaskId for race-free eviction.
#[derive(Debug, Clone)]
pub struct RunnerHandle {
    pub abort_handle: tokio::task::AbortHandle,
    pub task_id: tokio::task::Id,
}

impl RunnerHandle {
    pub fn new(abort_handle: tokio::task::AbortHandle, task_id: tokio::task::Id) -> Self {
        Self { abort_handle, task_id }
    }

    pub fn abort(&self) {
        self.abort_handle.abort();
    }
}

/// Hub for real-time broadcast and event orchestration.
pub struct CommunicationHub {
    /// Broadcast system logs to all connected UI WebSockets.
    pub tx: broadcast::Sender<LogEntry>,
    /// Dedicated broadcast for Engine events (decisions, lifecycle changes).
    pub event_tx: broadcast::Sender<serde_json::Value>,
    /// Dedicated high-speed broadcast for agent telemetry (thinking, status).
    pub telemetry_tx: broadcast::Sender<serde_json::Value>,
    /// Dedicated high-speed broadcast for neural audio streams (PCM chunks).
    pub audio_stream_tx: broadcast::Sender<Vec<u8>>,
    /// High-speed binary pulse broadcasting for swarm visualization.
    pub pulse_tx: broadcast::Sender<Arc<crate::telemetry::pulse_types::SwarmPulse>>,
    /// Pending Oversight entries awaiting human decision.
    pub oversight_queue: DashMap<String, OversightEntry>,
    /// Resolvers for pending oversight promises.
    pub oversight_resolvers: DashMap<String, oneshot::Sender<bool>>,
    /// Active AbortHandles and TaskIds for running agents, allowing for definitive task cancellation and safe eviction.
    pub active_runners: DashMap<String, RunnerHandle>,
    /// Monotonic sequence counter for outbound engine events.
    pub event_sequence: std::sync::atomic::AtomicU64,
}





// Metadata: [comm]
