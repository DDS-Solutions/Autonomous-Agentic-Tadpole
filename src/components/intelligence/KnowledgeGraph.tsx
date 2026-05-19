/**
 * @docs ARCHITECTURE:UI-Components
 * 
 * ### AI Assist Note
 * **Knowledge Graph Component**: Coordinates codebase dependency visualization.
 * Decomposed into specialized sub-modules (GraphView, CognitionSidebar, AnomalyPanel)
 * to separate concerns, prevent render cycles, and ensure type safety.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Canvas layout failures, or API connection issues loading graph nodes.
 * - **Telemetry Link**: Search for `[KnowledgeGraph]` in UI tracing.
 */

import React, { useEffect, useMemo, useRef, useState } from 'react';
import type { ForceGraphMethods } from 'react-force-graph-2d';
import { Target, Zap, RefreshCw } from 'lucide-react';
import { intelligence_api_service, type CodeGraphData } from '../../services/intelligence_api_service';
import type { ExtendedGraphNode, ForceGraphLink } from './knowledge_graph/types';
import { GraphView } from './knowledge_graph/GraphView';
import { CognitionSidebar } from './knowledge_graph/CognitionSidebar';
import { AnomalyPanel } from './knowledge_graph/AnomalyPanel';

export const KnowledgeGraph: React.FC = () => {
    const fg_ref = useRef<ForceGraphMethods<ExtendedGraphNode, ForceGraphLink> | undefined>(undefined);
    const [data, set_data] = useState<CodeGraphData | null>(null);
    const [loading, set_loading] = useState(true);
    const [selected_node, set_selected_node] = useState<ExtendedGraphNode | null>(null);
    const [affected_nodes, set_affected_nodes] = useState<Set<string>>(new Set());
    const [hover_node, set_hover_node] = useState<ExtendedGraphNode | null>(null);
    const [active_info_tab, set_active_info_tab] = useState<'info' | 'memory'>('info');

    const is_memory_node = useMemo(() => {
        if (!selected_node) return false;
        const name = selected_node.name.toLowerCase();
        const path = selected_node.path.toLowerCase();
        return name.includes('memory') || path.includes('memory');
    }, [selected_node]);

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

        const links: ForceGraphLink[] = data.links.map(link => ({
            source: link.source,
            target: link.target
        }));

        return { nodes, links };
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

    const handle_close_sidebar = () => {
        set_selected_node(null);
        set_affected_nodes(new Set());
    };

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

            {!loading && (
                <GraphView
                    graph_data={graph_data}
                    selected_node={selected_node}
                    hover_node={hover_node}
                    set_hover_node={set_hover_node}
                    affected_nodes={affected_nodes}
                    on_node_click={handle_node_click}
                    fg_ref={fg_ref}
                />
            )}

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
                <CognitionSidebar
                    selected_node={selected_node}
                    is_memory_node={is_memory_node}
                    active_info_tab={active_info_tab}
                    set_active_info_tab={set_active_info_tab}
                    affected_nodes={affected_nodes}
                    total_nodes_count={data?.nodes.length || 0}
                    on_close={handle_close_sidebar}
                />
            )}

            {/* Legend */}
            <div className="absolute top-6 right-6 flex flex-col gap-2 bg-zinc-950/40 backdrop-blur-md p-3 rounded-xl border border-zinc-900/50 select-none pointer-events-none z-30">
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

            {/* Code Anomalies Panel */}
            {data?.anomalies && data.anomalies.length > 0 && (
                <AnomalyPanel
                    anomalies={data.anomalies}
                    nodes={graph_data.nodes}
                    selected_node={selected_node}
                    on_anomaly_click={handle_node_click}
                />
            )}
        </div>
    );
};

// Metadata: [knowledge_graph]
