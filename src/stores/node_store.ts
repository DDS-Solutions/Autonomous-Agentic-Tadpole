/**
 * @docs ARCHITECTURE:State
 * 
 * ### AI Assist Note
 * **Zustand State**: Physical and virtual compute node orchestrator. 
 * Manages the state of distributed worker nodes, their heartbeat status, and resource allocation metrics.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Node heartbeat timeout (unreachable), resource reporting overflow (NaN in metrics), or compute shard desync.
 * - **Telemetry Link**: Search for `[NodeStore]` or `NODE_HEARTBEAT` in service logs.
 */

import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { tadpole_os_service } from '../services/tadpoleos_service';
import { event_bus } from '../services/event_bus';
import { log_error } from '../services/system_utils';
import type { Swarm_Node } from '../types';

/**
 * Node_State
 * Managed state for Swarm Bunker nodes.
 * Refactored for strict snake_case compliance for backend parity.
 */
export interface Node_State {
    nodes: Swarm_Node[];
    is_loading: boolean;

    fetch_nodes: (options?: RequestInit) => Promise<void>;
    discover_nodes: () => Promise<void>;
}

const SYNC_CHANNEL = 'tadpole-os-nodes-sync';
const sync_channel = typeof window !== 'undefined' ? new BroadcastChannel(SYNC_CHANNEL) : null;
const TAB_ID = typeof crypto !== 'undefined' && crypto.randomUUID ? crypto.randomUUID() : `tab-${Date.now()}`;

let is_applying_sync = false;

/**
 * use_node_store
 * Global store for managing physical and virtual infrastructure nodes.
 */
export const use_node_store = create<Node_State>()(
    persist(
        (set, get) => ({
            nodes: [],
            is_loading: false,

            fetch_nodes: async (options: RequestInit = {}) => {
                set({ is_loading: true });
                try {
                    const nodes = await tadpole_os_service.get_nodes(options) as unknown as Swarm_Node[];
                    set({ nodes, is_loading: false });
                    
                    if (!is_applying_sync && sync_channel) {
                        sync_channel.postMessage({ type: 'SYNC_NODES', payload: nodes, source_id: TAB_ID });
                    }
                } catch (error: unknown) {
                    set({ is_loading: false });
                    log_error('NodeStore', 'Node Retrieval Failed', error);
                }
            },

            discover_nodes: async () => {
                set({ is_loading: true });
                try {
                    const data = await tadpole_os_service.discover_nodes();
                    if (data.status === 'success' && data.discovered && data.discovered.length > 0) {
                        event_bus.emit_log({
                            source: 'System',
                            text: `📡 Network Scan: ${data.discovered.length} new node(s) identified.`,
                            severity: 'success'
                        });
                        await get().fetch_nodes();
                    } else {
                        event_bus.emit_log({
                            source: 'System',
                            text: `📡 Network Scan: No new nodes found.`,
                            severity: 'info'
                        });
                    }
                    set({ is_loading: false });
                } catch (error: unknown) {
                    set({ is_loading: false });
                    log_error('NodeStore', 'Node Discovery Failed', error);
                }
            }
        }),
        {
            name: 'tadpole-nodes-v2' // Incremented version
        }
    )
);

if (sync_channel) {
    sync_channel.onmessage = (event) => {
        if (event.data.type === 'SYNC_NODES' && event.data.source_id !== TAB_ID) {
            is_applying_sync = true;
            use_node_store.setState({ nodes: event.data.payload });
            is_applying_sync = false;
        } else if (event.data.type === 'REQUEST_NODES') {
            sync_channel.postMessage({ 
                type: 'SYNC_NODES', 
                payload: use_node_store.getState().nodes,
                source_id: TAB_ID 
            });
        }
    };

    // Request initial state from other tabs
    setTimeout(() => {
        if (use_node_store.getState().nodes.length === 0) {
            sync_channel.postMessage({ type: 'REQUEST_NODES', source_id: TAB_ID });
        }
    }, 200);
}


// Metadata: [node_store]

// Metadata: [node_store]
