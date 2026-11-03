-- Migration: governance_settings
-- Durable singleton configuration for cluster-wide governance policies

CREATE TABLE IF NOT EXISTS governance_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    auto_approve_safe_skills INTEGER NOT NULL DEFAULT 0,
    privacy_mode INTEGER NOT NULL DEFAULT 0,
    max_agents INTEGER NOT NULL DEFAULT 50,
    max_clusters INTEGER NOT NULL DEFAULT 10,
    max_swarm_depth INTEGER NOT NULL DEFAULT 5,
    max_task_length INTEGER NOT NULL DEFAULT 32768,
    default_budget_usd REAL NOT NULL DEFAULT 1.0,
    updated_by TEXT NOT NULL DEFAULT 'system',
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT OR IGNORE INTO governance_settings (
    id, auto_approve_safe_skills, privacy_mode, max_agents, max_clusters,
    max_swarm_depth, max_task_length, default_budget_usd, updated_by, updated_at
) VALUES (1, 0, 0, 50, 10, 5, 32768, 1.0, 'system', CURRENT_TIMESTAMP);

