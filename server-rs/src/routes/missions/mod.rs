//! @docs ARCHITECTURE:Storage
//!
//! ### AI Assist Note
//! **Missions Facade**: Sovereign flight recorder and debrief analysis subsystem.
//! Orchestrates mission catalog querying, Merkle hash-chain verification,
//! Content-Addressed Storage diffing, and automated debrief synthesis.
//! - **System Identity**: TadpoleOS/1.1.59
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Merkle chain tamper mismatch, database timeout, or unverified logs.
//! - **Telemetry Link**: Search `[missions]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::missions`

pub mod cas_diff;
pub mod catalog;
pub mod dossier;
pub mod merkle;
pub mod models;
pub mod pin;
#[cfg(test)]
pub mod tests;

// Re-export models & resource bounds
pub use models::{
    CatalogQuery, CatalogResponse, CasDiffRecord, CasDiffSummary, DebriefPayload,
    EconomicsPayload, MerkleStepRecord, MissionDossier, MissionSummaryItem, PinMissionRequest,
    SwarmFindingRecord, TransactionLedgerRecord, TrustGraphEdge, TrustGraphNode,
    TrustGraphPayload, MAX_DOSSIER_DIFF_BYTES, MAX_FILE_PREVIEW_BYTES, MAX_LOG_ROWS,
    MAX_PARTICIPANT_ADDRESSES, MAX_REVISION_ROWS, VALID_MISSION_STATUSES,
};

// Re-export catalog engine
pub use catalog::{
    decode_mission_summary, escape_like_pattern, get_missions_catalog, MissionCatalogFilters,
};

// Re-export Merkle log chain verifier
pub use merkle::{
    fetch_and_verify_mission_logs, read_log_fields, ChainCursor, RawLogRow, VerifiedLogsResult,
};

// Re-export CAS diff engine
pub use cas_diff::{
    compute_line_diff_summary, fetch_and_diff_cas_revisions, read_live_workspace_file_preview,
    CasDiffsResult,
};

// Re-export dossier pipeline
pub use dossier::{
    build_economics_payload, fetch_memory_palace, fetch_mission_header, fetch_trust_graph,
    get_mission_dossier, synthesize_debrief_payload,
};

// Re-export pin handler
pub use pin::toggle_mission_pin;

// Metadata: [missions]

