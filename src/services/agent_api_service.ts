/**
 * @docs ARCHITECTURE:Services
 * @docs API_REFERENCE:Endpoints
 * 
 * ### AI Assist Note
 * **Agent Domain Service**: Dedicated interface for agent lifecycle management, task dispatching, and vector memory operations. 
 * Implements Maturity Level 3 HATEOAS envelopes for paginated agent lists and secure API key injection via `NeuralVault`.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: 429 Rate Limit (governed by `model_store` RPM/TPM), Vault lock-out (prevents task dispatch), or memory fragmentation during semantic search.
 * - **Telemetry Link**: Look for `X-Request-Id` in backend logs or search `[AgentAPI]` in backend tracing.
 * 
 * @aiContext
 * - **Dependencies**: `base_api_service`, `vault_store`, `model_store`, `provider_store`.
 * - **Side Effects**: Modifies global agent registry in Rust sidecar.
 * - **Mocking**: Mock `api_request` from `base_api_service` for unit tests.
 */


import type { 
    Agent, 
    AgentPatch, 
    AgentDto, 
    Task_Payload,
    Agent_Memory_Entry,
    Raw_Agent_Memory_Entry,
    AgentUpdateDto
} from '../contracts/agent';
import { api_request } from './base_api_service';
import type { Skill_Definition, Workflow_Definition, Hook_Definition } from '../stores/skill_store';
import { PROVIDERS } from '../constants';
import { use_provider_store } from '../stores/provider_store';
import { use_vault_store } from '../stores/vault_store';
import { use_model_store, type Model_Entry } from '../stores/model_store';
import { event_bus } from './event_bus';
import { track_operation } from '../utils/telemetry';
import { serialize_agent_update } from '../domain/agents/serializers';
import { serialize_role } from '../domain/roles/normalizer';
import type { Role } from '../contracts/role/domain';

import { normalize_agent_memory_entry } from '../domain/agents/normalizers';
import { system_api_service } from './system_api_service';

/**
 * ResourceGuard: Utility to check system pressure before execution.
 * Hardened: Now implements a 'Soft-Wait' pulse instead of a 'Hard-Fail' throw.
 * This prevents tasks from aborting during transient spikes (Nexus-Fix-01).
 */
async function check_resource_guard(agent_id?: string) {
    let attempts = 0;
    const MAX_WAIT_ATTEMPTS = 12; // 60 seconds total at 5s intervals
    
    while (attempts < MAX_WAIT_ATTEMPTS) {
        const status = await system_api_service.get_engine_status();
        if (!status || !status.features?.includes('resource-guard')) return;

        const quotas = await system_api_service.get_security_quotas();
        const pressure = quotas.system_defense.memory_pressure;

        if (pressure <= 0.8) {
            if (attempts > 0) {
                event_bus.emit_log({ 
                    source: 'System', 
                    text: `✅ Memory pressure stabilized (${(pressure * 100).toFixed(1)}%). Resuming task for ${agent_id?.toUpperCase() || 'AGENT'}.`, 
                    severity: 'success' 
                });
            }
            return;
        }

        // Under high pressure: Emit pulse and wait
        if (attempts === 0) {
            event_bus.emit_log({ 
                source: 'System', 
                text: `⚠️ High memory pressure (${(pressure * 100).toFixed(1)}%). Throttling task for ${agent_id?.toUpperCase() || 'AGENT'}...`, 
                severity: 'warning' 
            });
        }
        
        attempts++;
        await new Promise(resolve => setTimeout(resolve, 5000));
    }

    throw new Error('Locally Throttled: System memory pressure remained > 80% for 60s. Task aborted to protect engine stability.');
}

