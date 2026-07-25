/**
 * @docs ARCHITECTURE:Services
 * 
 * ### AI Assist Note
 * **Infrastructure Bus**: Global Pub/Sub notification and telemetry relay. 
 * Orchestrates cross-subsystem event propagation (swarms, logs, security alerts) and manages high-velocity pulse buffering for the UI.
 * Features jittered broadcast storm prevention and async chunked listener delivery to prevent event loop starvation during autonomous swarm bursts.
 * 
 * ### 🧬 Logic Flow (Mermaid)
 * ```mermaid
 * sequenceDiagram
 *     participant S as Source Component
 *     participant EB as EventBus (Service)
 *     participant RB as Ring Buffer (Cache)
 *     participant L as Subscribed Listeners
 *     participant BC as BroadcastChannel (Cross-Tab)
 * 
 *     S->>EB: emit_log(entry)
 *     EB->>EB: Generate ID/Timestamp
 *     EB->>RB: Store in Ring Buffer (O(1))
 *     EB->>L: trigger(full_entry)
 *     EB->>BC: postMessage(EVENT_EMIT)
 *     BC-->>EB: onmessage (Deduplicate & Store)
 * ```
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Circular buffer overflow (clears oldest entry), ID cache saturation, or BroadcastChannel disconnect in non-secure browser contexts.
 * - **Telemetry Link**: Global log stream. Search for `[event_bus]` in tracing.
 * 
 * @aiContext
 * - **Dependencies**: `BroadcastChannel` (for cross-tab sync).
 * - **Side Effects**: Emits global log entries and broadcasts them to all open browser contexts.
 */

/**
 * @module event_bus
 * Central pub/sub service that synchronizes the Terminal, Voice Comms,
 * and WebSocket log stream into a single unified event timeline.
 */

import { telemetry_buffer } from './telemetry_buffer';

/** Origin of a log entry. */
type log_source = 'User' | 'System' | 'Agent';

/** Visual severity used for color-coding in the Terminal UI. */
type log_severity = 'info' | 'success' | 'warning' | 'error';

/** A single event in the unified command timeline. */
export interface log_entry {
    /** Unique identifier (auto-generated). */
    id: string;
    /** When the event occurred (auto-generated). */
    timestamp: Date;
    /** Who produced this entry. */
    source: log_source;
    /** Human-readable message content. */
    text: string;
    /** Severity level for UI color-coding. */
    severity: log_severity;
    /** The originating agent's ID, if `source` is `'Agent'`. */
    agent_id?: string;
    /** The originating agent's friendly name, if available. */
    agent_name?: string;
    /** The associated mission (cluster) ID, if applicable. */
    mission_id?: string;
    /** RFC 9457 Error URI for machine-readable error handling. */
    type_id?: string;
    /** Flexible metadata for extended diagnostic display. */
    metadata?: Record<string, unknown>;
}

/** Unified Telemetry Message Wrapper */
export interface telemetry_message {
    topic: 'LOG' | 'TRACE' | 'PULSE' | 'OVERSIGHT' | 'SYNC_REQUEST' | 'SYNC_RESPONSE';
    payload: unknown;
    timestamp: number;
    sender_id: string;
}

type Listener = (entry: log_entry) => void;

/**
 * Lightweight pub/sub event bus.
 * Components subscribe to receive {@link log_entry} objects in real time.
 * History uses a true circular buffer (no array reallocation).
 */
class event_bus_service {
    private listeners: Listener[] = [];
    private trace_listeners: ((span: unknown) => void)[] = [];
    private pulse_listeners: ((pulse: unknown) => void)[] = [];

