/**
 * @docs ARCHITECTURE:Intelligence
 * @docs OPERATIONS_MANUAL:Intelligence
 * 
 * ### AI Assist Note
 * **UI Component**: High-fidelity Code Intelligence Knowledge Graph.
 * Visualizes symbol interdependencies extracted from the codebase.
 * Supports "Blast Radius" analysis to predict impact of changes.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Graph stabilization failure (jitter), data ingestion timeout,
 *   or canvas context loss during rapid resizing.
 * - **Telemetry Link**: Search for `[KnowledgeGraph]` in UI tracing.
 */

import React, { useEffect, useMemo, useRef, useState } from 'react';
import ForceGraph2D, { type ForceGraphMethods } from 'react-force-graph-2d';
import { Search, Info, Zap, Target, RefreshCw } from 'lucide-react';
import { intelligence_api_service, type CodeGraphData } from '../../services/intelligence_api_service';
import { THEME_COLORS, GRAPH_THEME } from '../../constants/theme';
import type { SymbolNode } from '../../types/schemas';

interface ExtendedGraphNode extends SymbolNode {
    id: string; // Map path:name to id for force-graph
    x?: number;
    y?: number;
    vx?: number;
    vy?: number;
    is_affected?: boolean;
}

interface GraphLink {
    source: string;
    target: string;
}

