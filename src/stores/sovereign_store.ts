/**
 * @docs ARCHITECTURE:State
 *
 * ### AI Assist Note
 * **Zustand State**: High-level swarm consciousness, bounded sovereign chat transcript,
 * cross-window BroadcastChannel replication, optimistic question decisions, and Multiversal
 * session history / time-travel actions.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Chat history overflow (bounded at 5,000 messages), oversight submission
 *   failure (automatically rolled back), or Multiversal history fetch failure.
 * - **Telemetry Link**: Search `[SovereignStore]` in browser logs.
 */

import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { event_bus } from '../services/event_bus';
import { sovereign_api_service } from '../services/sovereign_api_service';
import { use_notification_store } from './notification_store';
import type {
    Chat_Message,
    Sovereign_State,
    Sovereign_Scope,
    SessionLeaf,
    Mission_Execution_Metrics
} from '../domain/chat/types';
import {
    empty_transcript,
    message_index,
    reduce_transcript,
    type Transcript_Action
} from '../domain/chat/transcript';
import { create_question_decisions } from '../services/chat/question_decisions';
import { create_chat_replication } from '../services/chat/replication';
import type { Message_Part } from '../types';

export type { Chat_Message, Sovereign_Scope, SessionLeaf, Mission_Execution_Metrics, Message_Part };

const TELEMETRY_SOURCE = '[SovereignStore]';

function report_session_error(error: unknown): void {
    use_notification_store.getState().add_notification({
        severity: 'error',
        title: 'Session history',
        message: error instanceof Error ? error.message : 'The session request failed.',
        persistent: false,
    });
}

type Persisted_Sovereign_State = Pick<
    Sovereign_State,
    'active_scope' | 'selected_agent_id' | 'target_agent' | 'target_cluster' | 'is_detached' | 'active_node_id' | 'active_mission_id'
>;

