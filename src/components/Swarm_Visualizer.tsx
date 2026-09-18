/**
 * @docs ARCHITECTURE:Telemetry
 * @docs OPERATIONS_MANUAL:Telemetry
 * 
 * ### AI Assist Note
 * **UI Component**: Real-time Force-Directed Graph for swarm intelligence visualization. 
 * Orchestrates high-speed telemetry ingestion (10Hz binary pulse) and renders neural node identity, halos, and data-flow pulses via custom Canvas operations.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Pulse starvation if `swarm_pulse` events drop, node collision desync due to force engine instability, or memory leak during high-frequency WebGL/Canvas context updates.
 * - **Telemetry Link**: Search for `[Swarm_Visualizer]` or `swarm_pulse` in UI tracing.
 */

import React, { useEffect, useMemo, useRef } from 'react';
import ForceGraph2D, { type ForceGraphMethods } from 'react-force-graph-2d';
import { ExternalLink, Send, X, CheckCircle2 } from 'lucide-react';
import { use_agent_store } from '../stores/agent_store';
import { use_sovereign_store } from '../stores/sovereign_store';
import { THEME_COLORS, GRAPH_THEME } from '../constants/theme';
import { i18n } from '../i18n';
import { tadpole_os_socket } from '../services/socket';
import { type Swarm_Pulse } from '../types';
import { forceCenter, forceManyBody } from 'd3-force';
import { api_request } from '../services/base_api_service';
import { tadpole_os_service } from '../services/tadpoleos_service';

/**
 * Swarm_Visualizer
 * High-performance real-time swarm intelligence visualizer.
 * Integrates with high-speed binary telemetry (MessagePack) at 10Hz.
 */

// Constants moved to src/constants/theme.ts

const NodeStatus = {
    IDLE: 0,
    BUSY: 1,
    ERROR: 2,
    DEGRADED: 3,
    HUB: 4
} as const;

type NodeStatusType = typeof NodeStatus[keyof typeof NodeStatus];

interface GraphNode {
    id: string;
    name: string;
    status: NodeStatusType | number;
    battery: number;
    signal: number;
    progress: number;
    x?: number;
    y?: number;
    vx?: number;
    vy?: number;
}

interface GraphLink {
    source: string | GraphNode;
    target: string | GraphNode;
}

