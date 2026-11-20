/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Local Memory & Visual Monitor Bridge Tests**: Exercises browser-side cosine similarity ranking,
 * zero-vector/dimension-mismatch edge cases in `local_memory_service`, and WebSocket event dispatch
 * (`ui:inspect`, `memory:save`, `memory:search`) in `visual_monitor_bridge`.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: NaN similarity scores on zero vectors or dropped WS bridge responses.
 * - **Telemetry Link**: Search `[local_memory_test]` in test logs.
 */

import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest';

const { stored_memories, add_mock, to_array_mock, embed_mock, custom_listeners, send_event_mock, analyze_dom_mock } = vi.hoisted(() => {
    const memories: Array<{ text: string; metadata: Record<string, unknown>; embedding: number[]; timestamp: number }> = [];
    const listeners: Array<(event: Record<string, unknown>) => Promise<void> | void> = [];
    return {
        stored_memories: memories,
        add_mock: vi.fn(async (entry: { text: string; metadata: Record<string, unknown>; embedding: number[]; timestamp: number }) => {
            memories.push(entry);
        }),
        to_array_mock: vi.fn(async () => [...memories]),
        embed_mock: vi.fn(),
        custom_listeners: listeners,
        send_event_mock: vi.fn(),
        analyze_dom_mock: vi.fn(),
    };
});

vi.mock('dexie', () => {
    class MockDexie {
        version() {
            return {
                stores: () => {
                    (this as unknown as { memories: { add: typeof add_mock; toArray: typeof to_array_mock } }).memories = {
                        add: add_mock,
                        toArray: to_array_mock,
                    };
                },
            };
        }
    }
    return { default: MockDexie };
});

vi.mock('./browser_inference', () => ({
    browser_inference_service: {
        get_embedding: embed_mock,
    },
}));

vi.mock('./socket', () => ({
    tadpole_os_socket: {
        subscribe_custom_event: vi.fn((cb: (event: Record<string, unknown>) => void) => {
            custom_listeners.push(cb);
            return () => undefined;
        }),
        send_event: send_event_mock,
    },
}));

vi.mock('../stores/browser_specialist_store', () => ({
    use_browser_specialist_store: {
        getState: () => ({
            analyze_dom: analyze_dom_mock,
        }),
    },
}));

import { local_memory_service } from './local_memory';
import { visual_monitor_bridge } from './visual_monitor_bridge';

describe('local_memory_service & visual_monitor_bridge', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        stored_memories.length = 0;
    });

    it('saves embedded memories and ranks semantic search by cosine similarity while handling edge vectors', async () => {
        embed_mock
            .mockResolvedValueOnce([1, 0, 0])
            .mockResolvedValueOnce([0, 1, 0])
            .mockResolvedValueOnce([0, 0, 0]) // zero norm edge case
            .mockResolvedValueOnce([1, 0]) // mismatched dimension edge case
            .mockResolvedValueOnce([1, 0, 0]); // query vector

        await local_memory_service.save('Exact match note', { tag: 'a' });
        await local_memory_service.save('Orthogonal note');
        await local_memory_service.save('Zero norm note');
        await local_memory_service.save('Dimension mismatch note');

        const results = await local_memory_service.search('Exact match', 2);
        expect(results).toHaveLength(2);
        expect(results[0]).toBe('[Score: 1.00] Exact match note');
        expect(results[1]).toContain('[Score: 0.00]');
    });

    it('routes ui:inspect, memory:save, and memory:search events via visual_monitor_bridge', async () => {
        visual_monitor_bridge.init();
        visual_monitor_bridge.init(); // idempotent guard
        expect(custom_listeners).toHaveLength(1);

        const dispatch = custom_listeners[0];

        analyze_dom_mock.mockResolvedValueOnce('DOM looks healthy');
        await dispatch({ type: 'ui:inspect', request_id: 'req-42', prompt: 'Check header' });
        await vi.waitFor(() => {
            expect(send_event_mock).toHaveBeenCalledWith({
                type: 'ui:inspect_response',
                request_id: 'req-42',
                analysis: 'DOM looks healthy',
            });
        });

        embed_mock.mockResolvedValueOnce([0.6, 0.8]).mockResolvedValueOnce([0.6, 0.8]);
        await dispatch({ type: 'memory:save', text: 'Saved via WS', metadata: { source: 'ws' } });
        await dispatch({ type: 'memory:search', query: 'Saved', request_id: 'mem-1', limit: 3 });

        expect(send_event_mock).toHaveBeenCalledWith({
            type: 'memory:search_response',
            request_id: 'mem-1',
            results: ['[Score: 1.00] Saved via WS'],
        });
        expect((embed_mock as Mock)).toHaveBeenCalledTimes(2);
    });
});

// Metadata: [local_memory_test]
