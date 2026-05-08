/**
 * @docs ARCHITECTURE:Contracts
 * 
 * ### AI Assist Note
 * **Governance Layer**: DTOs for system-wide resource boundaries and defense status.
 * Reconciled with backend Quotas DTO for 100% build parity.
 */

export interface Quota_Details {
    entity_id: string;
    budget_usd: number;
    used_usd: number;
    reset_period: 'daily' | 'monthly' | 'never';
    last_reset_at: string;
    next_reset_at: string;
}

export interface GovernanceQuotas {
    total_budget: number;
    total_spent: number;
    efficiency: number;
    remaining: number;
    agent_quotas: Quota_Details[];
    system_defense: {
        memory_pressure?: number;
        cpu_load?: number;
        merkle_integrity: number;
        aletheia_status?: 'verified' | 'bypassed';
        drift_status?: 'stable' | 'drifting';
        sandbox_status?: 'ACTIVE' | 'OFFLINE';
        sandbox_type?: string;
    };
    last_sync: string;
}

export const GOVERNANCE_VERSION = '1.2.0';

// Metadata: [governance]
