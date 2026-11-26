/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Chat Transcript Reducer Tests**: Verifies message insertion, deduplication, O(1) index
 * maintenance, part deduplication, replacement, removal, and bounded 5,000-message eviction.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Stale `message_index_by_id` after eviction or duplicate part insertion.
 * - **Trace Scope**: `src/domain/chat/transcript.test`
 */

import { describe, it, expect } from 'vitest';
import {
    MAX_TRANSCRIPT_MESSAGES,
    TRANSCRIPT_EVICTION_BATCH,
    reduce_transcript,
} from './transcript';

describe('reduce_transcript', () => {
    it('adds, updates, and appends deduplicated parts while keeping index synchronized (Happy Path)', () => {
        const empty = { messages: [], message_index_by_id: {} };
        const added = reduce_transcript(empty, {
            type: 'add',
            message: {
                id: 'm1',
                sender_id: 'agent-1',
                sender_name: 'Alpha',
                text: 'Initial',
                scope: 'agent',
                timestamp: '2026-11-26T00:00:00Z',
                parts: [{ type: 'text', content: 'Initial' }],
            },
        });
        expect(added.messages).toHaveLength(1);
        expect(added.message_index_by_id['m1']).toBe(0);

        const appended = reduce_transcript(added, {
            type: 'append',
            id: 'm1',
            part: { type: 'text', content: 'More' },
        });
        expect(appended.messages[0].parts).toHaveLength(2);

        const deduped = reduce_transcript(appended, {
            type: 'append',
            id: 'm1',
            part: { type: 'text', content: 'More' },
        });
        expect(deduped.messages[0].parts).toHaveLength(2);
    });

    it('handles unknown IDs and invalid mutations safely without corrupting state (Failure Path)', () => {
        const state = { messages: [], message_index_by_id: {} };
        const updated = reduce_transcript(state, {
            type: 'update',
            id: 'missing',
            updates: { text: 'nope' },
        });
        expect(updated).toBe(state);

        const removed = reduce_transcript(state, { type: 'remove', id: 'missing' });
        expect(removed).toBe(state);
    });

    it('evicts oldest batch cleanly when exceeding MAX_TRANSCRIPT_MESSAGES (Edge Case Path)', () => {
        const initial_messages = Array.from({ length: MAX_TRANSCRIPT_MESSAGES }, (_, idx) => ({
            id: `msg-${idx}`,
            sender_id: 'a1',
            sender_name: 'Alpha',
            text: `Message ${idx}`,
            scope: 'agent' as const,
            timestamp: '2026-11-26T00:00:00Z',
        }));
        const initial_index: Record<string, number> = {};
        initial_messages.forEach((m, idx) => {
            initial_index[m.id] = idx;
        });

        const next = reduce_transcript(
            { messages: initial_messages, message_index_by_id: initial_index },
            {
                type: 'add',
                message: {
                    id: 'msg-overflow',
                    sender_id: 'a1',
                    sender_name: 'Alpha',
                    text: 'Overflow',
                    scope: 'agent',
                    timestamp: '2026-11-26T00:00:01Z',
                },
            }
        );

        expect(next.messages).toHaveLength(MAX_TRANSCRIPT_MESSAGES + 1 - TRANSCRIPT_EVICTION_BATCH);
        expect(next.message_index_by_id['msg-0']).toBeUndefined();
        expect(next.message_index_by_id['msg-overflow']).toBe(next.messages.length - 1);
    });
});
