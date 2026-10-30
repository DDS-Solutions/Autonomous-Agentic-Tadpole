/**
 * @docs ARCHITECTURE:Logic
 * 
 * ### AI Assist Note
 * **Networking**: Manages the real-time WebSocket telemetry and relay between the UI and the backend. 
 * Orchestrates event normalization for the `event_bus` and `sovereign_store`.
 * 
 * ### @aiContext
 * - **Dependencies**: `event_bus`, `use_settings_store` (URL/Auth), `@msgpack/msgpack` (Binary Pulse).
 * - **Side Effects**: asynchronous store hydration (`agent_store`, `sovereign_store`, `trace_store`) via reactive socket messages.
 * - **Mocking**: Mocking the `WebSocket` global is required for connection lifecycle tests.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Connection timeouts (MAX_RETRIES reached), invalid API key/URL format causing 403/404, or MessagePack decoding errors for binary pulses.
 * - **Telemetry Link**: Search for `[socket]` or `bearer.tadpole` in browser/proxy logs.
 */

import { event_bus } from './event_bus';
import { get_settings, use_settings_store } from '../stores/settings_store';
import type { Message_Part, Swarm_Pulse } from '../types';
import { decode } from '@msgpack/msgpack';
import { sanitize_payload } from '../utils/security_utils';

/** Payload for agent update/status events from the WebSocket. */
export interface Agent_Update_Event {
    type: 'agent:create' | 'agent:update' | 'agent:status' | 'engine:ui_invalidate';
    agent_id?: string;
    agentId?: string;
    status?: string;
    data?: Record<string, unknown> | Partial<import('../types').Agent>;
    resource?: 'agents' | 'missions' | 'system';
    id?: string;
    source_id?: string;
}

/** Payload for engine health broadcast events. */
export interface Engine_Health_Event {
    type: 'engine:health';
    uptime?: number;
    agent_count?: number;
    active_missions?: number;
    active_agents?: number;
    max_depth?: number;
    tpm?: number;
    recruit_count?: number;
    activeProcesses?: number;
    cpu?: number;
    memory?: number;
    memory_total?: number;
    latency?: number;
    [key: string]: unknown;
}

/** Payload for inter-cluster handoff events. */
export interface Handoff_Event {
    type: 'agent:handoff';
    from_cluster: string;
    to_cluster: string;
    agent_id: string;
    payload?: Record<string, unknown>;
}

/** Payload for MCP tool pulse events. */
export interface Mcp_Pulse_Event {
    tool: string;
    status: 'success' | 'error';
    latency: number;
}

/** 
 * SEC-Telemetry: Structured headers for binary pulse decoding.
 */
export const Telemetry_Packet_Header = {
    AUDIO_STREAM: 0x01,
    SWARM_PULSE: 0x02,
    NEURAL_TRACE: 0x03, 
    METRICS_SNAPSHOT: 0x04
} as const;

export type Telemetry_Packet_Header = typeof Telemetry_Packet_Header[keyof typeof Telemetry_Packet_Header];

/** Maximum number of reconnect attempts before giving up. */
export const MAX_RETRIES = 10;
/** Initial backoff delay in ms. */
export const INITIAL_BACKOFF = 2000;
/** Maximum backoff delay in ms. */
export const MAX_BACKOFF = 30000;

/** Connection states for the socket. */
export type Connection_State = 'connecting' | 'connected' | 'disconnected' | 'reconnecting' | 'error';

type State_Listener = (state: Connection_State) => void;

type Store_Bindings = {
    use_agent_registry_store: typeof import('../stores/agent_store').use_agent_registry_store;
    use_sovereign_store: typeof import('../stores/sovereign_store').use_sovereign_store;
    use_trace_store: typeof import('../stores/trace_store').use_trace_store;
};

/**
 * Normalizes raw MessagePack swarm pulse data (tuple array or keyed object) into a typed `Swarm_Pulse`.
 */
