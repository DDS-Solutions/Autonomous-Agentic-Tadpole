-- Migration: Add lock expiry to a2a_ledger
-- @docs ARCHITECTURE:Persistence

ALTER TABLE a2a_ledger ADD COLUMN expires_at DATETIME;
CREATE INDEX IF NOT EXISTS idx_a2a_ledger_expires ON a2a_ledger(expires_at);
