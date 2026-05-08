/**
 * @docs ARCHITECTURE:State
 * 
 * ### AI Assist Note
 * **Zustand State**: High-level swarm consciousness and sovereign decision-making logs. 
 * Orchestrates the persistent feedback loop between the human operator (Sovereign) and the autonomous swarm brain.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Chat history buffer overflow, or message delivery failure during heavy backend telemetry pulses.
 * - **Telemetry Link**: Search for `[SovereignStore]` or `MESSAGE_SYNC` in UI logs.
 * 
 * ```mermaid
 * stateDiagram-v2
 *     [*] --> Initializing
 *     Initializing --> Persistence: getItem(tadpole-sovereign-chat)
 *     Persistence --> Migration: migrate(v1 -> v2)
 *     Migration --> Ready: set({messages, index})
 *     Ready --> Message_Flow: add_message(msg)
 *     Message_Flow --> Deduplication: check message_index_by_id
 *     Deduplication --> State_Update: push to array + update index
 *     State_Update --> Sync: BroadcastChannel (ADD_MESSAGE)
 *     Ready --> Part_Append: append_message_part(id, part)
 *     Part_Append --> State_Update
 *     Ready --> Scope_Change: set_scope(scope)
 *     Scope_Change --> Sync: BroadcastChannel (SET_SCOPE)
 *     Ready --> History_Clear: clear_history()
 *     History_Clear --> Sync: BroadcastChannel (CLEAR_HISTORY)
 * ```
 */


import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export interface SessionLeaf {
    id: string;
    role: string;
    content: string;
    created_at: string;
}

const TELEMETRY_SOURCE = '[SovereignStore]';

export type Sovereign_Scope = 'agent' | 'cluster' | 'swarm';

export type Message_Part = 
    | { type: 'text', content: string }
    | { type: 'thought', content: string, status: 'thinking' | 'done' }
    | { type: 'tool', name: string, input: unknown, output?: unknown };

export interface Chat_Message {
    id: string;
    sender_id: string;
    sender_name: string;
    text: string;
    parts?: Message_Part[];
    timestamp: string;
    scope: Sovereign_Scope;
    agent_id?: string;
    is_sub_agent?: boolean;
    lineage?: string[];
    target_node?: string;
}

interface Sovereign_State {
    messages: Chat_Message[];
    message_index_by_id: Record<string, number>;
    active_scope: Sovereign_Scope;
    selected_agent_id: string | null;
    target_agent: string;
    target_cluster: string;
    is_detached: boolean;
    active_node_id: string | null;
    active_mission_id: string | null;
    session_leaves: SessionLeaf[];

    // Actions
    add_message: (msg: Omit<Chat_Message, 'id' | 'timestamp'> & { id?: string, timestamp?: string }) => void;
    update_message: (id: string, updates: Partial<Chat_Message>) => void;
    append_message_part: (id: string, part: Message_Part) => boolean;
    get_message_by_id: (id: string) => Chat_Message | undefined;
    set_scope: (scope: Sovereign_Scope) => void;
    set_selected_agent_id: (agent_id: string | null) => void;
    set_target_agent: (name: string) => void;
    set_target_cluster: (name: string) => void;
    set_detached: (is_detached: boolean) => void;
    set_active_node: (node_id: string | null) => void;
    set_active_mission: (mission_id: string | null) => void;
    fetch_session_history: (mission_id: string, leaf_id: string) => Promise<void>;
    fetch_mission_leaves: (mission_id: string) => Promise<void>;
    clear_history: () => void;
}

type Persisted_Sovereign_State = Pick<
    Sovereign_State,
    'active_scope' | 'selected_agent_id' | 'target_agent' | 'target_cluster' | 'is_detached' | 'active_node_id' | 'active_mission_id'
>;

// Cross-window synchronization
const chat_channel = typeof window !== 'undefined' ? new BroadcastChannel('tadpole-chat-sync') : null;