export function normalize_swarm_pulse(data: Record<string, unknown>): Swarm_Pulse {
    const is_tuple = Array.isArray(data);
    const nodes_raw = is_tuple ? (data as unknown[])[1] : data.nodes;
    const edges_raw = is_tuple ? (data as unknown[])[2] : data.edges;

    return {
        timestamp: Number(is_tuple ? (data as unknown[])[0] : data.timestamp) || 0,
        nodes: ((nodes_raw || []) as unknown[]).map((n) => Array.isArray(n)
            ? {
                id: String((n as unknown[])[0]),
                x: Number((n as unknown[])[1]),
                y: Number((n as unknown[])[2]),
                status: Number((n as unknown[])[3]),
                battery: Number((n as unknown[])[4]),
                signal: Number((n as unknown[])[5]),
                progress: Number((n as unknown[])[6])
            }
            : n as Swarm_Pulse['nodes'][0]),
        edges: ((edges_raw || []) as unknown[]).map((e) => Array.isArray(e)
            ? { source: String((e as unknown[])[0]), target: String((e as unknown[])[1]) }
            : e as Swarm_Pulse['edges'][0])
    };
}

/**
 * Tadpole_OS_Socket_Client
 * WebSocket client for streaming real-time logs from TadpoleOS.
 * Reads the connection URL from the centralized settings store.
 * Refactored for strict snake_case compliance for backend parity.
 */
class Tadpole_OS_Socket_Client {
    private socket: WebSocket | null = null;
    private reconnect_timer: ReturnType<typeof setTimeout> | null = null;
    private is_explicitly_closed = false;
    private retry_count = 0;
    private last_url = '';
    private last_key = '';
    private store_bindings_promise: Promise<Store_Bindings> | null = null;

    // Agent name cache — O(1) lookups instead of O(n) per message
    private agent_name_cache: Map<string, string> = new Map();
    private cache_stale = true;

    // State Management
    private state: Connection_State = 'disconnected';
    private state_listeners: State_Listener[] = [];

    // Topic Listener Registries (arrays preserved for test suite state reset compatibility)
    private agent_update_listeners: ((update: Agent_Update_Event) => void)[] = [];
    private health_listeners: ((health: Engine_Health_Event) => void)[] = [];
    private handoff_listeners: ((handoff: Handoff_Event) => void)[] = [];
    private mcp_pulse_listeners: ((pulse: Mcp_Pulse_Event) => void)[] = [];
    private audio_stream_listeners: ((chunk: ArrayBuffer) => void)[] = [];
    private pulse_listeners: ((pulse: Swarm_Pulse) => void)[] = [];
    private custom_event_listeners: ((event: Record<string, unknown>) => void)[] = [];

    /**
     * Registers a listener in the target array and returns an idempotent unsubscribe callback.
     */
    private register_listener<T>(
        list: ((payload: T) => void)[],
        listener: (payload: T) => void
    ): () => void {
        list.push(listener);
        return () => {
            const idx = list.indexOf(listener);
            if (idx !== -1) {
                list.splice(idx, 1);
            }
        };
    }

    /** Subscribe to connection state changes. */
    subscribe_status(listener: State_Listener): () => void {
        const unsubscribe = this.register_listener(this.state_listeners, listener);
        listener(this.state); // Immediate update
        return unsubscribe;
    }

    constructor() {
        // REACTIVE RECONNECTION: Watch for infrastructure changes
        const initial = get_settings();
        this.last_url = initial.tadpole_os_url;
        this.last_key = initial.tadpole_os_api_key;

        use_settings_store.subscribe((state) => {
            const { tadpole_os_url, tadpole_os_api_key } = state.settings;

            if (tadpole_os_url !== this.last_url || tadpole_os_api_key !== this.last_key) {
                this.last_url = tadpole_os_url;
                this.last_key = tadpole_os_api_key;

                if (this.is_explicitly_closed) return;

                console.debug(`[Tadpole_OS] Infrastructure settings changed. Reconnecting...`);
                this.disconnect();
                // Reset closure flag so connect() can proceed
                this.is_explicitly_closed = false;
                this.connect();
            }
        });
    }

    private set_state(new_state: Connection_State): void {
        if (this.state !== new_state) {
            this.state = new_state;
            this.state_listeners.forEach(l => l(new_state));
        }
    }

    subscribe_agent_updates(listener: (update: Agent_Update_Event) => void): () => void {
        return this.register_listener(this.agent_update_listeners, listener);
    }

