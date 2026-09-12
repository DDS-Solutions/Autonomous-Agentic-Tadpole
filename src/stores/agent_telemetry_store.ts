/**
 * @docs ARCHITECTURE:Stores
 * 
 * ### AI Assist Note
 * **Agent Telemetry Store**: Manages high-frequency, real-time status 
 * overrides for the Tadpole OS swarm. Features **Volatile Reactivity**: 
 * status updates (heartbeats, task progress) are handled via raw 
 * WebSockets and merged dynamically into the UI without full 
 * registry re-hydration (AGNT-02).
 * 
 * ### 🔍 Debugging & Observability
 * - **Telemetry Link**: Search `[agent_telemetry_store]` in console logs.
 * - **Failure Path**: Socket disconnection, telemetry drift from canonical 
 *   registry state, or high-concurrency UI flickering.
 * - **Trace Scope**: `src/stores/agent_telemetry_store`
 */

import { create } from 'zustand';
import { tadpole_os_socket } from '../services/socket';
import { use_agent_registry_store } from './agent_registry_store';
import type { Agent, AgentDto } from '../types';
import { normalize_agent_dto } from '../domain/agents/normalizers';

export interface Telemetry_State {
    /** Map of Agent ID to real-time status overrides */
    live_status: Record<string, Partial<Agent>>;
    
    init_telemetry: () => () => void;
}

export const use_agent_telemetry_store = create<Telemetry_State>()((set) => ({
    live_status: {},

    init_telemetry: () => {
        const unsubscribe = tadpole_os_socket.subscribe_agent_updates((event) => {
            if (!event) return;

            if (event.type === 'agent:update' || event.type === 'agent:create') {
                if (!event.agent_id || !event.data) return;
                const id = event.agent_id;
                
                const existing_agent = use_agent_registry_store.getState().get_agent(id);
                const raw_data = event.data as Record<string, unknown>;
                
                let normalized_updates: Partial<Agent>;
                // If the event payload contains structural model fields, run through full DTO normalizer
                if (raw_data.modelConfig || raw_data.model_config || raw_data.model || raw_data.model2 || raw_data.model_2) {
                    normalized_updates = normalize_agent_dto(event.data as AgentDto, existing_agent?.workspace_path, existing_agent);
                } else {
                    normalized_updates = {
                        ...(event.data as Partial<Agent>),
                        status: raw_data.status === 'working' ? 'active' : (raw_data.status as Agent['status']),
                        current_task: (raw_data.currentTask ?? raw_data.current_task) as string | undefined,
                        tokens_used: (raw_data.tokensUsed ?? raw_data.tokens_used) as number | undefined,
                        failure_count: (raw_data.failureCount ?? raw_data.failure_count) as number | undefined,
                        last_pulse: (raw_data.lastPulse ?? raw_data.last_pulse) as string | null | undefined
                    };
                }
                
                set(state => ({
                    live_status: {
                        ...state.live_status,
                        [id]: {
                            ...state.live_status[id],
                            ...normalized_updates,
                            _telemetry_timestamp: Date.now()
                        }
                    }
                }));

                // If it's a "create" event and not in registry, trigger a registry refresh
                if (event.type === 'agent:create') {
                    const registry = use_agent_registry_store.getState();
                    if (!registry.get_agent(id)) {
                        registry.fetch_agents();
                    }
                }
            } else if (event.type === 'engine:ui_invalidate' && event.resource === 'agents') {
                use_agent_registry_store.getState().fetch_agents();
            }
        });

        return unsubscribe;
    }
}));





// Metadata: [agent_telemetry_store]