export const agent_api_service = {
    /**
     * Fetches agents from TadpoleOS. Supporting Maturity Level 3 HATEOAS envelopes.
     * Handles both direct arrays and object-wrapped 'data' envelopes to maintain
     * compatibility with paginated backend responses.
     * 
     * Returns Wire DTOs (AgentDto).
     */
    get_agents: async (options: RequestInit = {}): Promise<AgentDto[]> => {
        return track_operation('AgentAPI', 'Fetching agent registry...', async () => {
            // Maturity Level 3: Handle the 'data' field in the paginated envelope if present.
            interface Agent_List_Envelope {
                data?: AgentDto[];
            }
            
            const result = await api_request<Agent_List_Envelope | AgentDto[]>('/v1/agents?per_page=500', { 
                method: 'GET',
                ...options 
            });

            if (result && typeof result === 'object' && !Array.isArray(result) && 'data' in result) {
                return result.data ?? [];
            }

            return Array.isArray(result) ? result : [];
        });
    },

    /**
     * Updates an agent's configuration in the global Rust registry.
     */
    update_agent: async (agent_id: string, patch: AgentPatch): Promise<boolean> => {
        return track_operation('AgentAPI', `Updating configuration for agent: ${agent_id.toUpperCase()}`, async () => {
            const body: AgentUpdateDto = serialize_agent_update(patch);

            await api_request(`/v1/agents/${agent_id}`, {
                method: 'PUT',
                body: JSON.stringify(body)
            });
            return true;
        });
    },

    /**
     * Creates a new agent in the global Rust registry.
     */
    create_agent: async (agent: Agent): Promise<boolean> => {
        const body = {
            ...serialize_agent_update(agent),
            id: agent.id,
            description: agent.description || "New Agent Node",
            status: agent.status || "idle",
            created_at: agent.created_at || new Date().toISOString(),
        };

        await api_request('/v1/agents', {
            method: 'POST',
            body: JSON.stringify(body)
        });
        return true;
    },

    /**
     * Pauses a running agent.
     */
    pause_agent: async (agent_id: string): Promise<boolean> => {
        await api_request(`/v1/agents/${agent_id}/pause`, { method: 'POST' });
        return true;
    },

    /**
     * Resumes a paused agent.
     */
    resume_agent: async (agent_id: string): Promise<boolean> => {
        await api_request(`/v1/agents/${agent_id}/resume`, { method: 'POST' });
        return true;
    },

    /**
     * Dispatches a command task to a specific agent node.
     * Integrates with NeuralVault for secure API key injection.
     * 
     * SECURITY NOTE: If a local key is available in the vault, it is injected into the payload.
     * The Rust backend is responsible for redacting this key from systemic logs.
     */
    send_command: async (agent_id: string, message: string, model_id: string, provider: string, cluster_id?: string, department?: string, budget_usd?: number, external_id?: string, safe_mode?: boolean, analysis?: boolean, request_id?: string, parent_node_id?: string, enabled_skills?: string[]): Promise<string> => {
        const attempt_dispatch = async (current_model_id: string, current_provider: string, attempt_count: number, base_url_override?: string): Promise<string> => {
            return track_operation('AgentAPI', `Dispatching command to agent: ${agent_id.toUpperCase()} (Attempt ${attempt_count + 1})`, async () => {
                // 🛡️ Resource Guard: Check pressure before dispatch
                await check_resource_guard(agent_id);

                const vault_store = use_vault_store.getState();
                const model_store = use_model_store.getState();
                const body: Task_Payload = { message, cluster_id, department, provider: current_provider, model_id: current_model_id, budget_usd, external_id, safe_mode, analysis, parent_node_id, enabled_skills };

                const provider_api_key = await vault_store.get_api_key(current_provider);
                const is_actually_locked = vault_store.is_locked && !sessionStorage.getItem('tadpole-vault-master-key');
                const is_local = current_provider === PROVIDERS.OLLAMA || current_provider === PROVIDERS.LOCAL;

                if (provider_api_key) {
                    body.api_key = provider_api_key;
                    const inventory_model = model_store.models.find((m: Model_Entry) => m.name === current_model_id || m.modelId === current_model_id);
                    if (inventory_model) {
                        if (inventory_model.rpm) body.rpm = inventory_model.rpm;
                        if (inventory_model.tpm) body.tpm = inventory_model.tpm;
                    }
                } else if (!is_local) {
                    const reason = is_actually_locked ? 'Vault is Locked' : `No Key for ${current_provider.toUpperCase()}`;
                    event_bus.emit_log({
                        source: 'System',
                        text: `🔒 Neural Security: ${reason} for ${agent_id.toUpperCase()}.`,
                        severity: 'warning'
                    });
                }

                // 🌐 Multi-Instance Support: Use slot-specific baseUrl if available
                if (base_url_override) {
                    body.base_url = base_url_override;
                } else if (use_provider_store.getState().base_urls[current_provider]) {
                    body.base_url = use_provider_store.getState().base_urls[current_provider];
                }

                const final_request_id = request_id || (crypto.randomUUID ? crypto.randomUUID() : `task-${Date.now()}`);

                try {
                    await api_request(`/v1/agents/${agent_id}/tasks`, {
                        method: 'POST',
                        body: JSON.stringify(body),
                        headers: { 'X-Request-Id': final_request_id }
                    });
                    return final_request_id;
                } catch (err: any) {
                    // 🔄 Sovereign Failover: Detect Connection Failure
                    const is_conn_fail = err.message?.includes('error sending request') || 
                                       err.message?.includes('Failed to fetch') || 
                                       err.status === 503 || 
                                       err.status === 504;

                    if (is_conn_fail && attempt_count < 2) {
                        const agent_store = (await import('../stores/agent_store')).use_agent_store.getState();
                        const agent = agent_store.get_agent(agent_id);
                        
                        if (agent) {
                            const next_slot = attempt_count === 0 ? 2 : 3;
                            const next_config = next_slot === 2 ? agent.model_config2 : agent.model_config3;

                            if (next_config && next_config.modelId && next_config.provider) {
                                event_bus.emit_log({
                                    source: 'System',
                                    text: `🔄 Sovereign Failover: ${current_provider.toUpperCase()} unreachable. Switching ${agent.name} to Slot ${next_slot} (${next_config.provider.toUpperCase()} @ ${next_config.baseUrl || 'Default URL'})...`,
                                    severity: 'warning'
                                });
                                return attempt_dispatch(next_config.modelId, next_config.provider, attempt_count + 1, next_config.baseUrl);
                            }
                        }
                    }
                    throw err;
                }
            }, { agent_id, mission_id: cluster_id });
        };

        return attempt_dispatch(model_id, provider, 0);
    },

    /**
     * poll_task_status
     * Polls the backend audit trail to resolve the final status of a task.
     * This addresses the 'Asynchronous Blindness' identified in the Nexus audit.
     */
    poll_task_status: async (agent_id: string, request_id: string, timeout_ms = 60000): Promise<'success' | 'error' | 'pending'> => {
        const start_time = Date.now();
        while (Date.now() - start_time < timeout_ms) {
            try {
                const trail = await system_api_service.get_audit_trail(1, 10);
                const entry = trail.data.find(e => e.id === request_id || (e as any).request_id === request_id);
                
                if (entry) {
                    if (entry.status === 'success' || entry.status === 'completed') return 'success';
                    if (entry.status === 'failed' || entry.status === 'error') return 'error';
                }
                
                // Fallback: Check mission history if not in audit trail
                const history = await api_request<{ entries: any[] }>(`/v1/agents/${agent_id}/memories`, { method: 'GET' });
                const mission_completion = history.entries.find(e => e.metadata?.request_id === request_id && e.text?.toLowerCase().includes('mission completed'));
                if (mission_completion) return 'success';

            } catch (e) {
                console.warn('[AgentAPI] Status polling error:', e);
            }
            await new Promise(resolve => setTimeout(resolve, 3000)); // Poll every 3s
        }
        return 'pending';
    },

    /**
     * Fetches the long-term vector memory for a given agent.
     */
    get_agent_memory: async (agent_id: string): Promise<{ status: string; entries: Agent_Memory_Entry[] }> => {
        const result = await api_request<{ status: string; entries: Raw_Agent_Memory_Entry[] }>(`/v1/agents/${agent_id}/memories`, { method: 'GET' });
        return {
            ...result,
            entries: (result.entries ?? []).map(normalize_agent_memory_entry),
        };
    },

    /**
     * Deletes a specific long-term vector memory row for a given agent.
     */
    delete_agent_memory: async (agent_id: string, row_id: string): Promise<{ status: string }> => {
        return api_request<{ status: string }>(`/v1/agents/${agent_id}/memories/${row_id}`, { method: 'DELETE' });
    },

    /**
     * Persists a new text entry into the agent's long-term vector memory.
     */
    save_agent_memory: async (agent_id: string, text: string): Promise<{ status: string; id: string }> => {
        return api_request<{ status: string; id: string }>(`/v1/agents/${agent_id}/memories`, {
            method: 'POST',
            body: JSON.stringify({ text })
        });
    },

    save_role_blueprint: async (blueprint: Role): Promise<boolean> => {
        await api_request('/v1/governance/blueprints', {
            method: 'POST',
            body: JSON.stringify(serialize_role(blueprint))
        });
        return true;
    },


    /**
     * Resets an agent's failure count and status.
     */
    reset_agent: async (agent_id: string): Promise<{ status: string; message: string }> => {
        return api_request<{ status: string; message: string }>(`/v1/agents/${agent_id}/reset`, {
            method: 'POST'
        });
    },

    /**
     * Imports a capability from a file. Returns a structured preview.
     */
    import_capability: async (file: File): Promise<{ type: string; data: Skill_Definition | Workflow_Definition | Hook_Definition; preview: string }> => {
        const form_data = new FormData();
        form_data.append('file', file);
        return api_request('/v1/skills/import', {
            method: 'POST',
            body: form_data,
        });
    },

    /**
     * Finalizes registration of a parsed capability.
     */
    register_capability: async (type: string, data: Skill_Definition | Workflow_Definition | Hook_Definition, category: string): Promise<{ status: string; name: string }> => {
        return api_request('/v1/skills/register', {
            method: 'POST',
            body: JSON.stringify({ type, data, category })
        });
    },

    /**
     * Performs a global semantic search across agent memories and mission logs.
     */
    search_memory: async (query: string, agent_id?: string): Promise<{ status: string; entries: Agent_Memory_Entry[] }> => {
        const params = new URLSearchParams();
        params.append('query', query);
        if (agent_id) params.append('agent_id', agent_id);

        const result = await api_request<{ status: string; entries: Raw_Agent_Memory_Entry[] }>(`/v1/search/memory?${params.toString()}`, {
            method: 'GET'
        });
        return {
            ...result,
            entries: (result.entries ?? []).map(normalize_agent_memory_entry),
        };
    }
};


// Metadata: [agent_api_service]

// Metadata: [agent_api_service]
