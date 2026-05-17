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
 * - **Failure Path**: Socket disconnection, telemetry drift from canonical 
 *   registry state, or high-concurrency UI flickering.
 * - **Trace Scope**: `src/stores/agent_telemetry_store`
 */

import { create } from 'zustand';
import { tadpole_os_socket } from '../services/socket';
import { use_workspace_store } from './workspace_store';
import { use_agent_registry_store } from './agent_registry_store';
import type { Agent } from '../types';

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
                
                // Resolve Workspace context for normalization
                const workspace_store = use_workspace_store.getState();
                const cluster = (workspace_store.clusters || []).find(c => (c.collaborators || []).includes(id));

                const updates = event.data as Partial<Agent>;
                
                set(state => ({
                    live_status: {
                        ...state.live_status,
                        [id]: {
                            ...state.live_status[id],
                            ...updates,
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

// Metadata: [agent_telemetry_store]