export const use_sovereign_store = create<Sovereign_State>()(
    persist(
        (set, get) => ({
            messages: [],
            message_index_by_id: {},
            active_scope: 'agent',
            selected_agent_id: null,
            target_agent: 'Agent of Nine',
            target_cluster: '',
            is_detached: false,
            active_node_id: null,
            active_mission_id: null,
            session_leaves: [],

            add_message: (msg) => {
                const new_msg = {
                    ...msg,
                    id: msg.id || ((typeof crypto !== 'undefined' && crypto.randomUUID) ? crypto.randomUUID() : Date.now().toString(36)),
                    timestamp: msg.timestamp || new Date().toISOString(),
                    parts: msg.parts || (msg.text ? [{ type: 'text', content: msg.text }] : [])
                } as Chat_Message;
                let was_added = false;
                set((state) => {
                    if (state.message_index_by_id[new_msg.id] !== undefined) {
                        return state; // Deduplicate
                    }
                    was_added = true;
                    return {
                        messages: [...state.messages, new_msg],
                        message_index_by_id: {
                            ...state.message_index_by_id,
                            [new_msg.id]: state.messages.length
                        }
                    };
                });
                // Sync to other windows
                if (was_added) {
                    chat_channel?.postMessage({ type: 'ADD_MESSAGE', payload: new_msg });
                }
            },

            update_message: (id, updates) => {
                let updated_msg: Chat_Message | undefined;
                set((state) => {
                    const idx = state.message_index_by_id[id];
                    if (idx === undefined) return state;

                    const next_messages = [...state.messages];
                    updated_msg = { ...next_messages[idx], ...updates };
                    next_messages[idx] = updated_msg;
                    return { messages: next_messages };
                });
                if (updated_msg) {
                    chat_channel?.postMessage({ type: 'UPDATE_MESSAGE', payload: updated_msg });
                }
            },

            append_message_part: (id, part) => {
                let updated_msg: Chat_Message | undefined;
                set((state) => {
                    const idx = state.message_index_by_id[id];
                    if (idx === undefined) return state;

                    const current_message = state.messages[idx];
                    updated_msg = {
                        ...current_message,
                        parts: [...(current_message.parts || []), part]
                    };

                    const next_messages = [...state.messages];
                    next_messages[idx] = updated_msg;
                    return { messages: next_messages };
                });

                if (updated_msg) {
                    chat_channel?.postMessage({ type: 'UPDATE_MESSAGE', payload: updated_msg });
                    return true;
                }
                return false;
            },

            get_message_by_id: (id) => {
                const state = get();
                const idx = state.message_index_by_id[id];
                return idx === undefined ? undefined : state.messages[idx];
            },

            set_scope: (active_scope) => {
                set({ active_scope });
                chat_channel?.postMessage({ type: 'SET_SCOPE', payload: active_scope });
            },

            set_selected_agent_id: (selected_agent_id) => {
                set({ selected_agent_id });
                chat_channel?.postMessage({ type: 'SET_AGENT', payload: selected_agent_id });
            },

            set_target_agent: (target_agent) => {
                set({ target_agent });
                chat_channel?.postMessage({ type: 'SET_TARGET_AGENT', payload: target_agent });
            },

            set_target_cluster: (target_cluster) => {
                set({ target_cluster });
                chat_channel?.postMessage({ type: 'SET_TARGET_CLUSTER', payload: target_cluster });
            },

            set_detached: (is_detached) => {
                set({ is_detached });
                chat_channel?.postMessage({ type: 'SET_DETACHED', payload: is_detached });
            },

            set_active_node: (active_node_id) => {
                set({ active_node_id });
                chat_channel?.postMessage({ type: 'SET_ACTIVE_NODE', payload: active_node_id });
            },

            set_active_mission: (active_mission_id) => {
                set({ active_mission_id });
                chat_channel?.postMessage({ type: 'SET_ACTIVE_MISSION', payload: active_mission_id });
            },

            fetch_session_history: async (mission_id, leaf_id) => {
                try {
                    const response = await fetch(`/v1/sovereign/missions/${mission_id}/nodes/${leaf_id}/history`, {
                        headers: {
                            'Authorization': `Bearer ${localStorage.getItem('NEURAL_TOKEN')}`
                        }
                    });
                    const data = await response.json();
                    if (data.status === 'success') {
                        const history_messages = data.history.map((n: any) => ({
                            id: n.id,
                            sender_id: n.role === 'user' ? '0' : '1',
                            sender_name: n.role === 'user' ? 'Overlord' : 'Sovereign Agent',
                            text: n.content,
                            timestamp: n.created_at || new Date().toISOString(),
                            scope: 'agent',
                            target_node: n.mission_id
                        }));
                        set({ messages: history_messages, active_node_id: leaf_id });
                    }
                } catch (err) {
                    console.error(`${TELEMETRY_SOURCE} Failed to fetch multiversal history:`, err);
                }
            },

            fetch_mission_leaves: async (mission_id) => {
                try {
                    const response = await fetch(`/v1/sovereign/missions/${mission_id}/leaves`, {
                        headers: {
                            'Authorization': `Bearer ${localStorage.getItem('NEURAL_TOKEN')}`
                        }
                    });
                    const data = await response.json();
                    if (data.status === 'success') {
                        set({ session_leaves: data.leaves });
                    }
                } catch (err) {
                    console.error(`${TELEMETRY_SOURCE} Failed to fetch mission leaves:`, err);
                }
            },

            clear_history: () => {
                set({ messages: [], message_index_by_id: {} });
                chat_channel?.postMessage({ type: 'CLEAR_HISTORY' });
            },
        }),
        {
            name: 'tadpole-sovereign-chat',
            version: 2,
            partialize: (state): Persisted_Sovereign_State => ({
                active_scope: state.active_scope,
                selected_agent_id: state.selected_agent_id,
                target_agent: state.target_agent,
                target_cluster: state.target_cluster,
                is_detached: state.is_detached,
                active_node_id: state.active_node_id,
                active_mission_id: state.active_mission_id
            }),
        }
    )
);

