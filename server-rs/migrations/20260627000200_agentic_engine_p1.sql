-- Migration: Agentic Engine P1 — Skill Subscriptions & Runner Policy
-- @docs ARCHITECTURE:AgenticEngine

-- Add runner_policy column to agents table.
-- Default policy enforces single concurrency, resuming blocked tasks first, and preflight validation.
ALTER TABLE agents ADD COLUMN runner_policy TEXT DEFAULT '{"max_concurrent":1,"resume_blocked_first":true,"preflight_checks":["context_version","skill_updates"]}';