export const Swarm_Visualizer: React.FC<{ is_detached?: boolean, on_detach?: () => void }> = ({ is_detached = false, on_detach }) => {
    const fg_ref = useRef<ForceGraphMethods<GraphNode, GraphLink> | undefined>(undefined);
    const graph_data_ref = useRef<{ nodes: GraphNode[], links: GraphLink[] }>({ nodes: [], links: [] });
    // Trigger React state only when the structural topology changes (node/link count)
    const [graph_metadata, set_graph_metadata] = React.useState<{ nodes: number, links: number }>({ nodes: 0, links: 0 });
    const { agents } = use_agent_store();
    
    // Sovereign Actions for Focus
    const set_selected_agent_id = use_sovereign_store(s => s.set_selected_agent_id);
    const set_scope = use_sovereign_store(s => s.set_scope);
    const set_target_agent = use_sovereign_store(s => s.set_target_agent);

    // Interactive Quick Command Bar State
    const [selected_node, set_selected_node] = React.useState<GraphNode | null>(null);
    const [directive_input, set_directive_input] = React.useState('');
    const [is_dispatching, set_is_dispatching] = React.useState(false);
    const [dispatch_feedback, set_dispatch_feedback] = React.useState<string | null>(null);

    const handle_dispatch_directive = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!selected_node || !directive_input.trim() || is_dispatching) return;

        set_is_dispatching(true);
        set_dispatch_feedback(null);
        try {
            const matched_agent = agents_ref.current.find(a => a.id === selected_node.id);
            const model_id = matched_agent?.model || 'gemini-1.5-flash';
            const provider = tadpole_os_service.resolve_provider(model_id);

            await tadpole_os_service.send_command(
                selected_node.id,
                directive_input.trim(),
                model_id,
                provider,
                undefined,
                matched_agent?.department
            );

            set_dispatch_feedback(`Directive dispatched to ${selected_node.name}`);
            set_directive_input('');
            setTimeout(() => set_dispatch_feedback(null), 3500);
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : 'Unknown error';
            set_dispatch_feedback(`Dispatch failed: ${msg}`);
        } finally {
            set_is_dispatching(false);
        }
    };

    // ### 🧠 State Synchronization: Telemetry Ingestion
    // Subscribes to the high-speed (10Hz) binary telemetry pulse from the backend.
    // Maps the incoming 'SwarmPulse' protocol buffer records into the local D3 
    // Graph representation while preserving node momentum and position clusters.

    // Stable closure to prevent Event Loop thrashing & socket teardowns
    const agents_ref = useRef(agents);
    useEffect(() => { agents_ref.current = agents; }, [agents]);

    // Initial REST bootstrap hydration to prevent blank canvas while socket connects
    useEffect(() => {
        let is_mounted = true;
        const bootstrap_graph = async () => {
            if (graph_data_ref.current.nodes.length > 0) return;
            try {
                const rest_graph = await api_request<{
                    nodes: Array<{ id: string; label: string; type: string; status: string }>;
                    edges: Array<{ source: string; target: string }>;
                }>('/v1/agents/graph');

                if (!is_mounted || !rest_graph || graph_data_ref.current.nodes.length > 0) return;

                const initial_nodes: GraphNode[] = (rest_graph.nodes || []).map(n => {
                    const is_hub = n.type === 'mission';
                    const raw_name = n.label || n.id;
                    const safe_name = raw_name.substring(0, 32).replace(/[^\w\s-]/g, '');
                    return {
                        id: n.id,
                        name: is_hub ? `MISSION_HUB: ${safe_name.substring(0, 24)}` : safe_name,
                        status: is_hub ? NodeStatus.HUB : (n.status === 'active' || n.status === 'busy' ? NodeStatus.BUSY : NodeStatus.IDLE),
                        battery: 100,
                        signal: 100,
                        progress: 0,
                        x: (Math.random() - 0.5) * 50,
                        y: (Math.random() - 0.5) * 50,
                        fx: is_hub ? 0 : undefined,
                        fy: is_hub ? 0 : undefined,
                    };
                });

                const node_ids = new Set(initial_nodes.map(n => n.id));
                const initial_links: GraphLink[] = (rest_graph.edges || [])
                    .filter(e => node_ids.has(e.source) && node_ids.has(e.target))
                    .map(e => ({ source: e.source, target: e.target }));

                graph_data_ref.current = { nodes: initial_nodes, links: initial_links };
                set_graph_metadata({ nodes: initial_nodes.length, links: initial_links.length });
            } catch (err) {
                console.debug('[Swarm_Visualizer] Initial REST bootstrap deferred to socket pulse:', err);
            }
        };

        void bootstrap_graph();
        return () => { is_mounted = false; };
    }, []);

    useEffect(() => {
        // Subscribe to high-speed binary pulses
        const unsubscribe = tadpole_os_socket.subscribe_swarm_pulse((pulse: Swarm_Pulse) => {
            const current = graph_data_ref.current;
            
            // O(1) lookups eliminate synchronous execution blocking
            const agents_map = new Map(agents_ref.current.map(a => [a.id, a]));
            const existing_map = new Map(current.nodes.map(n => [n.id, n]));
            
            // 1. Map Nodes (Mutate in place to avoid React reconciliation overhead)
            const new_nodes = (pulse.nodes || []).map(pulse_node => {
                const agent = agents_map.get(pulse_node.id);
                const existing = existing_map.get(pulse_node.id);
                
                // ### ⚓ Anchor Logic: Mission Hub Stabilization
                // We lock the Mission Hub (status 4) to the origin (0,0) using fixed 
                // coordinate properties (fx, fy). This prevents the entire swarm 
                // from drifting away when specialists are recruited.
                const is_hub = pulse_node.status === NodeStatus.HUB;
                
                // Canvas DoS prevention: sanitize string
                const raw_name = agent?.name || pulse_node.id;
                const safe_name = raw_name.substring(0, 32).replace(/[^\w\s-]/g, '');

                // NaN Poisoning prevention via strict Type Narrowing
                const safe_x = Number.isFinite(existing?.x) ? existing!.x : (Math.random() - 0.5) * 50;
                const safe_y = Number.isFinite(existing?.y) ? existing!.y : (Math.random() - 0.5) * 50;
                
                return {
                    ...pulse_node,
                    name: is_hub ? `MISSION_HUB: ${safe_name.substring(0, 24)}` : safe_name,
                    // Preserve position/velocity from the D3 force engine
                    x: safe_x,
                    y: safe_y,
                    fx: is_hub ? 0 : undefined,
                    fy: is_hub ? 0 : undefined,
                    vx: Number.isFinite(existing?.vx) ? existing!.vx : undefined,
                    vy: Number.isFinite(existing?.vy) ? existing!.vy : undefined
                };
            });

            // 2. Map Links with Strict Mutual Endpoint Validation
            const valid_node_ids = new Set(new_nodes.map(n => n.id));
            const new_links = (pulse.edges || [])
                .filter(edge => valid_node_ids.has(edge.source) && valid_node_ids.has(edge.target))
                .map(edge => ({
                    source: edge.source,
                    target: edge.target
                }));

            // Check if structure or state changed using ID-based Map lookup
            const state_changed = new_nodes.some(node => {
                const prev = existing_map.get(node.id);
                return !prev || node.status !== prev.status || node.battery !== prev.battery;
            });

            const structure_changed = 
                new_nodes.length !== current.nodes.length || 
                new_links.length !== current.links.length ||
                state_changed;

            const first_pulse = current.nodes.length === 0 && new_nodes.length > 0;

            graph_data_ref.current = { nodes: new_nodes, links: new_links };

            if (structure_changed || first_pulse) {
                set_graph_metadata({ nodes: new_nodes.length, links: new_links.length });
            }
        });

        return () => unsubscribe();
    }, []);

    // ### 🛠️ Force Engine Calibration
    // Configures the D3 force simulation to maintain a stable, centered cluster.
    // Prevents "Swarm Drift" by anchoring the global center of mass.
    useEffect(() => {
        if (!fg_ref.current) return;
        
        const fg = fg_ref.current;
        // Add a strong centering force to prevent the swarm from floating away
        fg.d3Force('center', forceCenter(0, 0));
        // Add a stronger charge to keep agents separated but clustered
        fg.d3Force('charge', forceManyBody().strength(-150));
        // Add a link force to pull agents toward the hub
        fg.d3Force('link')?.distance(80).strength(1.5);
        
        // Initial zoom to fit
        setTimeout(() => fg.zoomToFit(400, 50), 500);
    }, []);

    // ### 🎨 High-Performance Rendering: Fast-Path Canvas Pipeline
    // To handle swarms with 100+ agents without saturating the Main Thread, 
    // we bypass the React Reconciliation loop for node aesthetics and 
    // draw directly to the Canvas context using 2D primitives.
    const node_canvas_object = useMemo(() => (node: GraphNode, ctx: CanvasRenderingContext2D, global_scale: number) => {
        ctx.save(); // Isolate context state
        
        const label = node.name;
        const font_size = 11 / global_scale;
        const radius = GRAPH_THEME.NODE_RADIUS;
        
        const safe_x = Number.isFinite(node.x) ? node.x! : 0;
        const safe_y = Number.isFinite(node.y) ? node.y! : 0;

        const status_color = 
            node.status === NodeStatus.BUSY ? THEME_COLORS.BUSY : 
            (node.status === NodeStatus.ERROR ? THEME_COLORS.ERROR : 
            (node.status === NodeStatus.DEGRADED ? THEME_COLORS.DEGRADED : 
            (node.status === NodeStatus.HUB ? THEME_COLORS.GLOW_CYAN : THEME_COLORS.IDLE)));

        // 🛡️ Drawing Layer 1: Glow Halo
        if (node.status === NodeStatus.BUSY || node.status === NodeStatus.ERROR) {
             ctx.beginPath();
             ctx.arc(safe_x, safe_y, radius * 1.6, 0, 2 * Math.PI, false);
             ctx.fillStyle = node.status === NodeStatus.BUSY ? THEME_COLORS.GLOW_CYAN : THEME_COLORS.GLOW_ROSE;
             ctx.fill();
        }

        // 🛡️ Drawing Layer 2: Core Neural Identity
        ctx.beginPath();
        ctx.arc(safe_x, safe_y, radius, 0, 2 * Math.PI, false);
        ctx.fillStyle = status_color;
        ctx.fill();
        
        // 🛡️ Drawing Layer 3: Busy Pulse Animation
        if (node.status === NodeStatus.BUSY) {
             const pulse = (Math.sin(Date.now() / 150) + 1) / 2;
             ctx.beginPath();
             ctx.arc(safe_x, safe_y, radius + (pulse * 3), 0, 2 * Math.PI, false);
             ctx.strokeStyle = `rgba(34, 211, 238, ${0.8 - pulse})`;
             ctx.lineWidth = 1 / global_scale;
             ctx.stroke();
        }

        // 🛡️ Drawing Layer 4: Semantic Labels
        if (global_scale > 2) {
            ctx.font = `${font_size}px ${GRAPH_THEME.LABEL_FONT}`;
            ctx.textAlign = 'center';
            ctx.textBaseline = 'top';
            ctx.fillStyle = 'white';
            ctx.fillText(label, safe_x, safe_y + radius + 4);
        }
        
        // 🛡️ Drawing Layer 5: Resource Telemetry (Bars)
        const bar_w = 12 / global_scale;
        const bar_h = 2 / global_scale;
        if (global_scale > 1.5) {
            ctx.fillStyle = GRAPH_THEME.TELEMETRY_BG;
            ctx.fillRect(safe_x - bar_w/2, safe_y - radius - 5, bar_w, bar_h);
            ctx.fillStyle = node.battery > 20 ? THEME_COLORS.SUCCESS : THEME_COLORS.ERROR;
            ctx.fillRect(safe_x - bar_w/2, safe_y - radius - 5, bar_w * (node.battery / 100), bar_h);
        }

        ctx.restore(); // Restore context state
    }, []);

    // graph_metadata is used to trigger re-renders and provide stable counts
    const current_node_count = graph_metadata.nodes;
    const current_link_count = graph_metadata.links;

    return (
        <div className="w-full h-full relative bg-zinc-950 rounded-xl border border-zinc-900/50 overflow-hidden group">
            <ForceGraph2D
                ref={fg_ref}
                /* eslint-disable-next-line react-hooks/refs */
                graphData={graph_data_ref.current}
                nodeCanvasObject={node_canvas_object}
                nodePointerAreaPaint={(node: GraphNode, color, ctx) => {
                    ctx.fillStyle = color;
                    ctx.beginPath(); ctx.arc(node.x ?? 0, node.y ?? 0, 8, 0, 2 * Math.PI, false); ctx.fill();
                }}
                linkColor={() => THEME_COLORS.NEURAL_GRID}
                linkWidth={GRAPH_THEME.LINK_WIDTH}
                linkDirectionalParticles={2}
                linkDirectionalParticleSpeed={GRAPH_THEME.PARTICLE_SPEED}
                backgroundColor="rgba(0,0,0,0)"
                // ### 🧪 D3 Force Optimization
                // alphaDecay: Higher values (0.02) ensure the graph stabilizes 
                // quickly after node injections.
                // velocityDecay: (0.3) provides 'viscosity' to prevent 
                // erratic jitter during high-frequency pulse updates.
                d3AlphaDecay={0.02}
                d3VelocityDecay={0.6}
                cooldownTicks={100}
                onNodeClick={(node: GraphNode) => {
                    // XSS prevention: sanitize store target
                    const safe_target = (node.name || node.id).replace(/[^\w\s-]/g, '');
                    set_selected_node(node);
                    set_dispatch_feedback(null);

                    if (node.status === NodeStatus.HUB) {
                        set_scope('cluster');
                        set_target_agent(safe_target);
                    } else {
                        // Focus Agent Logs & Scope
                        set_selected_agent_id(node.id);
                        set_scope('agent');
                        set_target_agent(safe_target);
                    }
                    
                    // Center View on Node
                    if (Number.isFinite(node.x) && Number.isFinite(node.y)) {
                        fg_ref.current?.centerAt(node.x!, node.y!, 400);
                        fg_ref.current?.zoom(2.5, 400);
                    }
                }}
            />
            
            {/* HUD Overlay */}
            <div className="absolute top-8 left-8 pointer-events-none select-none">
                 <div className="flex flex-col gap-1.5">
                    <div className="flex items-center gap-3">
                        <div className="w-2.5 h-2.5 rounded-full bg-cyan-500 animate-pulse shadow-[0_0_15px_#06b6d4]" />
                        <h2 className="text-xs font-black text-white uppercase tracking-[0.4em]">{i18n.t('swarm_visualizer.title')}</h2>
                    </div>
                    <div className="flex items-center gap-4 ml-7">
                        <p className="text-[9px] text-zinc-500 font-bold uppercase tracking-wider">{i18n.t('swarm_visualizer.telemetry_10hz')}</p>
                        <div className="w-px h-2 bg-zinc-800" />
                        <p className="text-[9px] text-zinc-500 font-bold uppercase tracking-wider">
                            {i18n.t('swarm_visualizer.nodes_online', { count: current_node_count })}
                        </p>
                        <div className="w-px h-2 bg-zinc-800" />
                        <p className="text-[9px] text-zinc-500 font-bold uppercase tracking-wider">
                            EDGES: {current_link_count}
                        </p>
                    </div>
                </div>
            </div>

            {/* Actionable Quick Command Bar Overlay */}
            {selected_node && (
                <div 
                    data-testid="swarm-quick-command-bar"
                    className="absolute bottom-8 left-8 right-40 bg-zinc-900/90 backdrop-blur-md border border-zinc-800 rounded-xl p-3 shadow-2xl z-20 flex flex-col gap-2 max-w-xl"
                >
                    <div className="flex items-center justify-between">
                        <div className="flex items-center gap-2">
                            <span 
                                className="w-2.5 h-2.5 rounded-full"
                                style={{
                                    backgroundColor: selected_node.status === NodeStatus.HUB 
                                        ? '#a855f7' 
                                        : (selected_node.status === NodeStatus.BUSY ? THEME_COLORS.SECONDARY : THEME_COLORS.SUCCESS)
                                }}
                            />
                            <span className="text-xs font-bold text-white tracking-wide">
                                {selected_node.name}
                            </span>
                            <span className="text-[10px] text-zinc-400 uppercase font-mono px-1.5 py-0.5 bg-zinc-800 rounded">
                                {selected_node.status === NodeStatus.HUB ? 'MISSION HUB' : `BATTERY ${selected_node.battery}%`}
                            </span>
                        </div>
                        <button
                            data-testid="close-quick-command"
                            onClick={() => {
                                set_selected_node(null);
                                set_dispatch_feedback(null);
                            }}
                            className="text-zinc-500 hover:text-white p-1 transition-colors"
                            aria-label="Close command bar"
                        >
                            <X size={14} />
                        </button>
                    </div>

                    {selected_node.status === NodeStatus.HUB ? (
                        <div className="flex items-center justify-between pt-1">
                            <p className="text-[11px] text-zinc-400">
                                Active swarm coordination hub. Inspect cluster telemetry and directives in the Missions console.
                            </p>
                            <a
                                href={`/missions?id=${encodeURIComponent(selected_node.id)}`}
                                className="px-3 py-1.5 bg-cyan-600/30 border border-cyan-500/50 hover:bg-cyan-500/30 text-cyan-300 text-xs font-bold rounded-lg transition-colors flex items-center gap-1.5"
                            >
                                Inspect Mission
                            </a>
                        </div>
                    ) : (
                        <form onSubmit={handle_dispatch_directive} className="flex gap-2">
                            <input
                                data-testid="quick-directive-input"
                                type="text"
                                value={directive_input}
                                onChange={(e) => set_directive_input(e.target.value)}
                                placeholder="Direct agent (e.g. analyze telemetry, refactor module)..."
                                className="flex-1 bg-zinc-950/80 border border-zinc-700/60 rounded-lg px-3 py-1.5 text-xs text-white placeholder-zinc-500 focus:outline-none focus:border-cyan-500 transition-colors"
                                disabled={is_dispatching}
                            />
                            <button
                                data-testid="submit-quick-directive"
                                type="submit"
                                disabled={!directive_input.trim() || is_dispatching}
                                className="px-3 py-1.5 bg-cyan-600 hover:bg-cyan-500 disabled:bg-zinc-800 text-white disabled:text-zinc-500 text-xs font-bold rounded-lg transition-colors flex items-center gap-1.5 shadow-lg shadow-cyan-950/50"
                            >
                                <Send size={12} />
                                <span>{is_dispatching ? 'Dispatching...' : 'Dispatch'}</span>
                            </button>
                        </form>
                    )}

                    {dispatch_feedback && (
                        <div className="text-[11px] text-cyan-400 font-mono tracking-tight flex items-center gap-1.5">
                            <CheckCircle2 size={12} className="text-cyan-400" />
                            <span>{dispatch_feedback}</span>
                        </div>
                    )}
                </div>
            )}

            {/* Detach Window Button - Hidden if already detached */}
            {!is_detached && (
                <div className="absolute top-8 right-8 flex gap-2">
                    <button 
                        onClick={() => {
                            if (on_detach) {
                                on_detach();
                            } else {
                                window.open(window.location.origin + '/detached/swarm-pulse', 'SwarmPulse', 'width=1000,height=800');
                            }
                        }}
                        className="p-2.5 bg-zinc-900/40 backdrop-blur-md border border-zinc-800 rounded-xl text-zinc-400 hover:bg-zinc-800 hover:text-white transition-all group/detach"
                        title={i18n.t('swarm_visualizer.detach_tooltip')}
                    >
                        <ExternalLink size={16} className="group-hover/detach:scale-110 transition-transform" />
                    </button>
                </div>
            )}

            {/* Bottom Controls Panel */}
            <div className="absolute bottom-8 right-8 flex gap-2">
                 <button 
                    onClick={() => fg_ref.current?.zoomToFit(400, 50)}
                    className="px-4 py-2 bg-zinc-900/40 backdrop-blur-md border border-zinc-800 rounded-xl text-[9px] font-bold text-zinc-400 uppercase tracking-widest hover:bg-zinc-800 hover:text-white transition-all overflow-hidden group/btn"
                 >
                    <span className="relative z-10">{i18n.t('swarm_visualizer.recenter_swarm')}</span>
                 </button>
            </div>
        </div>
    );
};


// Metadata: [Swarm_Visualizer]