    /** Circular buffer for history — avoids array reallocation on overflow. */
    private static readonly BUFFER_SIZE = 1000;
    private ring: (log_entry | null)[] = new Array(event_bus_service.BUFFER_SIZE).fill(null);
    private head = 0;   // write pointer
    private count = 0;  // number of entries currently stored
    private channel: BroadcastChannel | null = (typeof window !== 'undefined' && typeof BroadcastChannel !== 'undefined') ? new BroadcastChannel('tadpole-neural-hub') : null;
    private pending_sync_response: ReturnType<typeof setTimeout> | null = null;
    private instance_id = (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function')
        ? crypto.randomUUID()
        : (typeof performance !== 'undefined' ? (Math.floor(performance.now() * 1000) % 1000000).toString(36) : Date.now().toString(36));
    
    /** Track recently processed IDs to prevent duplication from cross-tab sync. */
    private processed_ids = new Set<string>();
    private static readonly MAX_ID_CACHE = 500;

    /** Helper to maintain processed_ids set size bounds and prevent O(N) memory allocation. */
    private purge_id_cache(): void {
        if (this.processed_ids.size >= event_bus_service.MAX_ID_CACHE) {
            const iter = this.processed_ids.values();
            for (let i = 0; i < event_bus_service.MAX_ID_CACHE / 2; i++) {
                const { value, done } = iter.next();
                if (done) break;
                this.processed_ids.delete(value);
            }
        }
    }

    constructor() {
        if (this.channel) {
            this.channel.onmessage = (event) => {
                try {
                    const msg = event.data as telemetry_message;
                    if (!msg || msg.sender_id === this.instance_id) return;

                    switch (msg.topic) {
                        case 'LOG':
                            this.internal_emit(msg.payload as log_entry, false);
                            break;
                        case 'TRACE':
                            this.internal_emit_trace(msg.payload, false);
                            break;
                        case 'PULSE':
                            this.internal_emit_pulse(msg.payload, false);
                            break;
                        case 'SYNC_REQUEST':
                            this.handle_sync_request();
                            break;
                        case 'SYNC_RESPONSE':
                            if (this.pending_sync_response) {
                                clearTimeout(this.pending_sync_response);
                                this.pending_sync_response = null;
                            }
                            this.handle_sync_response(msg.payload);
                            break;
                    }
                } catch (error) {
                    console.error('[event_bus] Error handling BroadcastChannel message:', error);
                }
            };

            setTimeout(() => this.request_sync(), 100);
        }
    }

    private request_sync(): void {
        if (this.channel) {
            try {
                this.channel.postMessage({
                    topic: 'SYNC_REQUEST',
                    payload: null,
                    timestamp: Date.now(),
                    sender_id: this.instance_id
                } as telemetry_message);
            } catch (error) {
                console.error('[event_bus] Failed to send sync request:', error);
            }
        }
    }

    private handle_sync_request(): void {
        if (this.count === 0 && this.processed_ids.size === 0) return;

        if (this.pending_sync_response) {
            clearTimeout(this.pending_sync_response);
        }

        // Apply a randomized delay (50ms - 200ms) to prevent broadcast storms.
        const jitter = (typeof performance !== 'undefined')
            ? (Math.floor(performance.now() * 1000) % 150)
            : (Date.now() % 150);
        this.pending_sync_response = setTimeout(() => {
            if (this.channel) {
                try {
                    this.channel.postMessage({
                        topic: 'SYNC_RESPONSE',
                        payload: {
                            logs: this.get_history().slice(-100),
                        },
                        timestamp: Date.now(),
                        sender_id: this.instance_id
                    } as telemetry_message);
                } catch (error) {
                    console.error('[event_bus] Failed to send sync response:', error);
                }
            }
            this.pending_sync_response = null;
        }, 50 + jitter);
    }

    private handle_sync_response(payload: unknown): void {
        if (!payload || typeof payload !== 'object') return;
        const p = payload as { logs?: log_entry[] };
        if (p.logs && Array.isArray(p.logs)) {
            const logs_to_process = p.logs.slice(-200);
            const new_logs = logs_to_process.filter((log: log_entry) => log && log.id && !this.processed_ids.has(log.id));
            if (new_logs.length === 0) return;

            new_logs.sort((a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime());

            new_logs.forEach((log: log_entry) => {
                this.purge_id_cache();
                this.processed_ids.add(log.id);

                this.ring[this.head] = log;
                this.head = (this.head + 1) % event_bus_service.BUFFER_SIZE;
                if (this.count < event_bus_service.BUFFER_SIZE) this.count++;
            });

            // Deliver notifications asynchronously in chunks to prevent event loop starvation.
            // Notify for all new_logs found during sync since delivery is chunked in batches of 50.
            const logs_to_notify = new_logs;
            const chunk_size = 50;
            let index = 0;
            const deliver_chunk = () => {
                const limit = Math.min(index + chunk_size, logs_to_notify.length);
                for (; index < limit; index++) {
                    const log = logs_to_notify[index];
                    this.listeners.forEach(listener => {
                        try {
                            listener(log);
                        } catch (error) {
                            console.error('[event_bus] Error in listener during sync:', error);
                        }
                    });
                }
                if (index < logs_to_notify.length) {
                    setTimeout(deliver_chunk, 0);
                }
            };
            setTimeout(deliver_chunk, 0);
        }
    }

    /** Subscribe to all future log events. Returns an unsubscribe function. */
    subscribe_logs(listener: Listener): () => void {
        this.listeners.push(listener);
        return () => {
            this.listeners = this.listeners.filter(l => l !== listener);
        };
    }

    /** Subscribe to trace spans. Returns an unsubscribe function. */
    subscribe_trace<T = unknown>(listener: (span: T) => void): () => void {
        const handler = listener as (span: unknown) => void;
        this.trace_listeners.push(handler);
        return () => {
            this.trace_listeners = this.trace_listeners.filter(l => l !== handler);
        };
    }

    /** Subscribe to swarm pulse telemetry. Returns an unsubscribe function. */
    subscribe_pulse<T = unknown>(listener: (pulse: T) => void): () => void {
        const handler = listener as (span: unknown) => void;
        this.pulse_listeners.push(handler);
        return () => {
            this.pulse_listeners = this.pulse_listeners.filter(l => l !== handler);
        };
    }

    /** Emit a log event to all subscribers. `id` and `timestamp` are auto-filled if not provided. */
    emit_log(entry: Omit<log_entry, 'id' | 'timestamp'> & { id?: string; timestamp?: Date }): void {
        const full_entry: log_entry = {
            id: entry.id || ((typeof crypto !== 'undefined' && crypto.randomUUID) ? crypto.randomUUID() : Math.random().toString(36).substring(2)),
            timestamp: entry.timestamp || new Date(),
            source: entry.source,
            text: entry.text,
            severity: entry.severity,
            agent_id: entry.agent_id,
            agent_name: entry.agent_name,
            mission_id: entry.mission_id,
            type_id: entry.type_id,
            metadata: entry.metadata
        };
        this.internal_emit(full_entry, true);
    }

    /** Emit a trace span payload with optional generic typing. */
    emit_trace<T = unknown>(span: T): void {
        this.internal_emit_trace(span, true);
    }

    /** Emit a swarm pulse payload with optional generic typing. */
    emit_pulse<T = unknown>(pulse: T): void {
        this.internal_emit_pulse(pulse, true);
    }

    private internal_emit(full_entry: log_entry, broadcast: boolean): void {
        if (this.processed_ids.has(full_entry.id)) {
            return;
        }

        this.purge_id_cache();
        this.processed_ids.add(full_entry.id);

        this.ring[this.head] = full_entry;
        this.head = (this.head + 1) % event_bus_service.BUFFER_SIZE;
        if (this.count < event_bus_service.BUFFER_SIZE) this.count++;

        this.listeners.forEach(listener => {
            try {
                listener(full_entry);
            } catch (error) {
                console.error('[event_bus] Error in listener:', error);
            }
        });

        // SEC-INTEGRATION: Non-blockingly persist event into IndexedDB telemetry_buffer for time-travel replay
        void telemetry_buffer.append_event(
            full_entry.mission_id || 'global',
            'log',
            full_entry as unknown as Record<string, unknown>
        );

        if (broadcast && this.channel) {
            try {
                this.channel.postMessage({
                    topic: 'LOG',
                    payload: full_entry,
                    timestamp: Date.now(),
                    sender_id: this.instance_id
                } as telemetry_message);
            } catch (error) {
                console.error('[event_bus] Failed to broadcast log:', error);
            }
        }
    }

    private internal_emit_trace(span: unknown, broadcast: boolean): void {
        this.trace_listeners.forEach(listener => {
            try {
                listener(span);
            } catch (error) {
                console.error('[event_bus] Error in trace listener:', error);
            }
        });

        if (span && typeof span === 'object') {
            const span_obj = span as Record<string, unknown>;
            const mission_id = (span_obj.mission_id as string) || (span_obj.missionId as string) || 'global';
            void telemetry_buffer.append_event(mission_id, 'trace', span_obj);
        }

        if (broadcast && this.channel) {
            try {
                this.channel.postMessage({
                    topic: 'TRACE',
                    payload: span,
                    timestamp: Date.now(),
                    sender_id: this.instance_id
                } as telemetry_message);
            } catch (error) {
                console.error('[event_bus] Failed to broadcast trace:', error);
            }
        }
    }

    private internal_emit_pulse(pulse: unknown, broadcast: boolean): void {
        this.pulse_listeners.forEach(listener => {
            try {
                listener(pulse);
            } catch (error) {
                console.error('[event_bus] Error in pulse listener:', error);
            }
        });

        if (pulse && typeof pulse === 'object') {
            const pulse_obj = pulse as Record<string, unknown>;
            const mission_id = (pulse_obj.mission_id as string) || (pulse_obj.missionId as string) || 'global';
            void telemetry_buffer.append_event(mission_id, 'swarm_pulse', pulse_obj);
        }

        if (broadcast && this.channel) {
            try {
                this.channel.postMessage({
                    topic: 'PULSE',
                    payload: pulse,
                    timestamp: Date.now(),
                    sender_id: this.instance_id
                } as telemetry_message);
            } catch (error) {
                console.error('[event_bus] Failed to broadcast pulse:', error);
            }
        }
    }

    /** Returns a chronologically ordered copy of all stored history. */
    get_history(): log_entry[] {
        if (this.count === 0) return [];
        const result: log_entry[] = [];
        const start = this.count < event_bus_service.BUFFER_SIZE
            ? 0
            : this.head;
        for (let i = 0; i < this.count; i++) {
            const idx = (start + i) % event_bus_service.BUFFER_SIZE;
            if (this.ring[idx]) result.push(this.ring[idx]!);
        }
        return result;
    }

    /** Clears event history but keeps all subscribers intact. Safe for `/clear`. */
    clear_history(): void {
        this.ring = new Array(event_bus_service.BUFFER_SIZE).fill(null);
        this.head = 0;
        this.count = 0;
        this.processed_ids.clear();
    }

    /** Full teardown: clears history AND removes all subscribers. Use on unmount. */
    destroy(): void {
        this.clear_history();
        this.listeners = [];
        this.trace_listeners = [];
        this.pulse_listeners = [];
    }
}

/** Singleton instance shared across the entire application. */
export const event_bus = new event_bus_service();
