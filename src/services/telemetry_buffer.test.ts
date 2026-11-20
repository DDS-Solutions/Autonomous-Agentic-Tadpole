/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Telemetry Buffer Service Tests**: Verifies IndexedDB schema initialization, event appending,
 * compound-index and fallback querying, 7-day rolling TTL pruning, mission clearing, and graceful
 * degradation when IndexedDB is unavailable.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Uncaught IndexedDB errors or dropped time-travel events.
 * - **Telemetry Link**: Search `[telemetry_buffer_test]` in test logs.
 */

import { describe, it, expect, vi, afterEach } from 'vitest';

interface StoredRecord {
    id: number;
    mission_id: string;
    event_type: 'handoff' | 'swarm_pulse' | 'log' | 'trace';
    timestamp: number;
    payload: Record<string, unknown>;
}

function install_mock_indexed_db(use_compound_index = true) {
    const records: StoredRecord[] = [];
    let seq = 1;
    const created_indexes = new Set<string>(use_compound_index ? ['mission_id', 'timestamp', 'mission_timestamp'] : ['mission_id', 'timestamp']);

    const make_store = () => ({
        indexNames: { contains: (name: string) => created_indexes.has(name) },
        createIndex: vi.fn((name: string) => { created_indexes.add(name); }),
        add: vi.fn((event: Omit<StoredRecord, 'id'>) => {
            records.push({ ...event, id: seq++ });
        }),
        index: (name: string) => ({
            getAll: vi.fn((range?: { kind: string; lower?: unknown; upper?: unknown; value?: unknown }) => {
                const req: Record<string, unknown> = { result: [] };
                queueMicrotask(() => {
                    if (name === 'mission_timestamp' && range?.kind === 'bound') {
                        const [mId, start] = range.lower as [string, number];
                        const [, end] = range.upper as [string, number];
                        req.result = records.filter(r => r.mission_id === mId && r.timestamp >= start && r.timestamp <= end);
                    } else if (name === 'mission_id' && range?.kind === 'only') {
                        req.result = records.filter(r => r.mission_id === range.value);
                    } else {
                        req.result = [...records];
                    }
                    (req.onsuccess as (() => void) | undefined)?.();
                });
                return req;
            }),
            openCursor: vi.fn((range?: { kind: string; upper?: number; value?: string }) => {
                const req: Record<string, unknown> = { result: null };
                const matching = records.filter(r => {
                    if (name === 'timestamp' && range?.kind === 'upper') return r.timestamp <= (range.upper ?? 0);
                    if (name === 'mission_id' && range?.kind === 'only') return r.mission_id === range.value;
                    return false;
                });
                let idx = 0;
                const emit_next = () => {
                    if (idx < matching.length) {
                        const target = matching[idx++];
                        req.result = {
                            delete: () => {
                                const pos = records.findIndex(r => r.id === target.id);
                                if (pos >= 0) records.splice(pos, 1);
                            },
                            continue: () => queueMicrotask(emit_next),
                        };
                    } else {
                        req.result = null;
                    }
                    (req.onsuccess as (() => void) | undefined)?.();
                };
                queueMicrotask(emit_next);
                return req;
            }),
        }),
    });

    const mock_db = {
        objectStoreNames: { contains: () => false },
        createObjectStore: vi.fn(() => make_store()),
        transaction: vi.fn(() => {
            const store = make_store();
            const tx: Record<string, unknown> = {
                objectStore: () => store,
            };
            queueMicrotask(() => (tx.oncomplete as (() => void) | undefined)?.());
            return tx;
        }),
    };

    const mock_idb = {
        open: vi.fn(() => {
            const req: Record<string, unknown> = { result: mock_db };
            queueMicrotask(() => {
                (req.onupgradeneeded as ((e: { target: { result: typeof mock_db } }) => void) | undefined)?.({
                    target: { result: mock_db },
                });
                (req.onsuccess as (() => void) | undefined)?.();
            });
            return req;
        }),
    };

    const mock_key_range = {
        bound: (lower: unknown, upper: unknown) => ({ kind: 'bound', lower, upper }),
        only: (value: unknown) => ({ kind: 'only', value }),
        upperBound: (upper: unknown) => ({ kind: 'upper', upper }),
    };

    vi.stubGlobal('indexedDB', mock_idb);
    vi.stubGlobal('IDBKeyRange', mock_key_range);
    return { records, mock_idb };
}

describe('TelemetryBufferService', () => {
    afterEach(() => {
        vi.unstubAllGlobals();
        vi.resetModules();
    });

    it('appends, queries via compound index, prunes expired records, and clears a mission', async () => {
        const { records } = install_mock_indexed_db(true);
        const { telemetry_buffer } = await import('./telemetry_buffer');

        await telemetry_buffer.append_event('m-1', 'handoff', { from: 'ceo' });
        await telemetry_buffer.append_event('m-2', 'log', { msg: 'other' });
        // Inject an expired 10-day-old record
        records.push({
            id: 999,
            mission_id: 'm-1',
            event_type: 'trace',
            timestamp: Date.now() - 10 * 86400 * 1000,
            payload: { stale: true },
        });

        await telemetry_buffer.prune_old_events(7);
        expect(records.some(r => r.id === 999)).toBe(false);

        const m1_events = await telemetry_buffer.query_events('m-1');
        expect(m1_events).toHaveLength(1);
        expect(m1_events[0].event_type).toBe('handoff');

        await telemetry_buffer.clear_mission('m-1');
        expect(await telemetry_buffer.query_events('m-1')).toHaveLength(0);
        expect(await telemetry_buffer.query_events('m-2')).toHaveLength(1);
    });

    it('falls back to mission_id index filtering and degrades gracefully without IndexedDB', async () => {
        install_mock_indexed_db(false);
        const { telemetry_buffer } = await import('./telemetry_buffer');

        await telemetry_buffer.append_event('m-fallback', 'swarm_pulse', { p: 1 });
        const rows = await telemetry_buffer.query_events('m-fallback', 1, Date.now() + 10_000);
        expect(rows).toHaveLength(1);

        vi.resetModules();
        vi.stubGlobal('indexedDB', undefined);
        const { telemetry_buffer: no_idb } = await import('./telemetry_buffer');
        expect(await no_idb.query_events('m-none')).toEqual([]);
        await expect(no_idb.append_event('m-none', 'log', {})).resolves.toBeUndefined();
    });
});

// Metadata: [telemetry_buffer_test]
