-- Migration: multiversal_session_tree
-- Supports Phase 2 of the Sovereign Entity Evolution

-- Table: mission_nodes
-- Represents a single node in a branched session tree
CREATE TABLE IF NOT EXISTS mission_nodes (
    id TEXT PRIMARY KEY,
    mission_id TEXT NOT NULL,
    parent_id TEXT, -- Nullable: root nodes have no parent
    role TEXT NOT NULL, -- 'user', 'assistant', 'system', 'tool'
    content TEXT NOT NULL,
    metadata TEXT, -- JSON blob for tool IDs, severity, or custom state
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(mission_id) REFERENCES mission_history(id),
    FOREIGN KEY(parent_id) REFERENCES mission_nodes(id)
);

-- Index for fast tree traversal
CREATE INDEX IF NOT EXISTS idx_mission_nodes_parent ON mission_nodes(parent_id);
CREATE INDEX IF NOT EXISTS idx_mission_nodes_mission ON mission_nodes(mission_id);

-- Update mission_history to track the current active leaf (multiversal tip)
-- This allows the system to instantly switch between branches
ALTER TABLE mission_history ADD COLUMN active_node_id TEXT;
