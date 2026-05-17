/**
 * @docs ARCHITECTURE:Stores
 * 
 * ### AI Assist Note
 * **Agent Registry Store**: Orchestrates the canonical state management 
 * for all agents within the Tadpole OS swarm. Features **Durable Persistence** 
 * via `zustand/middleware/persist` and **Cross-Tab Synchronization** 
 * via `BroadcastChannel` to ensure a unified view of agent identity 
 * and configuration (AGNT-01).
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Race conditions during multi-tab updates, 
 *   persistence hydration failures, or stale registry data.
 * - **Trace Scope**: `src/stores/agent_registry_store`
 */

import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { Agent } from '../types';
import { load_agents, persist_agent_update, normalize_agent, type Raw_Agent } from '../services/agent_service';
import { agent_api_service } from '../services/agent_api_service';
import { log_error } from '../services/system_utils';
import { agents as mock_agents } from '../data/mock_agents';

const SYNC_CHANNEL = 'tadpole-os-sync';
const sync_channel = typeof window !== 'undefined' ? new BroadcastChannel(SYNC_CHANNEL) : null;
const TAB_ID = typeof crypto !== 'undefined' && crypto.randomUUID ? crypto.randomUUID() : `tab-${Date.now()}`;

export interface Agent_Registry_State {
    agents: Agent[];
    is_loading: boolean;
    error: string | null;
    last_fetch_time: number;
    
    fetch_agents: (options?: RequestInit) => Promise<void>;
    update_agent: (id: string, updates: Partial<Agent>) => Promise<void>;
    add_agent: (agent: Agent) => Promise<boolean>;
    get_agent: (id: string) => Agent | undefined;
    
    // Internal sync helpers
    _set_agents_silently: (agents: Agent[]) => void;
}

export const use_agent_registry_store = create<Agent_Registry_State>()(
    persist(
        (set, get) => ({
            agents: [],
            is_loading: false,
            error: null,
            last_fetch_time: 0,

            fetch_agents: async (options = {}) => {
                const now = Date.now();
                if (get().is_loading || (get().agents.length > 0 && now - get().last_fetch_time < 3000)) return;

                set({ is_loading: true, error: null, last_fetch_time: now });
                try {
                    const live_agents = (await load_agents(options)) || [];
                    let final_agents: Agent[];

                    if (live_agents.length === 0 && mock_agents.length > 0) {
                        final_agents = (mock_agents as unknown as Raw_Agent[]).map(raw => {
                            const existing = get().agents.find(a => a.id === raw.id);
                            return existing || normalize_agent(raw);
                        });
                    } else {
                        final_agents = live_agents.map(la => {
                            const local = get().agents.find(a => a.id === la.id);
                            if (local && (local._local_timestamp || 0) > (la._local_timestamp || 0)) {
                                return { ...la, ...local };
                            }
                            return la;
                        });
                    }

                    set({ agents: final_agents, is_loading: false });
                    sync_channel?.postMessage({ type: 'agents:replace', payload: final_agents, source_id: TAB_ID });
                } catch (err) {
                    log_error('AgentRegistry', 'Fetch Failed', err);
                    const hydrated_mock = (mock_agents as unknown as Raw_Agent[]).map(raw => {
                        const existing = get().agents.find(a => a.id === raw.id);
                        return existing || normalize_agent(raw);
                    });
                    set({ agents: hydrated_mock, is_loading: false, error: 'Registry Sync Error' });
                }
            },

            update_agent: async (id, updates) => {
                const timestamp = Date.now();
                set(state => ({
                    agents: state.agents.map(a => a.id === id ? { ...a, ...updates, _local_timestamp: timestamp } : a)
                }));
                sync_channel?.postMessage({ type: 'agent:update', payload: { id, updates }, source_id: TAB_ID });

                try {
                    await persist_agent_update(id, updates);
                } catch (err) {
                    log_error('AgentRegistry', `Persistence Error: ${id}`, err, 'warning');
                }
            },

            add_agent: async (agent) => {
                set(state => ({ agents: [...state.agents, agent] }));
                sync_channel?.postMessage({ type: 'agent:add', payload: agent, source_id: TAB_ID });

                try {
                    await agent_api_service.create_agent(agent);
                    return true;
                } catch (err) {
                    log_error('AgentRegistry', 'Add Failed', err);
                    const fresh = await load_agents();
                    set({ agents: fresh || [] });
                    return false;
                }
            },

            get_agent: (id) => get().agents.find(a => a.id === id),

            _set_agents_silently: (agents) => set({ agents })
        }),
        {
            name: 'tadpole-agent-registry-v2',
            partialize: (state) => ({ agents: state.agents }),
        }
    )
);

// Cross-tab sync listener
if (sync_channel) {
    sync_channel.onmessage = (event) => {
        const { type, payload, source_id } = event.data;
        if (source_id === TAB_ID) return;

        const store = use_agent_registry_store.getState();
        if (type === 'agent:update') {
            store._set_agents_silently(store.agents.map(a => a.id === payload.id ? { ...a, ...payload.updates } : a));
        } else if (type === 'agent:add') {
            if (!store.agents.some(a => a.id === payload.id)) {
                store._set_agents_silently([...store.agents, payload]);
            }
        } else if (type === 'agents:replace') {
            store._set_agents_silently(payload);
        }
    };
}

// Metadata: [agent_registry_store]

// Metadata: [agent_registry_store]
