/**
 * @docs ARCHITECTURE:Services
 * ### AI Assist Note
 * Session-tree and artifact requests use the configured authenticated API client.
 * ### 🔍 Debugging & Observability
 * - Failure Path: HTTP failure or malformed session response.
 * - Telemetry Link: Search sovereign requests in the shared HTTP trace store.
 */
import { api_request } from './base_api_service';

export interface Session_Node {
    id: string;
    role: string;
    content: string;
    created_at: string;
    mission_id?: string;
}

function successful_response(value: unknown): Record<string, unknown> {
    if (!value || typeof value !== 'object' || (value as Record<string, unknown>).status !== 'success') {
        throw new Error('The engine returned an invalid session response.');
    }
    return value as Record<string, unknown>;
}

function session_nodes(value: unknown): Session_Node[] {
    if (!Array.isArray(value)) throw new Error('The engine returned invalid session history.');
    const ids = new Set<string>();
    return value.map((entry: unknown) => {
        if (!entry || typeof entry !== 'object') throw new Error('The engine returned an invalid session entry.');
        const node = entry as Record<string, unknown>;
        if (typeof node.id !== 'string' || !node.id || ids.has(node.id) ||
            typeof node.role !== 'string' || typeof node.content !== 'string') {
            throw new Error('The engine returned an invalid session entry.');
        }
        ids.add(node.id);
        return {
            id: node.id, role: node.role, content: node.content,
            created_at: typeof node.created_at === 'string' ? node.created_at : '',
            mission_id: typeof node.mission_id === 'string' ? node.mission_id : undefined,
        };
    });
}

const mission_path = (mission_id: string) => `/v1/sovereign/missions/${encodeURIComponent(mission_id)}`;

export const sovereign_api_service = {
    async history(mission_id: string, leaf_id: string): Promise<Session_Node[]> {
        const data = successful_response(await api_request(`${mission_path(mission_id)}/nodes/${encodeURIComponent(leaf_id)}/history`));
        return session_nodes(data.history);
    },
    async leaves(mission_id: string): Promise<Session_Node[]> {
        const data = successful_response(await api_request(`${mission_path(mission_id)}/leaves`));
        return session_nodes(data.leaves);
    },
    async revert(mission_id: string, node_id: string): Promise<void> {
        successful_response(await api_request(`${mission_path(mission_id)}/nodes/${encodeURIComponent(node_id)}/revert`, { method: 'POST' }));
    },
    async promote(payload: { name: string; description: string; cap_type: 'skill'; content: string; agent_id: string; mission_id?: string }): Promise<string> {
        const data = await api_request<{ id: string }>('/v1/skills/promote', { method: 'POST', body: JSON.stringify(payload) });
        if (!data || typeof data.id !== 'string' || !data.id) throw new Error('The engine returned an invalid promotion response.');
        return data.id;
    },
};

// Metadata: [sovereign_api_service]
