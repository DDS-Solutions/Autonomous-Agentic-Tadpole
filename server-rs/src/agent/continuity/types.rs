//! @docs ARCHITECTURE:State
//!
//! ### AI Assist Note
//! **Continuity Data Types**: Orchestrates the serialization and
//! state persistence schemas for the Tadpole OS continuity
//! subsystem. Features **Scheduled Job Definitions**: `ScheduledJob`
//! structures define the cron-based execution frequency and budget
//! constraints for autonomous missions. Implements **Execution
//! History Recording**: `ScheduledJobRun` captures the telemetry and
//! outcome (Success/Failure/BudgetExceeded) of previous autonomous
//! cycles. AI agents should monitor `consecutive_failures` and
//! `max_failures` to prevent recursive resource exhaustion in
//! degraded environments (CONT-02).
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[types]` in tracing logs.
//! - **Failure Path**: Deserialization errors for legacy job
//!   metadata, malformed cron-expressions in the database, or
//!   timestamp inconsistencies causing missed execution windows.
//! - **Trace Scope**: `server-rs::agent::continuity::types`

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A scheduled autonomous mission job.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ScheduledJob {
    pub id: String,
    pub agent_id: String,
    pub workflow_id: Option<String>,
    pub name: String,
    pub prompt: String,
    pub cron_expr: String,
    pub budget_usd: f64,
    pub enabled: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: DateTime<Utc>,
    pub consecutive_failures: i64,
    pub max_failures: i64,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

impl ScheduledJob {
    /// Extracts the execution AcceptMode ("observe" vs "auto") from job metadata.
    pub fn accept_mode(&self) -> crate::agent::types::AcceptMode {
        self.metadata
            .as_ref()
            .and_then(|m| m.get("accept_mode").or_else(|| m.get("acceptMode")))
            .and_then(|v| v.as_str())
            .map(|s| {
                if s.eq_ignore_ascii_case("observe") {
                    crate::agent::types::AcceptMode::Observe
                } else {
                    crate::agent::types::AcceptMode::Auto
                }
            })
            .unwrap_or(crate::agent::types::AcceptMode::Auto)
    }
}

/// A single execution record for a `ScheduledJob`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ScheduledJobRun {
    pub id: String,
    pub job_id: String,
    pub mission_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: JobRunStatus,
    pub cost_usd: f64,
    pub output_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum JobRunStatus {
    Running,
    Completed,
    Failed,
    BudgetExceeded,
    Skipped,
    Dropped,
}

impl JobRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobRunStatus::Running => "running",
            JobRunStatus::Completed => "completed",
            JobRunStatus::Failed => "failed",
            JobRunStatus::BudgetExceeded => "budget_exceeded",
            JobRunStatus::Skipped => "skipped",
            JobRunStatus::Dropped => "dropped",
        }
    }
}

// ─── Request / Response types for the REST API ───────────────────

#[derive(Debug, Deserialize)]
pub struct CreateJobRequest {
    pub agent_id: String,
    pub workflow_id: Option<String>,
    pub name: String,
    pub prompt: String,
    /// Standard 5-field cron expression, e.g. "0 9 * * *" (9 AM daily UTC)
    pub cron_expr: String,
    /// Max USD spend allowed per single run. Default: 0.10
    pub budget_usd: Option<f64>,
    /// Auto-disable after this many consecutive failures. Default: 3.
    pub max_failures: Option<i64>,
    pub metadata: Option<serde_json::Value>,
    #[serde(default, alias = "acceptMode")]
    pub accept_mode: Option<crate::agent::types::AcceptMode>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateJobRequest {
    pub name: Option<String>,
    pub prompt: Option<String>,
    pub workflow_id: Option<String>,
    pub cron_expr: Option<String>,
    pub budget_usd: Option<f64>,
    pub enabled: Option<bool>,
    pub max_failures: Option<i64>,
    #[serde(default, alias = "acceptMode")]
    pub accept_mode: Option<crate::agent::types::AcceptMode>,
}





// Metadata: [types]