    subscribe_health(listener: (health: Engine_Health_Event) => void): () => void {
        return this.register_listener(this.health_listeners, listener);
    }

    subscribe_handoff(listener: (handoff: Handoff_Event) => void): () => void {
        return this.register_listener(this.handoff_listeners, listener);
    }

    subscribe_pulse(listener: (pulse: Mcp_Pulse_Event) => void): () => void {
        return this.register_listener(this.mcp_pulse_listeners, listener);
    }

    subscribe_audio_stream(listener: (chunk: ArrayBuffer) => void): () => void {
        return this.register_listener(this.audio_stream_listeners, listener);
    }

    subscribe_swarm_pulse(listener: (pulse: Swarm_Pulse) => void): () => void {
        return this.register_listener(this.pulse_listeners, listener);
    }

    subscribe_custom_event(listener: (event: Record<string, unknown>) => void): () => void {
        return this.register_listener(this.custom_event_listeners, listener);
    }

    send_event(event: Record<string, unknown>): void {
        if (this.socket?.readyState === WebSocket.OPEN) {
            this.socket.send(JSON.stringify(event));
        }
    }

    get_connection_state(): Connection_State {
        return this.state;
    }

    private get_store_bindings(): Promise<Store_Bindings> {
        if (!this.store_bindings_promise) {
            this.store_bindings_promise = Promise.all([
                import('../stores/agent_store'),
                import('../stores/sovereign_store'),
                import('../stores/trace_store')
            ]).then(([agent_store, sovereign_store, trace_store]) => ({
                use_agent_registry_store: agent_store.use_agent_registry_store,
                use_sovereign_store: sovereign_store.use_sovereign_store,
                use_trace_store: trace_store.use_trace_store
            }));
        }
        return this.store_bindings_promise;
    }

    /**
     * Probes `/v1/engine/health` on abnormal WebSocket closure (1006) to halt reconnection
     * loops if the backend explicitly rejects the current Bearer token (401/403).
     */
    private probe_auth_on_abnormal_close(raw_http_url: string, token: string): void {
        const health_url = `${raw_http_url || 'http://localhost:8000'}`.trim().replace(/\/$/, '') + '/v1/engine/health';
        fetch(health_url, {
            headers: { 'Authorization': `Bearer ${token}` }
        }).then(res => {
            if (res.status === 401 || res.status === 403) {
                console.warn('[Tadpole_OS] Engine returned 401/403. Halting WebSocket reconnect loop.');
                event_bus.emit_log({
                    source: 'System',
                    text: 'WebSocket authentication failed. Please update NEURAL_TOKEN in Settings.',
                    severity: 'error'
                });
                return;
            }
            if (!this.is_explicitly_closed) {
                this.schedule_reconnect();
            }
        }).catch(() => {
            if (!this.is_explicitly_closed) {
                this.schedule_reconnect();
            }
        });
    }

    /**
     * Dispatches binary WebSocket frames by inspecting the 1-byte `Telemetry_Packet_Header`.
     */
    private handle_binary_frame(buffer: ArrayBuffer): void {
        if (buffer.byteLength === 0) return;

        const view = new Uint8Array(buffer);
        const header = view[0] as Telemetry_Packet_Header;
        const payload = buffer.slice(1);

        switch (header) {
            case Telemetry_Packet_Header.AUDIO_STREAM:
                this.audio_stream_listeners.forEach(l => l(payload));
                break;

            case Telemetry_Packet_Header.SWARM_PULSE:
                this.handle_swarm_pulse(payload);
                break;

            default:
                console.warn(`[Tadpole_OS] Received unknown telemetry header: 0x${header.toString(16)}`);
        }
    }

