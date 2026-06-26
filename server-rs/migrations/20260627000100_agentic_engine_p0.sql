-- Migration: Agentic Engine P0 — Status Ledger, Receipt System, Task Claim Lock
-- @docs ARCHITECTURE:AgenticEngine
--
-- Adds three P0 improvements inspired by the Agentic Engine coordination framework:
--
-- 1. TASK CLAIM LOCK: claimed_by / claimed_at prevent double-claim by concurrent agents
-- 2. RECEIPT SYSTEM: standardized state tokens (CLAIMED, DONE, BLOCKED, HUMAN_HOLD, etc.)
-- 3. STATUS LEDGER: per-agent living operational status document
--
-- These tables compose with the existing agent_tasks, agents, and audit_trail tables.

-- ─── 1. Task Claim Lock ────────────────────────────────────────────────────────
-- Add claim lock and receipt columns to agent_tasks.

CREATE TABLE IF NOT EXISTS agent_tasks (
    id                 TEXT PRIMARY KEY NOT NULL,
    agent_id           TEXT NOT NULL,
    title              TEXT NOT NULL,
    description        TEXT,
    status             TEXT NOT NULL DEFAULT 'todo',
    priority           TEXT NOT NULL DEFAULT 'normal',
    metadata           TEXT DEFAULT '{}',
    claimed_by         TEXT REFERENCES agents(id),
    claimed_at         INTEGER,
    current_receipt    TEXT CHECK(current_receipt IN (
                            'claimed', 'done', 'blocked', 'human_hold',
                            'unblocked', 'resumed', 'failed', 'review',
                            'skill_subscribed', 'skill_updated', 'follow_up'
                       )),
    receipt_history    TEXT DEFAULT '[]', -- JSON array
    block_type         TEXT CHECK(block_type IN ('inline', 'human_hold')),
    blocking_question  TEXT,
    last_safe_step     TEXT,
    retry_count        INTEGER DEFAULT 0,
    allowed_tools      TEXT DEFAULT '[]',  -- JSON array
    prohibited_cats    TEXT DEFAULT '[]',  -- JSON array
    tokens_in          INTEGER DEFAULT 0,
    tokens_out         INTEGER DEFAULT 0,
    cost_usd           REAL DEFAULT 0.0,
    provider_id        TEXT,
    created_at         INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at         INTEGER NOT NULL DEFAULT (unixepoch()),
    FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE
);

-- Fast lookup by claim lock and status
CREATE INDEX IF NOT EXISTS idx_agent_tasks_claimed_by ON agent_tasks(claimed_by);
CREATE INDEX IF NOT EXISTS idx_agent_tasks_status     ON agent_tasks(agent_id, status);

-- ─── 2. Status Ledger ─────────────────────────────────────────────────────────
-- One row per agent. Updated in-place on every runner heartbeat.
-- Never creates duplicate rows — use INSERT OR REPLACE.

CREATE TABLE IF NOT EXISTS agent_status_ledger (
    agent_id            TEXT PRIMARY KEY NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    agent_code          TEXT NOT NULL,              -- stable human-readable code e.g. "tadpole-alpha"
    runtime             TEXT,                       -- Ollama | OpenAI | Anthropic | etc.
    automation_state    TEXT NOT NULL DEFAULT 'manual',  -- manual | installed | blocked | paused
    last_heartbeat      INTEGER,                    -- unixepoch()
    last_queue_result   TEXT NOT NULL DEFAULT 'none',
    -- Values: none | checking | observed <task-id> | claimed <task-id>
    --         completed <task-id> | blocked <task-id> | holding <task-id>
    --         resumed <task-id> | failed <task-id>
    last_task_id        TEXT,
    last_successful_run INTEGER,
    context_version     INTEGER DEFAULT 1,
    context_packet      TEXT DEFAULT '{}',          -- JSON: engine_version, allowed_sources, boundaries
    subscribed_skills   TEXT DEFAULT '[]',          -- JSON: [{skill_id, version, approved_at}]
    notes               TEXT                        -- short human-readable note or blocker desc
);

CREATE INDEX IF NOT EXISTS idx_status_ledger_heartbeat ON agent_status_ledger(last_heartbeat);

-- ─── 3. Skill Subscriptions ───────────────────────────────────────────────────
-- Explicit per-agent skill approval and subscription tracking.
-- scope_hash = SHA-256(permissions + tools + external_actions) from skill manifest.
-- When scope_hash changes on skill update → subscription_status = 'pending_reapproval'

CREATE TABLE IF NOT EXISTS skill_subscriptions (
    id                  TEXT PRIMARY KEY NOT NULL,
    agent_id            TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    skill_id            TEXT NOT NULL,
    approved_at         INTEGER,
    scope_hash          TEXT NOT NULL,              -- SHA-256 of approved scope
    subscription_status TEXT NOT NULL DEFAULT 'pending'
        CHECK(subscription_status IN ('pending', 'approved', 'pending_reapproval', 'declined')),
    installed_at        INTEGER,
    last_updated_at     INTEGER,
    notes               TEXT,
    UNIQUE(agent_id, skill_id)
);

CREATE INDEX IF NOT EXISTS idx_skill_subs_agent    ON skill_subscriptions(agent_id);
CREATE INDEX IF NOT EXISTS idx_skill_subs_status   ON skill_subscriptions(subscription_status);
