-- @docs ARCHITECTURE:Core
-- Consolidate ad-hoc runtime DDL previously executed in server-rs/src/db/init.rs
-- into versioned SQLx migrations as the single source of truth for schema state.

CREATE TABLE IF NOT EXISTS fallback_memories (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    text TEXT NOT NULL,
    mission_id TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS agent_hires (
    id TEXT PRIMARY KEY,
    hiring_agent_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    budget REAL NOT NULL,
    task_description TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (hiring_agent_id) REFERENCES agents(id) ON DELETE CASCADE,
    FOREIGN KEY (target_agent_id) REFERENCES agents(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS event_triggers (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    event_filter TEXT,
    continuity_job_id TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS mission_checkpoints (
    id TEXT PRIMARY KEY,
    mission_id TEXT NOT NULL,
    turn_index INTEGER NOT NULL,
    state_blob TEXT NOT NULL,
    active_node_id TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (mission_id) REFERENCES mission_history(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_mission_checkpoints_mission
    ON mission_checkpoints(mission_id, turn_index);

CREATE TABLE IF NOT EXISTS tool_results (
    call_id TEXT PRIMARY KEY,
    mission_id TEXT NOT NULL,
    turn_index INTEGER NOT NULL,
    tool_name TEXT NOT NULL,
    args_hash TEXT NOT NULL,
    result_text TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (mission_id) REFERENCES mission_history(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_tool_results_lookup
    ON tool_results(mission_id, tool_name, args_hash);
