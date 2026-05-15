-- @docs ARCHITECTURE:Security
-- 
-- ### AI Assist Note
-- **Agent-Aware Permission Policies**: Implements granular control over tool execution.
-- Supports identity-based overrides, enabling a "Delegate-First" security model
-- where alpha agents are restricted from sensitive system tools.

-- 1. Ensure a baseline table exists so the rename operation is safe
CREATE TABLE IF NOT EXISTS permission_policies (
    tool_name TEXT PRIMARY KEY,
    mode TEXT NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 2. Rename existing table to a temporary name for migration
ALTER TABLE permission_policies RENAME TO permission_policies_old;

-- 3. Create the new authoritative schema with agent_id and id columns
CREATE TABLE permission_policies (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tool_name TEXT NOT NULL,
    agent_id TEXT, -- NULL means global policy
    mode TEXT NOT NULL CHECK(mode IN ('allow', 'deny', 'prompt')),
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(tool_name, agent_id)
);

-- 4. Migrate data from the old table to the new schema
-- This safely handles the transition from the legacy 20260412 schema.
-- We lowercase the mode to ensure compliance with the new CHECK constraint.
INSERT INTO permission_policies (tool_name, mode, updated_at)
SELECT tool_name, LOWER(mode), updated_at FROM permission_policies_old;

-- 5. Remove the temporary legacy table
DROP TABLE permission_policies_old;

-- 6. Seed baseline restrictions for the Alpha agent (Agent 2 / Tadpole)
-- Forces delegation of system status checks to maintenance specialists (Agents 7/13).
INSERT OR IGNORE INTO permission_policies (tool_name, agent_id, mode) VALUES ('system:check_system_status', '2', 'deny');
INSERT OR IGNORE INTO permission_policies (tool_name, agent_id, mode) VALUES ('get_system_status', '2', 'deny');
INSERT OR IGNORE INTO permission_policies (tool_name, agent_id, mode) VALUES ('check_system_status', '2', 'deny');

-- Allow maintenance specialists (Agent 7 and 13) to check system status
INSERT OR IGNORE INTO permission_policies (tool_name, agent_id, mode) VALUES ('system:check_system_status', '7', 'allow');
INSERT OR IGNORE INTO permission_policies (tool_name, agent_id, mode) VALUES ('system:check_system_status', '13', 'allow');
