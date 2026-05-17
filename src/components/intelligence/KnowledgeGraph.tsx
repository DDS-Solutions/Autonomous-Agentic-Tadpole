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
import { Search, Info, Zap, Target, RefreshCw, Brain, Trash2, Send, Cpu } from 'lucide-react';
import { intelligence_api_service, type CodeGraphData } from '../../services/intelligence_api_service';
import { THEME_COLORS, GRAPH_THEME } from '../../constants/theme';
import type { SymbolNode } from '../../types/schemas';
import { use_agent_store } from '../../stores/agent_store';
import { use_memory_store, type MemoryEntry } from '../../stores/memory_store';

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

const get_kind_display = (kind: string): string => {
    switch (kind.toLowerCase()) {
        case 'func': return 'Function';
        case 'struct': return 'Struct';
        case 'class': return 'Class';
        case 'trait': return 'Trait';
        case 'interface': return 'Interface';
        case 'enum': return 'Enum';
        case 'impl': return 'Implementation';
        case 'type': return 'Type';
        case 'method': return 'Method';
        default: return kind.charAt(0).toUpperCase() + kind.slice(1);
    }
};

export const KnowledgeGraph: React.FC = () => {
    const fg_ref = useRef<ForceGraphMethods<ExtendedGraphNode, GraphLink> | undefined>(undefined);
    const [data, set_data] = useState<CodeGraphData | null>(null);
    const [loading, set_loading] = useState(true);
    const [selected_node, set_selected_node] = useState<ExtendedGraphNode | null>(null);
    const [affected_nodes, set_affected_nodes] = useState<Set<string>>(new Set());
    const [hover_node, set_hover_node] = useState<ExtendedGraphNode | null>(null);

    // Swarm Agent Store
    const agents = use_agent_store(state => state.agents);
    const fetch_agents = use_agent_store(state => state.fetch_agents);

    // Vector Memory Store
    const {
        memories,
        is_loading: memory_loading,
        error: memory_error,
        fetch_memories,
        save_memory,
        delete_memory,
        search_memories
    } = use_memory_store();

    // Memory card local states
    const [selected_agent_id, set_selected_agent_id] = useState<string>('');
    const [active_info_tab, set_active_info_tab] = useState<'info' | 'memory'>('info');
    const [search_query, set_search_query] = useState('');
    const [search_results, set_search_results] = useState<MemoryEntry[] | null>(null);
    const [is_searching, set_is_searching] = useState(false);
    const [new_memory_text, setNew_memory_text] = useState('');

    const is_memory_node = useMemo(() => {
        if (!selected_node) return false;
        const name = selected_node.name.toLowerCase();
        const path = selected_node.path.toLowerCase();
        return name.includes('memory') || path.includes('memory');
    }, [selected_node]);

    // Fetch agents on mount if list is empty
    useEffect(() => {
        if (agents.length === 0) {
            fetch_agents();
        }
    }, [agents.length, fetch_agents]);

    // Auto-select first agent when memory node is loaded
    useEffect(() => {
        if (is_memory_node && agents.length > 0 && !selected_agent_id) {
            set_selected_agent_id(agents[0].id);
        }
    }, [is_memory_node, agents, selected_agent_id]);

    // Load memories when selected agent or info tab changes
    useEffect(() => {
        if (selected_agent_id && active_info_tab === 'memory') {
            fetch_memories(selected_agent_id);
        }
    }, [selected_agent_id, active_info_tab, fetch_memories]);

    const handle_inject_memory = async () => {
        if (!new_memory_text || !selected_agent_id) return;
        try {
            await save_memory(selected_agent_id, new_memory_text);
            setNew_memory_text('');
        } catch (err) {
            console.error('Failed to inject memory:', err);
        }
    };

    const handle_search = async () => {
        if (!search_query) {
            set_search_results(null);
            return;
        }
        set_is_searching(true);
        try {
            const results = await search_memories(search_query);
            set_search_results(results);
        } catch (err) {
            console.error('Failed to search vector memories:', err);
        } finally {
            set_is_searching(false);
        }
    };

    const display_memories = useMemo(() => {
        if (search_results !== null) {
            return search_results;
        }
        return memories || [];
    }, [search_results, memories]);

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
        const normalized_kind = (node.kind || '').toLowerCase();
        if (normalized_kind === 'func' || normalized_kind === 'function' || normalized_kind === 'method') kind_color = THEME_COLORS.BUSY;
        if (normalized_kind === 'struct' || normalized_kind === 'class') kind_color = THEME_COLORS.SUCCESS;
        if (normalized_kind === 'trait' || normalized_kind === 'interface') kind_color = THEME_COLORS.DEGRADED;
        if (normalized_kind === 'enum') kind_color = '#a855f7'; // Purple

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
                <div className={`absolute bottom-6 left-6 ${is_memory_node && active_info_tab === 'memory' ? 'w-[400px]' : 'w-80'} bg-zinc-900/80 backdrop-blur-xl border border-zinc-800 p-5 rounded-2xl animate-in fade-in slide-in-from-bottom-4 duration-300 transition-all z-50 shadow-2xl`}>
                    <div className="flex flex-col gap-4">
                        <div className="flex items-start justify-between">
                            <div className="flex flex-col gap-1 min-w-0">
                                <span className="text-[8px] font-black text-cyan-400 uppercase tracking-[0.2em]">{get_kind_display(selected_node.kind)}</span>
                                <h3 className="text-sm font-bold text-white truncate pr-2 font-mono" title={selected_node.name}>{selected_node.name}</h3>
                            </div>
                            <button 
                                onClick={() => { set_selected_node(null); set_affected_nodes(new Set()); set_search_results(null); set_search_query(''); }}
                                className="text-zinc-500 hover:text-white transition-colors cursor-pointer shrink-0"
                            >
                                <RefreshCw size={14} />
                            </button>
                        </div>

                        {/* Interactive Memory Tabs */}
                        {is_memory_node && (
                            <div className="flex border-b border-zinc-850 pb-2">
                                <button
                                    onClick={() => set_active_info_tab('info')}
                                    className={`flex-1 text-[10px] font-bold uppercase tracking-wider text-center py-1 transition-all cursor-pointer ${
                                        active_info_tab === 'info'
                                            ? 'text-cyan-400 border-b-2 border-cyan-400 font-black'
                                            : 'text-zinc-500 hover:text-zinc-300 font-medium'
                                    }`}
                                >
                                    Symbol Info
                                </button>
                                <button
                                    onClick={() => set_active_info_tab('memory')}
                                    className={`flex-1 text-[10px] font-bold uppercase tracking-wider text-center py-1 transition-all cursor-pointer ${
                                        active_info_tab === 'memory'
                                            ? 'text-cyan-400 border-b-2 border-cyan-400 font-black'
                                            : 'text-zinc-500 hover:text-zinc-300 font-medium'
                                    }`}
                                >
                                    Memory Workspace
                                </button>
                            </div>
                        )}

                        {(!is_memory_node || active_info_tab === 'info') ? (
                            <div className="flex flex-col gap-4">
                                <div className="flex flex-col gap-2">
                                    <div className="flex items-center gap-2 bg-zinc-950/50 p-2.5 rounded-lg border border-zinc-800/50">
                                        <Info size={12} className="text-zinc-500 shrink-0" />
                                        <span className="text-[10px] text-zinc-400 font-mono truncate" title={selected_node.path}>{selected_node.path}</span>
                                    </div>
                                     
                                    <div className="mt-2 flex flex-col gap-2">
                                        <div className="flex items-center justify-between">
                                            <span className="text-[9px] font-bold text-zinc-500 uppercase tracking-wider">Blast Radius</span>
                                            <span className="text-[9px] font-bold text-rose-500 bg-rose-500/10 px-1.5 py-0.5 rounded-md">+{affected_nodes.size - 1} dependents</span>
                                        </div>
                                        <div className="w-full h-1 bg-zinc-800 rounded-full overflow-hidden">
                                            <div className="h-full bg-rose-500 transition-all duration-500" style={{ width: `${Math.min(100, (affected_nodes.size / (data?.nodes.length || 1)) * 100)}%` }} />
                                        </div>
                                    </div>
                                </div>

                                <div className="grid grid-cols-2 gap-2 mt-2">
                                    <button className="flex items-center justify-center gap-2 px-3 py-2.5 bg-zinc-800 hover:bg-zinc-700/80 text-white rounded-xl transition-all group cursor-pointer border border-zinc-700/30">
                                        <Search size={12} className="text-cyan-400 group-hover:scale-110 transition-transform" />
                                        <span className="text-[9px] font-bold uppercase tracking-widest">Explore</span>
                                    </button>
                                    <button className="flex items-center justify-center gap-2 px-3 py-2.5 bg-cyan-500 hover:bg-cyan-400 text-black rounded-xl transition-all group cursor-pointer">
                                        <Target size={12} className="group-hover:scale-110 transition-transform" />
                                        <span className="text-[9px] font-bold uppercase tracking-widest">Analyze</span>
                                    </button>
                                </div>
                            </div>
                        ) : (
                            <div className="flex flex-col gap-4 font-mono">
                                {/* Agent Selector */}
                                <div className="flex flex-col gap-1.5">
                                    <label className="text-[8px] font-black text-zinc-500 uppercase tracking-widest flex items-center gap-1">
                                        <Cpu size={10} className="text-zinc-400" />
                                        Target Agent Node
                                    </label>
                                    <select
                                        value={selected_agent_id}
                                        onChange={(e) => {
                                            set_selected_agent_id(e.target.value);
                                            set_search_results(null);
                                            set_search_query('');
                                        }}
                                        className="bg-zinc-950 border border-zinc-800 rounded-xl px-3 py-2 text-xs text-zinc-200 focus:outline-none focus:border-cyan-500 font-mono w-full transition-all hover:border-zinc-700 cursor-pointer"
                                    >
                                        {agents.map((agent) => (
                                            <option key={agent.id} value={agent.id}>
                                                {agent.name} [{agent.role}]
                                            </option>
                                        ))}
                                    </select>
                                </div>

                                {/* Memory Injection */}
                                <div className="flex flex-col gap-1.5">
                                    <label className="text-[8px] font-black text-zinc-500 uppercase tracking-widest flex items-center gap-1">
                                        <Brain size={10} className="text-zinc-400" />
                                        Inject Cognition
                                    </label>
                                    <div className="flex gap-2">
                                        <input
                                            type="text"
                                            value={new_memory_text}
                                            onChange={(e) => setNew_memory_text(e.target.value)}
                                            placeholder="Write new mental record..."
                                            className="flex-1 bg-zinc-950 border border-zinc-800 rounded-xl px-3 py-2 text-xs text-zinc-200 focus:outline-none focus:border-cyan-500 font-mono"
                                            onKeyDown={(e) => {
                                                if (e.key === 'Enter') handle_inject_memory();
                                            }}
                                        />
                                        <button
                                            onClick={handle_inject_memory}
                                            disabled={!new_memory_text || memory_loading}
                                            className="px-3 bg-cyan-500 hover:bg-cyan-400 disabled:opacity-40 disabled:hover:bg-cyan-500 disabled:cursor-not-allowed text-black rounded-xl transition-all flex items-center justify-center font-bold font-mono text-xs cursor-pointer"
                                        >
                                            <Send size={12} />
                                        </button>
                                    </div>
                                </div>

                                {/* Vector Search */}
                                <div className="flex flex-col gap-1.5">
                                    <label className="text-[8px] font-black text-zinc-500 uppercase tracking-widest flex items-center gap-1">
                                        <Search size={10} className="text-zinc-400" />
                                        Semantic Search
                                    </label>
                                    <div className="flex gap-2">
                                        <input
                                            type="text"
                                            value={search_query}
                                            onChange={(e) => set_search_query(e.target.value)}
                                            placeholder="Query agent's memory space..."
                                            className="flex-1 bg-zinc-950 border border-zinc-800 rounded-xl px-3 py-2 text-xs text-zinc-200 focus:outline-none focus:border-cyan-500 font-mono"
                                            onKeyDown={(e) => {
                                                if (e.key === 'Enter') handle_search();
                                            }}
                                        />
                                        <button
                                            onClick={handle_search}
                                            disabled={is_searching}
                                            className="px-3 bg-zinc-850 hover:bg-zinc-750 text-zinc-300 hover:text-white border border-zinc-800 rounded-xl transition-all flex items-center justify-center cursor-pointer shrink-0"
                                        >
                                            {is_searching ? <RefreshCw size={12} className="animate-spin" /> : <Search size={12} />}
                                        </button>
                                    </div>
                                    {search_results !== null && (
                                        <button
                                            onClick={() => {
                                                set_search_results(null);
                                                set_search_query('');
                                            }}
                                            className="text-[9px] text-cyan-400 hover:text-cyan-300 font-bold uppercase tracking-wider text-left hover:underline mt-1 cursor-pointer"
                                        >
                                            ✕ Clear Search Results
                                        </button>
                                    )}
                                </div>

                                {/* Memory Records List */}
                                <div className="flex flex-col gap-2">
                                    <div className="flex justify-between items-center border-b border-zinc-850 pb-1">
                                        <label className="text-[8px] font-black text-zinc-500 uppercase tracking-widest">
                                            Memory Vector Space
                                        </label>
                                        {memory_loading && <span className="text-[8px] text-cyan-400 font-bold uppercase tracking-widest animate-pulse">Syncing...</span>}
                                    </div>
                                    <div className="max-h-48 overflow-y-auto space-y-2 pr-1 custom-scrollbar">
                                        {memory_error ? (
                                            <div className="text-[10px] text-rose-400 italic bg-rose-950/20 border border-rose-900/30 p-2 rounded-xl">
                                                {memory_error}
                                            </div>
                                        ) : display_memories.length === 0 ? (
                                            <div className="text-[10px] text-zinc-500 italic bg-zinc-950/40 p-3 rounded-xl border border-zinc-900 text-center font-mono">
                                                No cognitive records found.
                                            </div>
                                        ) : (
                                            display_memories.map((m) => (
                                                <div
                                                    key={m.id}
                                                    className="group/mem p-2.5 bg-zinc-950/60 hover:bg-zinc-950 border border-zinc-800 hover:border-zinc-700/80 rounded-xl transition-all flex items-start justify-between gap-3"
                                                >
                                                    <div className="flex flex-col gap-1 min-w-0">
                                                        <p className="text-[10px] text-zinc-300 leading-relaxed font-mono whitespace-normal break-words max-w-[310px]">
                                                            {m.text}
                                                        </p>
                                                        {m.timestamp && (
                                                            <span className="text-[8px] text-zinc-650 font-mono uppercase font-bold">
                                                                {new Date(m.timestamp * 1000).toLocaleString()}
                                                            </span>
                                                        )}
                                                    </div>
                                                    <button
                                                        onClick={() => delete_memory(selected_agent_id, m.id)}
                                                        className="p-1 text-zinc-600 hover:text-rose-400 hover:bg-rose-500/10 rounded-lg transition-all opacity-0 group-hover/mem:opacity-100 shrink-0 cursor-pointer"
                                                        title="Prune memory"
                                                    >
                                                        <Trash2 size={10} />
                                                    </button>
                                                </div>
                                            ))
                                        )}
                                    </div>
                                </div>
                            </div>
                        )}
                    </div>
                </div>
            )}

            {/* Legend */}
            <div className="absolute top-6 right-6 flex flex-col gap-2 bg-zinc-950/40 backdrop-blur-md p-3 rounded-xl border border-zinc-900/50 select-none pointer-events-none">
                <div className="flex items-center gap-2">
                    <div className="w-1.5 h-1.5 rounded-full bg-cyan-400" />
                    <span className="text-[8px] font-bold text-zinc-400 uppercase tracking-widest">Function / Method</span>
                </div>
                <div className="flex items-center gap-2">
                    <div className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
                    <span className="text-[8px] font-bold text-zinc-400 uppercase tracking-widest">Struct / Class</span>
                </div>
                <div className="flex items-center gap-2">
                    <div className="w-1.5 h-1.5 rounded-full bg-amber-400" />
                    <span className="text-[8px] font-bold text-zinc-400 uppercase tracking-widest">Trait / Interface</span>
                </div>
                <div className="flex items-center gap-2">
                    <div className="w-1.5 h-1.5 rounded-full bg-[#a855f7]" />
                    <span className="text-[8px] font-bold text-zinc-400 uppercase tracking-widest">Enum</span>
                </div>
            </div>
        </div>
    );
};
