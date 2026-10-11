-- Migration: 20261011000100_guardrail_permission_policies.sql
-- Enforces baseline human-in-the-loop prompt guardrails for high-impact tools
-- (write_file, fetch_url, spawn_subagent) under the agent-aware permission schema.

INSERT INTO permission_policies (tool_name, agent_id, mode)
SELECT 'write_file', NULL, 'prompt'
WHERE NOT EXISTS (SELECT 1 FROM permission_policies WHERE tool_name = 'write_file' AND agent_id IS NULL);

INSERT INTO permission_policies (tool_name, agent_id, mode)
SELECT 'fetch_url', NULL, 'prompt'
WHERE NOT EXISTS (SELECT 1 FROM permission_policies WHERE tool_name = 'fetch_url' AND agent_id IS NULL);

INSERT INTO permission_policies (tool_name, agent_id, mode)
SELECT 'spawn_subagent', NULL, 'prompt'
WHERE NOT EXISTS (SELECT 1 FROM permission_policies WHERE tool_name = 'spawn_subagent' AND agent_id IS NULL);
