-- Hot-path composite indexes identified during codebase audit (M36)
-- Optimizes:
-- 1. Agent task claiming & working queries (claimed_by, status)
-- 2. Mission history reaping queries (updated_at, is_pinned)
-- 3. Oversight log querying by mission (mission_id)

CREATE INDEX IF NOT EXISTS idx_agent_tasks_claimed_status ON agent_tasks(claimed_by, status);
CREATE INDEX IF NOT EXISTS idx_mission_history_reap ON mission_history(updated_at, is_pinned);
CREATE INDEX IF NOT EXISTS idx_oversight_log_mission ON oversight_log(mission_id);
