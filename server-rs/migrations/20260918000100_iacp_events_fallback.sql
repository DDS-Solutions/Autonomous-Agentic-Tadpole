-- Migration: 20260918000100_iacp_events_fallback.sql
-- Codifies ad-hoc tables (fallback_memories, agent_hires, event_triggers) into formal migration tracking.

CREATE TABLE IF NOT EXISTS fallback_memories (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    text TEXT NOT NULL,
    mission_id TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS agent_hires (
    id TEXT PRIMARY KEY,
    hiring_agent_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    budget REAL NOT NULL,
    task_description TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS event_triggers (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    event_filter TEXT,
    continuity_job_id TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
