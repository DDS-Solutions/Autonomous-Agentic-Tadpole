//! @docs ARCHITECTURE:Storage
//!
//! ### AI Assist Note
//! **Missions Data Models**: Defines request payloads, response envelopes,
//! Merkle flight log steps, CAS diff records, economics, and dossier contracts.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Serialization errors, negative bounds, or missing fields in dossier contracts.
//! - **Telemetry Link**: Search `[missions_models]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::missions::models`

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────
//  RESOURCE & PAYLOAD CEILING CONSTANTS
// ─────────────────────────────────────────────────────────

/// Maximum number of Merkle flight log steps returned in a single dossier payload.
pub const MAX_LOG_ROWS: i64 = 2000;
/// Maximum number of distinct file revision entries returned in a single dossier payload.
pub const MAX_REVISION_ROWS: i64 = 500;
/// Maximum byte size (256 KB) for an individual pre-mutation CAS blob or live disk preview.
pub const MAX_FILE_PREVIEW_BYTES: usize = 256 * 1024;
/// Maximum aggregate byte budget (4 MB) for all file diff previews combined in a single dossier.
pub const MAX_DOSSIER_DIFF_BYTES: usize = 4 * 1024 * 1024;
/// Maximum number of distinct participant addresses bound in the A2A ledger query.
pub const MAX_PARTICIPANT_ADDRESSES: usize = 50;

/// Known canonical mission statuses allowed in catalog filtering.
pub const VALID_MISSION_STATUSES: &[&str] = &[
    "pending",
    "active",
    "running",
    "completed",
    "failed",
    "paused",
    "cancelled",
    "aborted",
];

// ─────────────────────────────────────────────────────────
//  DATA CONTRACTS
// ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CatalogQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub status: Option<String>,
    pub search: Option<String>,
    pub is_pinned: Option<bool>,
    pub pinned_only: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
pub struct PinMissionRequest {
    pub is_pinned: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MissionSummaryItem {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub status: String,
    pub budget_usd: f64,
    pub cost_usd: f64,
    pub is_pinned: bool,
    pub created_at: String,
    pub updated_at: String,
    pub log_count: i64,
    pub diff_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MerkleStepRecord {
    pub id: String,
    pub mission_id: String,
    pub agent_id: String,
    pub source: String,
    pub text: String,
    pub severity: String,
    pub timestamp: String,
    pub metadata: Option<serde_json::Value>,
    pub hash: Option<String>,
    pub prev_hash: Option<String>,
    pub is_verified: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CasDiffSummary {
    pub additions: usize,
    pub deletions: usize,
    pub changes: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CasDiffRecord {
    pub id: i64,
    pub file_path: String,
    pub version_num: i64,
    pub hash: String,
    pub size_bytes: i64,
    pub created_at: String,
    pub pre_mutation_content: String,
    pub current_content: String,
    pub diff_summary: CasDiffSummary,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SwarmFindingRecord {
    pub id: String,
    pub mission_id: String,
    pub agent_id: String,
    pub topic: String,
    pub finding: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrustGraphNode {
    pub id: String,
    pub name: String,
    pub node_type: String,
    pub description: Option<String>,
    pub mission_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrustGraphEdge {
    pub id: String,
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub relation_type: String,
    pub weight: f64,
    pub mission_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrustGraphPayload {
    pub entities: Vec<TrustGraphNode>,
    pub relations: Vec<TrustGraphEdge>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransactionLedgerRecord {
    pub id: String,
    pub transaction_type: String,
    pub buyer_address: Option<String>,
    pub seller_address: Option<String>,
    pub asset_id: Option<String>,
    pub amount: i64,
    pub status: String,
    pub audit_signature: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EconomicsPayload {
    pub total_cost_usd: f64,
    pub budget_usd: f64,
    #[serde(default)]
    pub total_tokens: u64,
    #[serde(default)]
    pub total_tokens_source: String,
    pub transactions: Vec<TransactionLedgerRecord>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DebriefPayload {
    pub agent99_notes: Option<String>,
    pub drift_score: Option<f64>,
    pub verdict: Option<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MissionDossier {
    pub mission: MissionSummaryItem,
    pub logs: Vec<MerkleStepRecord>,
    pub diffs: Vec<CasDiffRecord>,
    pub memory_palace: Vec<SwarmFindingRecord>,
    pub trust_graph: TrustGraphPayload,
    pub economics: EconomicsPayload,
    pub debrief: DebriefPayload,
    #[serde(default)]
    pub logs_total: u64,
    #[serde(default)]
    pub logs_truncated: bool,
    #[serde(default)]
    pub diffs_total: u64,
    #[serde(default)]
    pub diffs_truncated: bool,
    #[serde(default)]
    pub partial: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogResponse {
    pub success: bool,
    pub data: Vec<MissionSummaryItem>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

// Metadata: [missions_models]

