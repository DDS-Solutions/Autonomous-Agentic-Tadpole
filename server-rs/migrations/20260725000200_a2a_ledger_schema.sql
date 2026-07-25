-- Migration: A2A Double-Entry Ledger (A2E-01 Protocol)
-- @docs ARCHITECTURE:Persistence

CREATE TABLE IF NOT EXISTS a2a_ledger (
    tx_id TEXT PRIMARY KEY,
    debit_agent_id TEXT NOT NULL,
    credit_agent_id TEXT NOT NULL,
    amount_micros INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'PREPARED', -- 'PREPARED', 'COMMITTED', 'ROLLED_BACK'
    lock_id TEXT UNIQUE NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(debit_agent_id) REFERENCES agents(id),
    FOREIGN KEY(credit_agent_id) REFERENCES agents(id)
);

CREATE INDEX IF NOT EXISTS idx_a2a_ledger_debit ON a2a_ledger(debit_agent_id);
CREATE INDEX IF NOT EXISTS idx_a2a_ledger_credit ON a2a_ledger(credit_agent_id);
CREATE INDEX IF NOT EXISTS idx_a2a_ledger_status ON a2a_ledger(status);
