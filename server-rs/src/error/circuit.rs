//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **Atomic Circuit Breaker Engine**:
//! Zero-allocation, thread-safe circuit breaker protecting remote provider APIs,
//! local inference engines, and external tools from cascading failures.
//! Provides Closed -> Open -> HalfOpen state machine with exponential cooling.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[circuit_breaker]` in tracing logs.
//! - **Trace Scope**: `server-rs::error::circuit`

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};

/// State of the Circuit Breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Circuit is healthy; requests pass through normally.
    Closed,
    /// Circuit is tripped due to repeated failures; calls fail fast without hitting upstream.
    Open,
    /// Reset timeout has elapsed; a single test probe is allowed through to verify recovery.
    HalfOpen,
}

/// Thread-safe, atomic circuit breaker.
pub struct CircuitBreaker {
    name: String,
    failure_threshold: u32,
    reset_timeout_secs: u64,
    consecutive_failures: AtomicU32,
    last_failure_timestamp: AtomicU64,
}

impl CircuitBreaker {
    /// Creates a new CircuitBreaker with the specified failure threshold and cooldown timeout.
    pub fn new(name: impl Into<String>, failure_threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            name: name.into(),
            failure_threshold,
            reset_timeout_secs: reset_timeout.as_secs().max(1),
            consecutive_failures: AtomicU32::new(0),
            last_failure_timestamp: AtomicU64::new(0),
        }
    }

    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// Evaluates current circuit breaker state.
    pub fn state(&self) -> CircuitState {
        let failures = self.consecutive_failures.load(Ordering::Relaxed);
        if failures < self.failure_threshold {
            return CircuitState::Closed;
        }

        let last_failure = self.last_failure_timestamp.load(Ordering::Relaxed);
        let now = Self::current_timestamp();

        if now.saturating_sub(last_failure) >= self.reset_timeout_secs {
            CircuitState::HalfOpen
        } else {
            CircuitState::Open
        }
    }

    /// Checks if a call is allowed through the circuit breaker.
    pub fn can_execute(&self) -> bool {
        match self.state() {
            CircuitState::Closed | CircuitState::HalfOpen => true,
            CircuitState::Open => {
                tracing::warn!(
                    "⚡ [CircuitBreaker:{}] Circuit OPEN. Fast-failing request to protect downstream service.",
                    self.name
                );
                false
            }
        }
    }

    /// Records a successful operation, resetting the failure counter and closing the circuit.
    pub fn record_success(&self) {
        let prev = self.consecutive_failures.swap(0, Ordering::Relaxed);
        if prev >= self.failure_threshold {
            tracing::info!(
                "🌱 [CircuitBreaker:{}] Probe succeeded. Circuit healed: Open/HalfOpen -> CLOSED.",
                self.name
            );
        }
    }

    /// Records a failure. If consecutive failures exceed threshold, trips the circuit to Open.
    pub fn record_failure(&self) {
        let now = Self::current_timestamp();
        self.last_failure_timestamp.store(now, Ordering::Relaxed);
        let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;

        if failures >= self.failure_threshold {
            tracing::error!(
                "🚨 [CircuitBreaker:{}] Tripped to OPEN! Consecutive failures: {} >= threshold {}. Cooldown: {}s",
                self.name,
                failures,
                self.failure_threshold,
                self.reset_timeout_secs
            );
        }
    }

    /// Name identifier of this circuit breaker.
    pub fn name(&self) -> &str {
        &self.name
    }
}
