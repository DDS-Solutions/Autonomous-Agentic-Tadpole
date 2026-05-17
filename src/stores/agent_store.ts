/**
 * @docs ARCHITECTURE:Stores
 * 
 * ### AI Assist Note
 * **Agent Facade Store**: Aggregates the Registry (canonical) and 
 * Telemetry (real-time) stores into a unified reactive interface. 
 * Implements the **Nexus Reconciliation Pattern**: merges stable 
 * configuration data with volatile status overrides to provide 
 * a single source of truth for the frontend (AGNT-03).
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Reconciliation lag or inconsistent identity mapping 
 *   between registry and telemetry streams.
 * - **Trace Scope**: `src/stores/agent_store`
 */

import type { Agent } from '../types';
import { use_agent_registry_store } from './agent_registry_store';
import { use_agent_telemetry_store } from './agent_telemetry_store';

export interface Agent_Store_State {
    agents: Agent[];
    is_loading: boolean;
    error: string | null;
    fetch_agents: (options?: RequestInit) => Promise<void>;
    update_agent: (id: string, updates: Partial<Agent>) => Promise<void>;
    add_agent: (agent: Agent) => Promise<boolean>;
    get_agent: (id: string) => Agent | undefined;
    init_telemetry: () => () => void;
}

export const use_agent_store = <T = Agent_Store_State>(
    selector?: (state: Agent_Store_State) => T
): T => {
    const registry = use_agent_registry_store();
    const telemetry = use_agent_telemetry_store();

    // RECONCILIATION: Merge Registry Agents with Live Telemetry
    // This ensures that components always see the most up-to-date health and task status
    // without triggering full registry re-renders.
    const raw_agents = Array.isArray(registry.agents) ? registry.agents : [];
    const agents = raw_agents.map(agent => {
        const live = telemetry.live_status[agent.id];
        if (live) {
            return { ...agent, ...live };
        }
        return agent;
    });

    const state: Agent_Store_State = {
        // Combined State
        agents,
        is_loading: registry.is_loading,
        error: registry.error,
        
        // Registry Actions (Persistence & CRUD)
        fetch_agents: registry.fetch_agents,
        update_agent: registry.update_agent,
        add_agent: registry.add_agent,
        get_agent: (id: string) => agents.find(a => a.id === id),
        
        // Telemetry Actions (Real-time Socket)
        init_telemetry: telemetry.init_telemetry,
    };

    if (selector) {
        return selector(state);
    }
    return state as unknown as T;
};

// Re-export individual stores for granular subscriptions (Performance Optimization)
export { use_agent_registry_store, use_agent_telemetry_store };

// Metadata: [agent_store]

// Metadata: [agent_store]
