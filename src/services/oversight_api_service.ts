/**
 * @docs ARCHITECTURE:Services
 * 
 * ### AI Assist Note
 * **Oversight Domain API Service**: Frontend API client for calling oversight,
 * audit trail, settings configuration, and health metrics endpoints.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Ledger truncation or empty audit trail due to network limits.
 * - **Telemetry Link**: Search `[oversight_api_service]` in console logs.
 */

import { api_request } from './base_api_service';
import type { Audit_Entry, Agent_Health, Quotas, Quota_Details } from './system_api_service';

export interface OversightSettings {
    auto_approve_safe_skills: boolean;
    privacy_mode?: boolean;
    max_agents?: number;
    max_clusters?: number;
    max_swarm_depth?: number;
    max_task_length?: number;
    default_budget_usd?: number;
}

export const oversight_api_service = {
    /**
     * get_pending_oversight
     * Fetches actions awaiting human approval.
     */
    get_pending_oversight: async (): Promise<unknown[]> => {
        try {
            const res = await api_request<unknown | unknown[]>('/v1/oversight/pending', { method: 'GET' });
            return Array.isArray(res) ? res : ((res as { data?: unknown[] }).data || []);
        } catch (error) {
            console.error('[oversight_api_service] Failed to fetch pending oversight actions:', error);
            return [];
        }
    },

    /**
     * get_oversight_ledger
     * Fetches the historical ledger of all oversight decisions.
     * Requests up to 10000 items to ensure accurate statistics and avoid card truncation.
     */
    get_oversight_ledger: async (): Promise<unknown[]> => {
        try {
            const res = await api_request<unknown | unknown[]>('/v1/oversight/ledger?per_page=10000', { method: 'GET' });
            return Array.isArray(res) ? res : ((res as { data?: unknown[] }).data || []);
        } catch (error) {
            console.error('[oversight_api_service] Failed to fetch oversight ledger:', error);
            return [];
        }
    },

    /**
     * decide_oversight
     * Records a decision (approve/reject) for a pending oversight action.
     */
    decide_oversight: async (id: string, decision: 'approved' | 'rejected'): Promise<void> => {
        await api_request(`/v1/oversight/${id}/decide`, {
            method: 'POST',
            body: JSON.stringify({ decision })
        });
    },

    /**
     * get_security_quotas
     * Returns aggregate security quotas (budget vs spent).
     */
    get_security_quotas: async (): Promise<Quotas> => {
        return api_request<Quotas>('/v1/oversight/security/quotas', { method: 'GET' });
    },

    /**
     * update_security_quota
     * Updates a specific security quota for an entity.
     */
    update_security_quota: async (entity_id: string, budget_usd: number): Promise<{ status: string }> => {
        return api_request<{ status: string }>(`/v1/oversight/security/quotas/${entity_id}`, {
            method: 'PUT',
            body: JSON.stringify({ budget_usd })
        });
    },

    /**
     * get_mission_quotas
     * Returns all registered mission quotas.
     */
    get_mission_quotas: async (): Promise<{ quotas: Quota_Details[] }> => {
        return api_request<{ quotas: Quota_Details[] }>('/v1/oversight/security/missions/quotas', { method: 'GET' });
    },

    /**
     * update_mission_quota
     * Updates the budget quota for a specific mission cluster.
     */
    update_mission_quota: async (cluster_id: string, budget_usd: number): Promise<{ status: string }> => {
        return api_request<{ status: string }>(`/v1/oversight/security/missions/${cluster_id}/quota`, {
            method: 'PUT',
            body: JSON.stringify({ budget_usd })
        });
    },

    /**
     * get_audit_trail
     * Returns the full historical audit trail.
     */
    get_audit_trail: async (page = 1, per_page = 50): Promise<{ data: Audit_Entry[]; total: number }> => {
        try {
            return await api_request<{ data: Audit_Entry[]; total: number }>(`/v1/oversight/security/audit-trail?page=${page}&per_page=${per_page}`, { method: 'GET' });
        } catch (error) {
            console.error('[oversight_api_service] Failed to fetch audit trail:', error);
            throw error;
        }
    },

    /**
     * get_agent_health
     * Returns health metrics for all agents.
     */
    get_agent_health: async (): Promise<{ agents: Agent_Health[] }> => {
        try {
            return await api_request<{ agents: Agent_Health[] }>('/v1/oversight/security/health', { method: 'GET' });
        } catch (error) {
            console.error('[oversight_api_service] Failed to fetch agent health metrics:', error);
            throw error;
        }
    },

    /**
     * get_integrity_status
     * Checks the Merkle chain integrity status.
     */
    get_integrity_status: async (): Promise<{ integrity_score: number, status: string, verified_count: number, total_count: number }> => {
        try {
            return await api_request<{ integrity_score: number, status: string, verified_count: number, total_count: number }>('/v1/oversight/security/integrity', { method: 'GET' });
        } catch (error) {
            console.error('[oversight_api_service] Failed to fetch integrity status:', error);
            throw error;
        }
    },

    /**
     * update_governance_settings
     * Updates global governance and oversight settings.
     * Hardened payload parameter to type-safe OversightSettings to prevent privilege escalation.
     */
    update_governance_settings: async (settings: OversightSettings): Promise<unknown> => {
        return api_request('/v1/oversight/settings', {
            method: 'PUT',
            body: JSON.stringify(settings)
        });
    }
};
