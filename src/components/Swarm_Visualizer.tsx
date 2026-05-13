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
import { ExternalLink } from 'lucide-react';
import { use_agent_store } from '../stores/agent_store';
import { use_sovereign_store } from '../stores/sovereign_store';
import { THEME_COLORS, GRAPH_THEME } from '../constants/theme';
import { i18n } from '../i18n';
import { tadpole_os_socket } from '../services/socket';
import { type Swarm_Pulse } from '../types';
import { forceCenter, forceManyBody } from 'd3-force';

/**
 * Swarm_Visualizer
 * High-performance real-time swarm intelligence visualizer.
 * Integrates with high-speed binary telemetry (MessagePack) at 10Hz.
 */

// Constants moved to src/constants/theme.ts

export const NodeStatus = {
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

    // ### 🧠 State Synchronization: Telemetry Ingestion
    // Subscribes to the high-speed (10Hz) binary telemetry pulse from the backend.
    // Maps the incoming 'SwarmPulse' protocol buffer records into the local D3 
    // Graph representation while preserving node momentum and position clusters.

    // Stable closure to prevent Event Loop thrashing & socket teardowns
    const agents_ref = useRef(agents);
    useEffect(() => { agents_ref.current = agents; }, [agents]);

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
                    name: is_hub ? `MISSION_HUB: ${safe_name.substring(0, 8)}` : safe_name,
                    // Preserve position/velocity from the D3 force engine
                    x: safe_x,
                    y: safe_y,
                    fx: is_hub ? 0 : undefined,
                    fy: is_hub ? 0 : undefined,
                    vx: Number.isFinite(existing?.vx) ? existing!.vx : undefined,
                    vy: Number.isFinite(existing?.vy) ? existing!.vy : undefined
                };
            });

            // 2. Map Links
            const new_links = (pulse.edges || []).map(edge => ({
                source: edge.source,
                target: edge.target
            }));

            // Check if structure or state changed (additions/deletions or status/battery shifts)
            const state_changed = new_nodes.some((node, i) => {
                const prev = current.nodes[i];
                return !prev || node.id !== prev.id || node.status !== prev.status || node.battery !== prev.battery;
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

                    // Focus Agent Logs & Scope
                    set_selected_agent_id(node.id);
                    set_scope('agent');
                    set_target_agent(safe_target);
                    
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
                            {/* eslint-disable-next-line react-hooks/refs */}
                            {i18n.t('swarm_visualizer.nodes_online', { count: current_node_count })}
                        </p>
                        <div className="w-px h-2 bg-zinc-800" />
                        <p className="text-[9px] text-zinc-500 font-bold uppercase tracking-wider">
                            EDGES: {current_link_count}
                        </p>
                    </div>
                </div>
            </div>

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
