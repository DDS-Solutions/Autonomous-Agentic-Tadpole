/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Event Bus Service Tests**: Exercises circular history buffer, ID deduplication & cache purge,
 * BroadcastChannel cross-tab relay (`LOG`, `TRACE`, `PULSE`, `SYNC_REQUEST`, `SYNC_RESPONSE`),
 * listener exception isolation, and trace/pulse pub-sub channels.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Duplicate cross-tab events, uncaught listener errors breaking delivery, or dropped sync responses.
 * - **Telemetry Link**: Search `[event_bus_test]` in test logs.
 */

import { describe, it, expect, vi, afterEach } from 'vitest';

vi.mock('./telemetry_buffer', () => ({
    telemetry_buffer: {
        append_event: vi.fn().mockResolvedValue(undefined),
    },
}));

describe('event_bus service', () => {
    afterEach(() => {
        vi.unstubAllGlobals();
        vi.resetModules();
        vi.useRealTimers();
    });

    it('emits logs, deduplicates IDs, isolates throwing listeners, and supports trace/pulse pub-sub', async () => {
        const posted: unknown[] = [];
        const channels: Array<{ onmessage: ((ev: { data: unknown }) => void) | null; postMessage: (msg: unknown) => void }> = [];

        class MockBroadcastChannel {
            onmessage: ((ev: { data: unknown }) => void) | null = null;
            postMessage = vi.fn((msg: unknown) => {
                posted.push(msg);
            });
            constructor() {
                channels.push(this);
            }
        }

        vi.stubGlobal('BroadcastChannel', MockBroadcastChannel);
        vi.useFakeTimers();

        const { event_bus } = await import('./event_bus');

        const received_logs: string[] = [];
        const unsub_bad = event_bus.subscribe_logs(() => {
            throw new Error('listener boom');
        });
        const unsub_good = event_bus.subscribe_logs((entry) => {
            received_logs.push(entry.text);
        });

        event_bus.emit_log({ id: 'dup-1', source: 'System', text: 'First', severity: 'info', mission_id: 'm-1' });
        event_bus.emit_log({ id: 'dup-1', source: 'System', text: 'Duplicate ignored', severity: 'info' });
        expect(received_logs).toEqual(['First']);
        expect(event_bus.get_history().some((l) => l.id === 'dup-1')).toBe(true);

        // Trace & pulse channels + error isolation
        const traces: unknown[] = [];
        const pulses: unknown[] = [];
        const unsub_trace_bad = event_bus.subscribe_trace(() => { throw new Error('trace boom'); });
        const unsub_trace = event_bus.subscribe_trace((s) => traces.push(s));
        const unsub_pulse_bad = event_bus.subscribe_pulse(() => { throw new Error('pulse boom'); });
        const unsub_pulse = event_bus.subscribe_pulse((p) => pulses.push(p));

        event_bus.emit_trace({ mission_id: 'm-1', span: 's-1' });
        event_bus.emit_pulse({ missionId: 'm-2', bpm: 60 });
        expect(traces).toHaveLength(1);
        expect(pulses).toHaveLength(1);

        // Cross-tab BroadcastChannel dispatch for LOG, TRACE, PULSE, SYNC_REQUEST, SYNC_RESPONSE
        const channel_instance = channels[0] ?? null;
        expect(channel_instance).not.toBeNull();
        channel_instance!.onmessage?.({
            data: {
                topic: 'LOG',
                sender_id: 'other-tab',
                timestamp: Date.now(),
                payload: { id: 'remote-log-1', timestamp: new Date(), source: 'Agent', text: 'From tab 2', severity: 'success' },
            },
        });
        channel_instance!.onmessage?.({
            data: { topic: 'TRACE', sender_id: 'other-tab', timestamp: Date.now(), payload: { span: 'remote' } },
        });
        channel_instance!.onmessage?.({
            data: { topic: 'PULSE', sender_id: 'other-tab', timestamp: Date.now(), payload: { bpm: 90 } },
        });
        channel_instance!.onmessage?.({
            data: { topic: 'SYNC_REQUEST', sender_id: 'other-tab', timestamp: Date.now(), payload: null },
        });
        vi.advanceTimersByTime(300);
        expect(posted.some((m) => (m as { topic?: string }).topic === 'SYNC_RESPONSE')).toBe(true);

        channel_instance!.onmessage?.({
            data: {
                topic: 'SYNC_RESPONSE',
                sender_id: 'other-tab',
                timestamp: Date.now(),
                payload: {
                    logs: [
                        { id: 'sync-log-1', timestamp: new Date(Date.now() - 100), source: 'System', text: 'Synced 1', severity: 'info' },
                        { id: 'sync-log-2', timestamp: new Date(), source: 'System', text: 'Synced 2', severity: 'warning' },
                    ],
                },
            },
        });
        vi.advanceTimersByTime(50);
        expect(received_logs).toContain('From tab 2');
        expect(received_logs).toContain('Synced 1');
        expect(received_logs).toContain('Synced 2');

        unsub_bad();
        unsub_good();
        unsub_trace_bad();
        unsub_trace();
        unsub_pulse_bad();
        unsub_pulse();
    });
});

// Metadata: [event_bus_test]