// Listen for sync events
if (chat_channel) {
    chat_channel.onmessage = (event: MessageEvent) => {
        const { type, payload } = event.data as { type: string, payload: unknown };
        const state = use_sovereign_store.getState();

        switch (type) {
            case 'ADD_MESSAGE':
                {
                    const msg = payload as Chat_Message;
                    if (state.message_index_by_id[msg.id] === undefined) {
                        use_sovereign_store.setState({
                            messages: [...state.messages, msg],
                            message_index_by_id: {
                                ...state.message_index_by_id,
                                [msg.id]: state.messages.length
                            }
                        });
                    }
                }
                break;
            case 'UPDATE_MESSAGE':
                {
                    const msg = payload as Chat_Message;
                    const idx = state.message_index_by_id[msg.id];
                    if (idx !== undefined) {
                        const next_messages = [...state.messages];
                        next_messages[idx] = msg;
                        use_sovereign_store.setState({ messages: next_messages });
                    }
                }
                break;
            case 'SET_SCOPE':
                use_sovereign_store.setState({ active_scope: payload as Sovereign_Scope });
                break;
            case 'SET_AGENT':
                use_sovereign_store.setState({ selected_agent_id: payload as string | null });
                break;
            case 'SET_TARGET_AGENT':
                use_sovereign_store.setState({ target_agent: payload as string });
                break;
            case 'SET_TARGET_CLUSTER':
                use_sovereign_store.setState({ target_cluster: payload as string });
                break;
            case 'SET_DETACHED':
                use_sovereign_store.setState({ is_detached: payload as boolean });
                break;
            case 'SET_ACTIVE_NODE':
                use_sovereign_store.setState({ active_node_id: payload as string | null });
                break;
            case 'SET_ACTIVE_MISSION':
                use_sovereign_store.setState({ active_mission_id: payload as string | null });
                break;
            case 'CLEAR_HISTORY':
                use_sovereign_store.setState({ messages: [], message_index_by_id: {} });
                break;
        }
    };
}


// Metadata: [sovereign_store]

// Metadata: [sovereign_store]