export const KnowledgeGraph: React.FC = () => {
    const fg_ref = useRef<ForceGraphMethods<ExtendedGraphNode, GraphLink> | undefined>(undefined);
    const [data, set_data] = useState<CodeGraphData | null>(null);
    const [loading, set_loading] = useState(true);
    const [selected_node, set_selected_node] = useState<ExtendedGraphNode | null>(null);
    const [affected_nodes, set_affected_nodes] = useState<Set<string>>(new Set());
    const [hover_node, set_hover_node] = useState<ExtendedGraphNode | null>(null);

    const fetch_graph = async () => {
        set_loading(true);
        try {
            const graph = await intelligence_api_service.get_graph();
            set_data(graph);
        } catch (err) {
            console.error('[KnowledgeGraph] Failed to fetch graph:', err);
        } finally {
            set_loading(false);
        }
    };

    useEffect(() => {
        fetch_graph();
    }, []);

    // Transform data for force-graph
    const graph_data = useMemo(() => {
        if (!data) return { nodes: [], links: [] };
        
        const nodes: ExtendedGraphNode[] = data.nodes.map(node => ({
            ...node,
            id: `${node.path}:${node.name}`,
            is_affected: affected_nodes.has(`${node.path}:${node.name}`)
        }));

        return { nodes, links: data.links };
    }, [data, affected_nodes]);

    const handle_node_click = async (node: ExtendedGraphNode) => {
        set_selected_node(node);
        try {
            const affected = await intelligence_api_service.get_blast_radius(node.name, node.path);
            const affected_ids = new Set(affected.map(n => `${n.path}:${n.name}`));
            set_affected_nodes(affected_ids);
            
            // Center and zoom
            if (fg_ref.current) {
                fg_ref.current.centerAt(node.x, node.y, 1000);
                fg_ref.current.zoom(2.5, 1000);
            }
        } catch (err) {
            console.error('[KnowledgeGraph] Blast radius failed:', err);
        }
    };

    // ### 🎨 High-Performance Rendering: Intelligence Aesthetic
    const node_canvas_object = useMemo(() => (node: ExtendedGraphNode, ctx: CanvasRenderingContext2D, global_scale: number) => {
        ctx.save();
        
        const label = node.name;
        const font_size = 10 / global_scale;
        const radius = GRAPH_THEME.NODE_RADIUS * (node.is_affected ? 1.5 : 1);
        
        const x = node.x ?? 0;
        const y = node.y ?? 0;

        // Color based on Symbol Kind
        let kind_color = THEME_COLORS.IDLE;
        if (node.kind === 'Function') kind_color = THEME_COLORS.BUSY;
        if (node.kind === 'Struct' || node.kind === 'Class') kind_color = THEME_COLORS.SUCCESS;
        if (node.kind === 'Trait' || node.kind === 'Interface') kind_color = THEME_COLORS.DEGRADED;
        if (node.kind === 'Enum') kind_color = '#a855f7'; // Purple

        // Override if affected
        if (node.is_affected) kind_color = THEME_COLORS.ERROR;

        // 1. Glow Halo
        if (node.is_affected || node === selected_node) {
            ctx.beginPath();
            ctx.arc(x, y, radius * 1.8, 0, 2 * Math.PI, false);
            ctx.fillStyle = node.is_affected ? THEME_COLORS.GLOW_ROSE : THEME_COLORS.GLOW_CYAN;
            ctx.fill();
        }

        // 2. Core Node
        ctx.beginPath();
        ctx.arc(x, y, radius, 0, 2 * Math.PI, false);
        ctx.fillStyle = kind_color;
        ctx.fill();

        // 3. Highlight Border
        if (node === hover_node || node === selected_node) {
            ctx.strokeStyle = 'white';
            ctx.lineWidth = 1 / global_scale;
            ctx.stroke();
        }

        // 4. Label (Zoom Dependent)
        if (global_scale > 1.2 || node === selected_node || node.is_affected) {
            ctx.font = `${font_size}px ${GRAPH_THEME.LABEL_FONT}`;
            ctx.textAlign = 'center';
            ctx.textBaseline = 'top';
            ctx.fillStyle = node.is_affected ? '#fda4af' : 'white';
            ctx.fillText(label, x, y + radius + 2);
        }

        ctx.restore();
    }, [selected_node, affected_nodes, hover_node]);

    return (
        <div className="w-full h-full relative bg-zinc-950 rounded-2xl border border-zinc-900 overflow-hidden">
            {loading ? (
                <div className="absolute inset-0 flex items-center justify-center bg-zinc-950/80 backdrop-blur-sm z-50">
                    <div className="flex flex-col items-center gap-4">
                        <RefreshCw className="w-8 h-8 text-cyan-500 animate-spin" />
                        <p className="text-[10px] font-bold text-zinc-500 uppercase tracking-[0.3em]">Synthesizing Symbol Graph...</p>
                    </div>
                </div>
            ) : null}

            <ForceGraph2D
                ref={fg_ref}
                graphData={graph_data}
                nodeCanvasObject={node_canvas_object}
                nodePointerAreaPaint={(node: ExtendedGraphNode, color, ctx) => {
                    ctx.fillStyle = color;
                    ctx.beginPath(); ctx.arc(node.x ?? 0, node.y ?? 0, 8, 0, 2 * Math.PI, false); ctx.fill();
                }}
                linkColor={(link: any) => {
                    const is_source_affected = affected_nodes.has(link.source.id || link.source);
                    const is_target_affected = affected_nodes.has(link.target.id || link.target);
                    return is_source_affected && is_target_affected ? THEME_COLORS.ERROR : THEME_COLORS.NEURAL_GRID;
                }}
                linkWidth={(link: any) => {
                    const is_source_affected = affected_nodes.has(link.source.id || link.source);
                    return is_source_affected ? 2 : 1;
                }}
                linkDirectionalParticles={2}
                linkDirectionalParticleSpeed={GRAPH_THEME.PARTICLE_SPEED}
                linkDirectionalParticleWidth={(link: any) => affected_nodes.has(link.source.id || link.source) ? 3 : 0}
                backgroundColor="rgba(0,0,0,0)"
                d3AlphaDecay={0.01}
                d3VelocityDecay={0.3}
                onNodeClick={handle_node_click}
                onNodeHover={(node: any) => set_hover_node(node)}
            />

            {/* Header HUD */}
            <div className="absolute top-6 left-6 pointer-events-none select-none">
                <div className="flex flex-col gap-2">
                    <div className="flex items-center gap-3">
                        <div className="w-2.5 h-2.5 rounded-full bg-cyan-500 shadow-[0_0_15px_#22d3ee]" />
                        <h2 className="text-xs font-black text-white uppercase tracking-[0.4em]">Knowledge Graph</h2>
                    </div>
                    <div className="flex items-center gap-4 ml-6">
                        <div className="flex items-center gap-2">
                            <Target size={10} className="text-zinc-500" />
                            <span className="text-[9px] font-bold text-zinc-500 uppercase tracking-widest">{data?.nodes.length || 0} Symbols</span>
                        </div>
                        <div className="w-px h-2 bg-zinc-800" />
                        <div className="flex items-center gap-2">
                            <Zap size={10} className="text-zinc-500" />
                            <span className="text-[9px] font-bold text-zinc-500 uppercase tracking-widest">{data?.links.length || 0} Edges</span>
                        </div>
                    </div>
                </div>
            </div>

            {/* Floating Info Panel */}
            {selected_node && (
                <div className="absolute bottom-6 left-6 w-72 bg-zinc-900/80 backdrop-blur-xl border border-zinc-800 p-5 rounded-2xl animate-in fade-in slide-in-from-bottom-4 duration-300">
                    <div className="flex flex-col gap-4">
                        <div className="flex items-start justify-between">
                            <div className="flex flex-col gap-1">
                                <span className="text-[8px] font-black text-cyan-400 uppercase tracking-[0.2em]">{selected_node.kind}</span>
                                <h3 className="text-sm font-bold text-white truncate max-w-[200px]">{selected_node.name}</h3>
                            </div>
                            <button 
                                onClick={() => { set_selected_node(null); set_affected_nodes(new Set()); }}
                                className="text-zinc-500 hover:text-white transition-colors"
                            >
                                <RefreshCw size={14} />
                            </button>
                        </div>

                        <div className="flex flex-col gap-2">
                             <div className="flex items-center gap-2 bg-zinc-950/50 p-2 rounded-lg border border-zinc-800/50">
                                <Info size={12} className="text-zinc-500" />
                                <span className="text-[10px] text-zinc-400 font-mono truncate">{selected_node.path}</span>
                             </div>
                             
                             <div className="mt-2 flex flex-col gap-2">
                                <div className="flex items-center justify-between">
                                    <span className="text-[9px] font-bold text-zinc-500 uppercase">Blast Radius</span>
                                    <span className="text-[9px] font-bold text-rose-500 bg-rose-500/10 px-1.5 py-0.5 rounded-md">+{affected_nodes.size - 1} dependents</span>
                                </div>
                                <div className="w-full h-1 bg-zinc-800 rounded-full overflow-hidden">
                                    <div className="h-full bg-rose-500" style={{ width: `${Math.min(100, (affected_nodes.size / (data?.nodes.length || 1)) * 100)}%` }} />
                                </div>
                             </div>
                        </div>

                        <div className="grid grid-cols-2 gap-2 mt-2">
                            <button className="flex items-center justify-center gap-2 px-3 py-2 bg-zinc-800 hover:bg-zinc-700 text-white rounded-xl transition-all group">
                                <Search size={12} className="text-cyan-400 group-hover:scale-110 transition-transform" />
                                <span className="text-[9px] font-bold uppercase tracking-widest">Explore</span>
                            </button>
                            <button className="flex items-center justify-center gap-2 px-3 py-2 bg-cyan-500 hover:bg-cyan-400 text-black rounded-xl transition-all group">
                                <Target size={12} className="group-hover:scale-110 transition-transform" />
                                <span className="text-[9px] font-bold uppercase tracking-widest">Analyze</span>
                            </button>
                        </div>
                    </div>
                </div>
            )}

            {/* Legend */}
            <div className="absolute top-6 right-6 flex flex-col gap-2 bg-zinc-950/40 backdrop-blur-md p-3 rounded-xl border border-zinc-900/50">
                <div className="flex items-center gap-2">
                    <div className="w-1.5 h-1.5 rounded-full bg-cyan-400" />
                    <span className="text-[8px] font-bold text-zinc-400 uppercase tracking-widest">Function</span>
                </div>
                <div className="flex items-center gap-2">
                    <div className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
                    <span className="text-[8px] font-bold text-zinc-400 uppercase tracking-widest">Struct</span>
                </div>
                <div className="flex items-center gap-2">
                    <div className="w-1.5 h-1.5 rounded-full bg-amber-400" />
                    <span className="text-[8px] font-bold text-zinc-400 uppercase tracking-widest">Trait</span>
                </div>
            </div>
        </div>
    );
};