export const use_sovereign_store = create<Sovereign_State>()(
    persist(
        (set, get) => {
            const apply = (action: Transcript_Action) => {
                let changed = false;
                set(state => {
                    const next = reduce_transcript(state, action);
                    changed = next !== state;
                    return next;
                });
                return changed;
            };

            const remove_message = (id: string) => {
                apply({ type: 'remove', id });
            };

            const replication = create_chat_replication({
                get,
                remove_message,
                subscribe_logs: listener => event_bus.subscribe_logs(listener)
            });

            const resolve_user_question = create_question_decisions({
                get,
                remove_message: id => {
                    remove_message(id);
                    replication.publish({ type: 'REMOVE_MESSAGE', payload: id });
                },
                submit: async (id, decision, answer) => {
                    const { system_api_service } = await import('../services/system_api_service');
                    return system_api_service.decide_oversight(id, decision, answer);
                },
                report_failure: agent_id => event_bus.emit_log({
                    source: 'System',
                    severity: 'error',
                    agent_id,
                    text: '[Oversight] Failed to submit decision. Please try again.',
                    metadata: { type: 'log', level: 'error', message: 'Oversight submission failed. Your answer was not recorded.' }
                })
            });

            return {
                ...empty_transcript(),
                active_scope: 'agent',
                selected_agent_id: null,
                target_agent: 'Agent of Nine',
                target_cluster: '',
                is_detached: false,
                active_node_id: null,
                active_mission_id: null,
                session_leaves: [],
                latest_metrics: null,

                add_message: (msg, is_remote_sync) => {
                    const message: Chat_Message = {
                        ...msg,
                        id: msg.id || ((typeof crypto !== 'undefined' && crypto.randomUUID) ? crypto.randomUUID() : Date.now().toString(36)),
                        timestamp: msg.timestamp ?? new Date().toISOString(),
                        parts: msg.parts || (msg.text ? [{ type: 'text', content: msg.text }] : [])
                    };
                    if (apply({ type: 'add', message }) && !is_remote_sync) {
                        replication.publish({ type: 'ADD_MESSAGE', payload: message });
                    }
                },

                update_message: (id, updates, is_remote_sync) => {
                    if (apply({ type: 'update', id, updates }) && !is_remote_sync) {
                        replication.publish({ type: 'UPDATE_MESSAGE', payload: get().get_message_by_id(id) });
                    }
                },

                append_message_part: (id, part, is_remote_sync) => {
                    if (message_index(get(), id) === undefined) return false;
                    if (apply({ type: 'append', id, part }) && !is_remote_sync) {
                        replication.publish({ type: 'UPDATE_MESSAGE', payload: get().get_message_by_id(id) });
                    }
                    return true;
                },

                get_message_by_id: (id) => {
                    const index = message_index(get(), id);
                    return index === undefined ? undefined : get().messages[index];
                },

                replace_history: (history_messages, active_node_id, is_remote_sync) => {
                    set(state => ({
                        ...reduce_transcript(state, { type: 'replace', messages: history_messages }),
                        active_node_id,
                    }));
                    if (!is_remote_sync) {
                        replication.publish({
                            type: 'SET_HISTORY',
                            payload: { messages: history_messages, active_node_id }
                        });
                    }
                },

                set_scope: (active_scope, is_remote_sync) => {
                    if (active_scope !== 'agent' && active_scope !== 'cluster' && active_scope !== 'swarm') {
                        console.error(`${TELEMETRY_SOURCE} Invalid scope payload:`, active_scope);
                        return;
                    }
                    set({ active_scope });
                    if (!is_remote_sync) {
                        replication.publish({ type: 'SET_SCOPE', payload: active_scope });
                    }
                },

                set_selected_agent_id: (selected_agent_id, is_remote_sync) => {
                    set({ selected_agent_id });
                    if (!is_remote_sync) {
                        replication.publish({ type: 'SET_AGENT', payload: selected_agent_id });
                    }
                },

                set_target_agent: (target_agent, is_remote_sync) => {
                    set({ target_agent });
                    if (!is_remote_sync) {
                        replication.publish({ type: 'SET_TARGET_AGENT', payload: target_agent });
                    }
                },

                set_target_cluster: (target_cluster, is_remote_sync) => {
                    set({ target_cluster });
                    if (!is_remote_sync) {
                        replication.publish({ type: 'SET_TARGET_CLUSTER', payload: target_cluster });
                    }
                },

                set_detached: (is_detached, is_remote_sync) => {
                    set({ is_detached });
                    if (!is_remote_sync) {
                        replication.publish({ type: 'SET_DETACHED', payload: is_detached });
                    }
                },

                set_active_node: (active_node_id, is_remote_sync) => {
                    set({ active_node_id });
                    if (!is_remote_sync) {
                        replication.publish({ type: 'SET_ACTIVE_NODE', payload: active_node_id });
                    }
                },

                set_active_mission: (active_mission_id, is_remote_sync) => {
                    set({ active_mission_id });
                    if (!is_remote_sync) {
                        replication.publish({ type: 'SET_ACTIVE_MISSION', payload: active_mission_id });
                    }
                },

                fetch_session_history: async (mission_id, leaf_id) => {
                    try {
                        const history = await sovereign_api_service.history(mission_id, leaf_id);
                        const history_messages: Chat_Message[] = history.map((n) => ({
                            id: n.id,
                            sender_id: n.role === 'user' ? '0' : '1',
                            sender_name: n.role === 'user' ? 'Overlord' : 'Sovereign Agent',
                            text: n.content,
                            timestamp: n.created_at || new Date().toISOString(),
                            scope: 'agent',
                            target_node: n.mission_id
                        }));
                        get().replace_history(history_messages, leaf_id);
                    } catch (err) {
                        console.error(`${TELEMETRY_SOURCE} Failed to fetch multiversal history:`, err);
                        report_session_error(err);
                        throw err;
                    }
                },

                fetch_mission_leaves: async (mission_id) => {
                    try {
                        set({ session_leaves: await sovereign_api_service.leaves(mission_id) });
                    } catch (err) {
                        console.error(`${TELEMETRY_SOURCE} Failed to fetch mission leaves:`, err);
                        report_session_error(err);
                        throw err;
                    }
                },

                revert_to_node: async (mission_id, node_id) => {
                    try {
                        await sovereign_api_service.revert(mission_id, node_id);
                        await get().fetch_session_history(mission_id, node_id);
                    } catch (err) {
                        console.error(`${TELEMETRY_SOURCE} Time Travel failure:`, err);
                        report_session_error(err);
                        throw err;
                    }
                },

                set_execution_metrics: (metrics, is_remote_sync) => {
                    set({ latest_metrics: metrics });
                    if (!is_remote_sync) {
                        replication.publish({ type: 'SET_EXECUTION_METRICS', payload: metrics });
                    }
                },

                resolve_user_question,

                clear_history: (is_remote_sync) => {
                    apply({ type: 'clear' });
                    if (!is_remote_sync) {
                        replication.publish({ type: 'CLEAR_HISTORY' });
                    }
                },

                initialize_sync_listeners: () => replication.acquire(),
            };
        },
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

// Metadata: [sovereign_store]
