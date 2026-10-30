/**
 * @docs ARCHITECTURE:Types
 *
 * ### AI Assist Note
 * **mission_dossier**: Authoritative TypeScript interfaces for the Sovereign Observability Core (SOC)
 * Mission Flight Recorder. Aligns 1:1 with `server-rs::routes::missions` and `server-rs::routes::cas`.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Schema drift between `routes::missions` JSON payload and UI Dossier tabs.
 * - **Telemetry Link**: Search `[mission_dossier]` in console logs.
 */

export interface MissionSummaryItem {
    id: string;
    agent_id: string;
    title: string;
    status: string;
    budget_usd: number;
    cost_usd: number;
    is_degraded?: boolean | null;
    is_pinned: boolean;
    created_at: string;
    updated_at: string;
    step_count?: number;
    log_count: number;
    file_mutation_count?: number;
    diff_count: number;
    has_debrief?: boolean;
}

export interface CatalogResponse {
    success: boolean;
    data: MissionSummaryItem[];
    missions?: MissionSummaryItem[];
    total: number;
    page: number;
    per_page: number;
    limit?: number;
    offset?: number;
}

export interface MerkleAuditSummary {
    chain_verified: boolean;
    total_steps: number;
    tampered_at_step: number | null;
    genesis_hash: string | null;
    head_hash: string | null;
}

export interface MerkleStepRecord {
    id: string;
    mission_id: string;
    step_index?: number;
    agent_id: string;
    source: string;
    text: string;
    message?: string;
    severity: 'info' | 'success' | 'warning' | 'error' | string;
    timestamp: string;
    metadata: Record<string, unknown> | null;
    hash: string | null;
    prev_hash: string | null;
    is_verified: boolean;
    /** Structured step decomposition for Overlord Entity (0) */
    thought_summary?: string | null;
    tool_name?: string | null;
    tool_args?: unknown | null;
    duration_ms?: number | null;
}

export interface CasDiffSummary {
    additions: number;
    deletions: number;
    changes: number;
}

export interface CasDiffRecord {
    id: number;
    file_path: string;
    version_num: number;
    initial_version?: number;
    latest_version?: number;
    hash: string;
    initial_hash?: string;
    size_bytes: number;
    created_at: string;
    pre_mutation_content: string;
    before_content?: string;
    current_content: string;
    after_content?: string;
    lines_added?: number;
    lines_removed?: number;
    diff_summary: CasDiffSummary;
}

export interface CasRevisionSummary {
    id: number;
    workspace_root: string;
    file_path: string;
    version_num: number;
    hash: string;
    size_bytes: number;
    mission_id: string | null;
    agent_id: string | null;
    created_at: string;
}

export interface SwarmFindingRecord {
    id: string;
    mission_id: string;
    agent_id: string;
    topic: string;
    finding: string;
    timestamp: string;
}

export interface TrustGraphNode {
    id: string;
    name: string;
    node_type: string;
    description: string | null;
    mission_id: string | null;
    created_at: number;
}

export interface TrustGraphEdge {
    id: string;
    source_entity_id: string;
    target_entity_id: string;
    relation_type: string;
    weight: number;
    mission_id: string | null;
    created_at: number;
}

export interface TrustGraphPayload {
    entities: TrustGraphNode[];
    relations: TrustGraphEdge[];
}

export interface TransactionLedgerRecord {
    id: string;
    tx_id?: string;
    transaction_type: string;
    buyer_address: string | null;
    debit_agent_id?: string;
    seller_address: string | null;
    credit_agent_id?: string;
    asset_id: string | null;
    amount: number;
    amount_micros?: number;
    status: string;
    audit_signature: string | null;
    lock_id?: string | null;
    created_at: number;
}

export interface EconomicsPayload {
    total_cost_usd: number;
    budget_usd: number;
    utilization_pct?: number;
    total_tokens?: number;
    total_tokens_source?: string;
    transactions: TransactionLedgerRecord[];
    ledger_entries?: TransactionLedgerRecord[];
}

export interface DebriefPayload {
    agent99_notes: string | null;
    drift_score: number | null;
    verdict: string | null;
    recommendations: string[];
}

export interface MissionDossier {
    mission: MissionSummaryItem;
    merkle_audit?: MerkleAuditSummary;
    logs: MerkleStepRecord[];
    steps?: MerkleStepRecord[];
    diffs: CasDiffRecord[];
    file_diffs?: CasDiffRecord[];
    memory_palace: SwarmFindingRecord[];
    findings?: SwarmFindingRecord[];
    trust_graph: TrustGraphPayload;
    economics: EconomicsPayload;
    debrief: DebriefPayload;
    logs_total?: number;
    logs_truncated?: boolean;
    diffs_total?: number;
    diffs_truncated?: boolean;
    partial?: boolean;
    warnings?: string[];
}

export type DossierTabKey =
    | 'merkle'
    | 'cas'
    | 'memory'
    | 'trustgraph'
    | 'economics'
    | 'debrief';

// Metadata: [mission_dossier]

