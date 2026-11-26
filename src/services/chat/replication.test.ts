/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Chat Replication Service Tests**: Verifies reference-counted `BroadcastChannel` sync,
 * payload validation, and Multiversal state propagation (`SET_ACTIVE_NODE`, `SET_ACTIVE_MISSION`,
 * `SET_HISTORY`) across detached windows.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Detached portal desync or malformed BroadcastChannel messages causing crashes.
 * - **Trace Scope**: `src/services/chat/replication.test`
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { create_chat_replication } from './replication';
import type { Sovereign_State } from '../../domain/chat/types';

describe('create_chat_replication', () => {
    const store_mock = {
        messages: [],
        add_message: vi.fn(),
        update_message: vi.fn(),
        append_message_part: vi.fn(),
        clear_history: vi.fn(),
        set_scope: vi.fn(),
        set_selected_agent_id: vi.fn(),
        set_target_agent: vi.fn(),
        set_target_cluster: vi.fn(),
        set_detached: vi.fn(),
        set_active_node: vi.fn(),
        set_active_mission: vi.fn(),
        replace_history: vi.fn(),
        active_scope: 'agent' as const,
        target_agent: 'Alpha',
        target_cluster: '',
    };

    const remove_message = vi.fn();
    const subscribe_logs = vi.fn(() => vi.fn());

    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('publishes Multiversal sync envelopes across tabs (Happy Path)', () => {
        const post_spy = vi.spyOn(BroadcastChannel.prototype, 'postMessage');
        const rep = create_chat_replication({
            get: () => store_mock as unknown as Sovereign_State,
            remove_message,
            subscribe_logs,
        });
        const release = rep.acquire();

        rep.publish({ type: 'SET_ACTIVE_NODE', payload: 'node-42' });
        rep.publish({ type: 'SET_ACTIVE_MISSION', payload: 'mission-7' });
        rep.publish({ type: 'SET_HISTORY', payload: [] });

        expect(post_spy).toHaveBeenCalledWith({ type: 'SET_ACTIVE_NODE', payload: 'node-42' });
        expect(post_spy).toHaveBeenCalledWith({ type: 'SET_ACTIVE_MISSION', payload: 'mission-7' });
        expect(post_spy).toHaveBeenCalledWith({ type: 'SET_HISTORY', payload: [] });

        release();
        post_spy.mockRestore();
    });

    it('handles reference counting and ignores malformed incoming envelopes safely (Failure & Edge Case)', () => {
        const channels: Array<{
            listeners: Array<(e: MessageEvent) => void>;
            close: ReturnType<typeof vi.fn>;
        }> = [];
        const OrigBC = globalThis.BroadcastChannel;
        class FakeBC {
            listeners: Array<(e: MessageEvent) => void> = [];
            postMessage = vi.fn();
            close = vi.fn();
            addEventListener = (_event: string, fn: (e: MessageEvent) => void) => {
                this.listeners.push(fn);
            };
            removeEventListener = (_event: string, fn: (e: MessageEvent) => void) => {
                this.listeners = this.listeners.filter((l) => l !== fn);
            };
            constructor() {
                channels.push(this);
            }
        }
        vi.stubGlobal('BroadcastChannel', FakeBC);

        const rep = create_chat_replication({
            get: () => store_mock as unknown as Sovereign_State,
            remove_message,
            subscribe_logs,
        });
        const release1 = rep.acquire();
        const release2 = rep.acquire();

        expect(channels).toHaveLength(1);
        const dispatch = (data: unknown) =>
            channels[0].listeners.forEach((fn) => fn({ data } as MessageEvent));
        dispatch({ type: 'SET_ACTIVE_NODE', payload: 'leaf-99' });
        dispatch({ type: 'SET_ACTIVE_MISSION', payload: 'mission-9' });
        dispatch({ type: 'SET_HISTORY', payload: { messages: [], active_node_id: 'leaf-99' } });
        // Malformed payloads must be ignored without throwing
        dispatch(null);
        dispatch({ type: 'SET_HISTORY', payload: 'not-an-object' });

        expect(store_mock.set_active_node).toHaveBeenCalledWith('leaf-99', true);
        expect(store_mock.set_active_mission).toHaveBeenCalledWith('mission-9', true);
        expect(store_mock.replace_history).toHaveBeenCalledWith([], 'leaf-99', true);

        release1();
        expect(channels[0].close).not.toHaveBeenCalled();
        release2();
        expect(channels[0].close).toHaveBeenCalledOnce();

        vi.stubGlobal('BroadcastChannel', OrigBC);
    });
});
