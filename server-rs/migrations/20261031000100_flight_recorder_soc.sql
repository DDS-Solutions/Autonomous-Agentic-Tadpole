-- Migration: 20261031000100_flight_recorder_soc.sql
-- Sovereign Observability Core (SOC): Merkle Audit Trail, Lean File Revisions, and Compound Telemetry Indexes
-- @docs ARCHITECTURE:Persistence

-- 1. Add SHA-256 Merkle chain hash columns to mission_logs
ALTER TABLE mission_logs ADD COLUMN hash TEXT;
ALTER TABLE mission_logs ADD COLUMN prev_hash TEXT;

-- 2. Lean Workspace File Revision Ledger table (Read-Only Diff Observability)
CREATE TABLE IF NOT EXISTS file_revisions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,          -- Workspace root path or identifier
    file_path TEXT NOT NULL,             -- Relative file path within workspace
    hash TEXT NOT NULL,                  -- SHA-256 hash of pre-mutation content
    content BLOB NOT NULL,               -- Pre-mutation UTF-8 snapshot (capped at 1 MB)
    size_bytes INTEGER NOT NULL,         -- Uncompressed byte size
    version_num INTEGER NOT NULL,        -- Monotonically increasing revision index (0, 1, 2...)
    mission_id TEXT,                     -- Associated agent mission ID
    agent_id TEXT,                       -- Agent that triggered the modification
    created_at TEXT NOT NULL
);

-- 3. Compound Performance Indexes for Sub-Millisecond Dossier & Catalog Queries
CREATE UNIQUE INDEX IF NOT EXISTS idx_file_revisions_unique 
ON file_revisions (workspace_id, file_path, version_num);

CREATE INDEX IF NOT EXISTS idx_file_revisions_lookup 
ON file_revisions (workspace_id, file_path, version_num DESC);

CREATE INDEX IF NOT EXISTS idx_file_revisions_mission 
ON file_revisions (mission_id, file_path ASC, version_num ASC);

CREATE INDEX IF NOT EXISTS idx_mission_history_pinned_created 
ON mission_history (is_pinned DESC, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_mission_logs_chain_lookup 
ON mission_logs (mission_id, timestamp ASC, id ASC);

CREATE INDEX IF NOT EXISTS idx_mission_logs_count 
ON mission_logs (mission_id);