    /** Opens the WebSocket connection and begins listening for events. */
    connect(): void {
        // Guard: Don't connect if already connecting or connected
        if (this.socket || this.reconnect_timer || this.state === 'connected') {
            return;
        }

        this.is_explicitly_closed = false;
        this.set_state('connecting');

        // Get URL from centralized settings, converting http/https to ws/wss
        const { tadpole_os_url, tadpole_os_api_key } = get_settings();
        const token = tadpole_os_api_key.trim();
        if (!token) {
            this.set_state('error');
            event_bus.emit_log({
                source: 'System',
                text: 'Tadpole_OS: Missing API token. Add your NEURAL_TOKEN in Settings before connecting telemetry.',
                severity: 'error'
            });
            return;
        }
        // Remove trailing slash if present, then replace http with ws
        const base_url = (tadpole_os_url || 'http://localhost:8000').trim().replace(/\/$/, '').replace(/^http/, 'ws');
        // SEC-01: Token sent via Sec-WebSocket-Protocol header, not URL query params.
        const ws_url = `${base_url}/v1/engine/ws`;

        try {
            const ws = new WebSocket(ws_url, [`bearer.${token}`, 'tadpole-pulse-v1']);
            ws.binaryType = 'arraybuffer';
            this.socket = ws;

            ws.onopen = () => {
                if (this.socket !== ws) return; // Guard against stale connections
                this.retry_count = 0; // Reset on successful connection
                this.set_state('connected');

                event_bus.emit_log({
                    source: 'System',
                    text: 'Connected to TadpoleOS Log Stream.',
                    severity: 'success'
                });
            };

            this.socket.onmessage = (event) => {
                if (event.data instanceof ArrayBuffer) {
                    this.handle_binary_frame(event.data);
                    return;
                }

                try {
                    const data = JSON.parse(event.data);
                    void this.handle_socket_message(data).catch((err) => {
                        console.error('[Tadpole_OS] Error processing WebSocket packet:', err);
                    });
                } catch (e) {
                    console.debug('[Tadpole_OS] Dropped malformed packet:', e);
                }
            };

            ws.onclose = (event?: CloseEvent) => {
                if (this.socket === ws) {
                    this.socket = null;
                    this.set_state('disconnected');
                    if (event?.code === 1008) {
                        console.warn('[Tadpole_OS] WebSocket closed with code 1008 (Policy Violation / Auth Failed). Halting reconnect.');
                        event_bus.emit_log({
                            source: 'System',
                            text: 'WebSocket rejected authorization (code 1008). Verify NEURAL_TOKEN in Settings.',
                            severity: 'error'
                        });
                        return;
                    }

                    // On abnormal closure (code 1006), verify if backend rejected auth before scheduling reconnect
                    if (event?.code === 1006 && typeof fetch !== 'undefined') {
                        this.probe_auth_on_abnormal_close(tadpole_os_url, token);
                        return;
                    }

                    if (!this.is_explicitly_closed) {
                        this.schedule_reconnect();
                    }
                }
            };

            ws.onerror = () => {
                if (this.socket === ws) {
                    this.set_state('disconnected');
                    ws.close();
                }
            };

        } catch (error) {
            console.error('[Tadpole_OS] Connection failed:', error);
            this.set_state('disconnected');
            this.schedule_reconnect();
        }
    }

    private handle_swarm_pulse(payload: ArrayBuffer): void {
        try {
            const raw = decode(payload) as Record<string, unknown>;
            const pulse = normalize_swarm_pulse(raw);
            this.pulse_listeners.forEach(l => l(pulse));
        } catch (e) {
            console.error('[Tadpole_OS] Failed to decode swarm pulse:', e);
        }
    }

    /**
     * Lazily refreshes the internal `agent_name_cache` when marked stale.
     */
    private refresh_agent_name_cache_if_stale(
        use_agent_registry_store: Store_Bindings['use_agent_registry_store']
    ): void {
        if (!this.cache_stale) return;

        const raw_agents = use_agent_registry_store.getState().agents;
        const agents = Array.isArray(raw_agents) ? raw_agents : [];
        this.agent_name_cache.clear();
        for (const a of agents) {
            if (a && a.id && a.name) {
                this.agent_name_cache.set(a.id, a.name);
            }
        }
        this.cache_stale = false;
    }

    /**
     * Resolves or generates a correlation ID for a SovereignChat message.
     */
    private resolve_message_id(data: Record<string, unknown>, prefer_id_over_request = false): string {
        const candidate = prefer_id_over_request
            ? (data.message_id || data.messageId || data.id)
            : (data.message_id || data.messageId || data.request_id || data.requestId);

        if (candidate) return String(candidate);
        return (typeof crypto !== 'undefined' && crypto.randomUUID)
            ? crypto.randomUUID()
            : Date.now().toString();
    }

