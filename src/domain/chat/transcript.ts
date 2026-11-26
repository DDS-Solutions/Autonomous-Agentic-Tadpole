/**
 * @docs ARCHITECTURE:State
 *
 * ### AI Assist Note
 * **Transcript Reducer**: Pure immutable transcript state transitions owning O(1) message identity
 * indexes, duplicate part suppression, and bounded 5,000-message history eviction.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Unbounded transcript growth or stale `message_index_by_id` after eviction.
 * - **Telemetry Link**: Search `[transcript]` in logs.
 */

import type { Message_Part } from '../../types';
import type { Chat_Message } from './types';

export const MAX_TRANSCRIPT_MESSAGES = 5000;
export const TRANSCRIPT_EVICTION_BATCH = 200;

export interface Transcript_State {
    messages: Chat_Message[];
    message_index_by_id: Record<string, number>;
}

export type Transcript_Action =
    | { type: 'add'; message: Chat_Message }
    | { type: 'update'; id: string; updates: Partial<Chat_Message> }
    | { type: 'append'; id: string; part: Message_Part }
    | { type: 'remove'; id: string }
    | { type: 'replace'; messages: Chat_Message[] }
    | { type: 'clear' };

export const empty_transcript = (): Transcript_State => ({ messages: [], message_index_by_id: {} });

export function indexed_transcript(messages: Chat_Message[]): Transcript_State {
    return { messages, message_index_by_id: Object.fromEntries(messages.map((m, i) => [m.id, i])) };
}

export function message_index(state: Transcript_State, id: string): number | undefined {
    return Object.hasOwn(state.message_index_by_id, id) ? state.message_index_by_id[id] : undefined;
}

function same_part(existing: Message_Part, incoming: Message_Part): boolean {
    if (existing.type !== incoming.type) return false;
    if (existing.type === 'question' && incoming.type === 'question') {
        return existing.question_id === incoming.question_id;
    }
    return 'content' in existing && 'content' in incoming
        ? existing.content === incoming.content
        : JSON.stringify(existing) === JSON.stringify(incoming);
}

export function reduce_transcript(state: Transcript_State, action: Transcript_Action): Transcript_State {
    if (action.type === 'clear') return empty_transcript();
    if (action.type === 'replace') {
        const bounded = action.messages.length > MAX_TRANSCRIPT_MESSAGES
            ? action.messages.slice(action.messages.length - MAX_TRANSCRIPT_MESSAGES)
            : action.messages;
        return indexed_transcript(bounded);
    }
    const id = action.type === 'add' ? action.message.id : action.id;
    const index = message_index(state, id);
    if (action.type === 'add') {
        if (index !== undefined) return state;
        if (state.messages.length >= MAX_TRANSCRIPT_MESSAGES) {
            return indexed_transcript([...state.messages.slice(TRANSCRIPT_EVICTION_BATCH), action.message]);
        }
        return {
            messages: [...state.messages, action.message],
            message_index_by_id: { ...state.message_index_by_id, [id]: state.messages.length }
        };
    }
    if (index === undefined) return state;
    if (action.type === 'remove') return indexed_transcript(state.messages.filter(m => m.id !== id));
    const current = state.messages[index];
    if (action.type === 'append' && current.parts?.some(p => same_part(p, action.part))) return state;
    const updated = action.type === 'update'
        ? { ...current, ...action.updates, id }
        : { ...current, parts: [...(current.parts || []), action.part] };
    const messages = [...state.messages];
    messages[index] = updated;
    return { messages, message_index_by_id: state.message_index_by_id };
}

// Metadata: [transcript]
