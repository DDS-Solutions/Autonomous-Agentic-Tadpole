-- Migration: Agent Economics Metadata & Daily Budget Caps
-- @docs ARCHITECTURE:Persistence

CREATE TABLE IF NOT EXISTS agent_economics_meta (
    agent_id TEXT PRIMARY KEY,
    economic_zone TEXT NOT NULL DEFAULT 'DEV',
    daily_spend_limit_micros INTEGER NOT NULL DEFAULT 10000000, -- Default $10.00 cap (10,000,000 micros)
    daily_spent_accumulated_micros INTEGER NOT NULL DEFAULT 0,
    last_reset_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    FOREIGN KEY(agent_id) REFERENCES agents(id)
);

CREATE INDEX IF NOT EXISTS idx_agent_econ_zone ON agent_economics_meta(economic_zone);

-- Auto-enroll baseline agents into economic governance
INSERT OR IGNORE INTO agent_economics_meta (agent_id, daily_spend_limit_micros)
SELECT id, 10000000 FROM agents;