    /**
     * Appends a message part to an existing SovereignChat message or creates a new entry.
     */
    private bridge_to_sovereign_chat(
        sovereign_store: ReturnType<Store_Bindings['use_sovereign_store']['getState']>,
        params: {
            message_id: string;
            sender_id: string;
            sender_name: string;
            agent_id: string;
            target_node?: string;
            text: string;
            part: Message_Part;
        }
    ): void {
        const did_update = sovereign_store.append_message_part(params.message_id, params.part);
        if (!did_update) {
            sovereign_store.add_message({
                id: params.message_id,
                sender_id: params.sender_id,
                sender_name: params.sender_name,
                agent_id: params.agent_id,
                ...(params.target_node ? { target_node: params.target_node } : {}),
                text: params.text,
                parts: [params.part],
                scope: sovereign_store.active_scope
            });
        }
    }

    private async handle_socket_message(raw_data: Record<string, unknown>): Promise<void> {
        // SEC-02: Sanitize all incoming telemetry data before it reaches the event bus or stores.
        const data = sanitize_payload(raw_data);
        const { use_agent_registry_store, use_sovereign_store, use_trace_store } = await this.get_store_bindings();
        const sovereign_store = use_sovereign_store.getState();

        this.refresh_agent_name_cache_if_stale(use_agent_registry_store);

        if (data.type === 'log' || data.type === 'thought') {
            const agent_id = data.agent_id as string;
            const agent_name = this.agent_name_cache.get(agent_id) || (data.agent_name as string);

            event_bus.emit_log({
                id: (data.id || data.request_id || data.requestId) as string,
                source: agent_id ? 'Agent' : 'System',
                agent_id,
                agent_name,
                text: (data.text || data.message || JSON.stringify(data)) as string,
                severity: (data.level === 'error' ? 'error' : 'info'),
                metadata: data as Record<string, unknown>
            });

            // Bridge agent logs/thoughts to SovereignChat
            if (agent_id && (data.type === 'thought' || (data.type === 'log' && data.level !== 'debug'))) {
                const resolved_name = agent_name || agent_id;
                const message_id = this.resolve_message_id(data, false);
                const content = (data.message || data.text || '') as string;

                const new_part: Message_Part = data.type === 'thought'
                    ? { type: 'thought', content, status: 'done' }
                    : { type: 'text', content };

                this.bridge_to_sovereign_chat(sovereign_store, {
                    message_id,
                    sender_id: agent_id,
                    sender_name: resolved_name,
                    agent_id,
                    text: content,
                    part: new_part
                });
            }
        } else if (data.type === 'agent:message') {
            const agent_id = data.agent_id as string;
            const agent_name = this.agent_name_cache.get(agent_id) || (data.agent_name as string);

            event_bus.emit_log({
                id: (data.id || data.message_id || data.messageId) as string,
                source: 'Agent',
                agent_id,
                agent_name,
                text: (data.text || 'Mission action complete.') as string,
                severity: 'info',
                metadata: data as Record<string, unknown>
            });

            const resolved_name = agent_name || agent_id || 'Agent';
            const message_id = this.resolve_message_id(data, true);
            const content = (data.text || data.message || 'Mission action complete.') as string;
            const new_part: Message_Part = { type: 'text' as const, content };

            this.bridge_to_sovereign_chat(sovereign_store, {
                message_id,
                sender_id: agent_id || 'system',
                sender_name: resolved_name,
                agent_id,
                target_node: resolved_name,
                text: content,
                part: new_part
            });
        } else if (data.type === 'agent:create' || data.type === 'agent:update' || data.type === 'agent:status' || data.type === 'engine:ui_invalidate') {
            const normalized_agent_id = (data.agent_id ?? data.agentId ?? data.id) as string;
            const normalized_data = data.type === 'agent:status'
                ? { ...data, type: 'agent:update', agent_id: normalized_agent_id, data: { status: data.status } }
                : { ...data, agent_id: normalized_agent_id };

            if (data.type === 'engine:ui_invalidate') {
                event_bus.emit_log({
                    source: 'System',
                    text: `UI Invalidated: ${data.resource}${data.id ? ` (#${data.id})` : ''}`,
                    severity: 'info'
                });
            }

            this.agent_update_listeners.forEach(l => l(normalized_data as unknown as Agent_Update_Event));
            // Invalidate agent name cache on any agent update
            this.cache_stale = true;
        } else if (data.type === 'engine:health') {
            this.health_listeners.forEach(l => l(data as unknown as Engine_Health_Event));
        } else if (data.type === 'agent:handoff') {
            this.handoff_listeners.forEach(l => l(data as unknown as Handoff_Event));
        } else if (data.type === 'engine:mcp_pulse') {
            this.mcp_pulse_listeners.forEach(l => l(data as unknown as Mcp_Pulse_Event));
        } else if (data.type === 'trace:span') {
            use_trace_store.getState().add_span(data.span as import('../stores/trace_store').Trace_Span);
        } else if (data.type === 'trace:event') {
            const level = String(data.level || 'INFO').toUpperCase();
            if (level === 'ERROR' || level === 'WARN') {
                const attrs = (data.attributes && typeof data.attributes === 'object') ? (data.attributes as Record<string, unknown>) : {};
                const message = (attrs.message || data.message || `[${data.target || 'Trace'}] Alert`) as string;
                event_bus.emit_log({
                    source: (data.target as string) || 'Kernel',
                    text: message,
                    severity: level === 'ERROR' ? 'error' : 'warning'
                });
            }
        } else if (data.type === 'trace:span_update') {
            use_trace_store.getState().update_span((data.span_id || data.spanId) as string, data.update as Partial<import('../stores/trace_store').Trace_Span>);
        } else if (data.type === 'engine:scheduled_job_complete') {
            event_bus.emit_log({
                source: 'System',
                text: `Scheduled Job '${data.job_name}' completed. Cost: $${(data.cost_usd as number || 0).toFixed(4)}`,
                severity: data.status === 'failed' ? 'error' : 'success'
            });
        } else {
            // Handle custom/specialized events
            this.custom_event_listeners.forEach(l => l(data));
        }
    }

    private schedule_reconnect(): void {
        if (this.retry_count >= MAX_RETRIES) {
            event_bus.emit_log({
                source: 'System',
                text: `Tadpole_OS: Connection failed after ${MAX_RETRIES} attempts. Verify URL in Settings.`,
                severity: 'error'
            });
            return;
        }

        const delay = Math.min(INITIAL_BACKOFF * Math.pow(2, this.retry_count), MAX_BACKOFF);
        this.retry_count++;

        this.set_state('reconnecting');
        if (this.reconnect_timer) clearTimeout(this.reconnect_timer);
        this.reconnect_timer = setTimeout(() => {
            this.reconnect_timer = null;
            this.connect();
        }, delay);
    }

    disconnect(): void {
        this.is_explicitly_closed = true;
        this.retry_count = 0;
        this.set_state('disconnected');
        if (this.reconnect_timer) {
            clearTimeout(this.reconnect_timer);
            this.reconnect_timer = null;
        }
        const active_socket = this.socket;
        this.socket = null;
        active_socket?.close();
    }
}

/** 
 * Lazy constructor to avoid side-effects at import time.
 */
let instance: Tadpole_OS_Socket_Client | null = null;
export const get_tadpole_os_socket = () => {
    if (!instance) {
        instance = new Tadpole_OS_Socket_Client();
    }
    return instance;
};

/** 
 * Type-safe Proxy for the Tadpole_OS_Socket_Client.
 */
export const tadpole_os_socket = new Proxy({} as Tadpole_OS_Socket_Client, {
    get(_, prop) {
        const inst = get_tadpole_os_socket();
        const value = inst[prop as keyof Tadpole_OS_Socket_Client];
        if (typeof value === 'function') {
            return (value as (...args: unknown[]) => unknown).bind(inst);
        }
        return value;
    },
    set(_, prop, value) {
        const inst = get_tadpole_os_socket();
        const key = prop as keyof Tadpole_OS_Socket_Client;
        // Type-safe property setting
        (inst[key] as typeof value) = value;
        return true;
    }
});

// Metadata: [socket]
