-- OKF v0.3 Extensions: Relational Knowledge Edges & Environmental Constraints
--
-- @docs ARCHITECTURE:IKS
-- Metadata: [OKF_V03_EXTENSIONS]

CREATE TABLE IF NOT EXISTS knowledge_edges (
    id            TEXT PRIMARY KEY,
    source_id     TEXT NOT NULL,
    target_id     TEXT NOT NULL,
    relation_type TEXT NOT NULL DEFAULT 'relates_to', -- 'depends_on' | 'implements' | 'refutes' | 'replaces' | 'derives_from'
    weight        REAL NOT NULL DEFAULT 1.0,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    FOREIGN KEY(source_id) REFERENCES knowledge_store_meta(id) ON DELETE CASCADE,
    FOREIGN KEY(target_id) REFERENCES knowledge_store_meta(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ke_source ON knowledge_edges(source_id);
CREATE INDEX IF NOT EXISTS idx_ke_target ON knowledge_edges(target_id);
CREATE INDEX IF NOT EXISTS idx_ke_relation ON knowledge_edges(relation_type);

-- Structured environmental constraints & provenance tracking
ALTER TABLE knowledge_store_meta ADD COLUMN constraints_json TEXT;
ALTER TABLE knowledge_store_meta ADD COLUMN provenance_chain TEXT;
