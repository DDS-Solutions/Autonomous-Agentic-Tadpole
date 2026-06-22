/**
 * @docs ARCHITECTURE:UI-Components
 * 
 * ### AI Assist Note
 * **Knowledge Graph Component**: Coordinates codebase dependency visualization and OKF knowledge graph visualization.
 * Decomposed into specialized sub-modules (GraphView, CognitionSidebar, AnomalyPanel)
 * to separate concerns, prevent render cycles, and ensure type safety.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Canvas layout failures, or API connection issues loading graph nodes.
 * - **Telemetry Link**: Search for `[KnowledgeGraph]` in UI tracing.
 */

import React, { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import type { ForceGraphMethods } from 'react-force-graph-2d';
import { Target, Zap, RefreshCw } from 'lucide-react';
import { intelligence_api_service, type CodeGraphData } from '../../services/intelligence_api_service';
import type { ExtendedGraphNode, ForceGraphLink } from './knowledge_graph/types';
import { GraphView } from './knowledge_graph/GraphView';
import { CognitionSidebar } from './knowledge_graph/CognitionSidebar';
import { AnomalyPanel } from './knowledge_graph/AnomalyPanel';

// Helper to extract links from text matching concept IDs in OKF descriptions
const extractLinks = (text: string, nodeIds: Set<string>): string[] => {
    if (!text || typeof text !== 'string') return [];
    const linkRegex = /\[([^\]]+)\]\(([^)]+)\)/g;
    const targets: string[] = [];
    let match;
    while ((match = linkRegex.exec(text)) !== null) {
        const targetUrl = match[2];
        const cleanTarget = targetUrl.replace(/^\//, '').replace(/\.md$/, '').trim();
        if (nodeIds.has(cleanTarget)) {
            targets.push(cleanTarget);
        } else {
            for (const id of nodeIds) {
                if (id.endsWith(cleanTarget) || cleanTarget.endsWith(id)) {
                    targets.push(id);
                    break;
                }
            }
        }
    }
    return targets;
};

export const KnowledgeGraph: React.FC = () => {
    const fg_ref = useRef<ForceGraphMethods<ExtendedGraphNode, ForceGraphLink> | undefined>(undefined);
    const [data, set_data] = useState<CodeGraphData | null>(null);
    const [loading, set_loading] = useState(true);
    const [selected_node, set_selected_node] = useState<ExtendedGraphNode | null>(null);
    const [affected_nodes, set_affected_nodes] = useState<Set<string>>(new Set());
    const [hover_node, set_hover_node] = useState<ExtendedGraphNode | null>(null);
    const [active_info_tab, set_active_info_tab] = useState<'info' | 'memory'>('info');
    
    // View mode state to toggle between codebase symbols and semantic OKF graph
    const [view_mode, set_view_mode] = useState<'symbols' | 'okf'>('symbols');

    const is_memory_node = useMemo(() => {
        if (!selected_node) return false;
        const name = selected_node.name.toLowerCase();
        const path = selected_node.path.toLowerCase();
        return name.includes('memory') || path.includes('memory');
    }, [selected_node]);

    const fetch_graph = useCallback(async () => {
        set_loading(true);
        try {
            if (view_mode === 'symbols') {
                const graph = await intelligence_api_service.get_graph();
                set_data(graph);
            } else {
                // Fetch OKF knowledge entries from IKS (limit 200)
                const entries = await intelligence_api_service.get_knowledge({ limit: 200 });
                const nodeIds = new Set(entries.map(e => e.id));
                const nodes = entries.map(e => ({
                    id: e.id,
                    name: e.title || e.id,
                    path: e.topic,
                    kind: e.concept_type,
                    signature: e.resource_uri || '',
                    start_line: 0,
                    end_line: 0,
                    is_affected: false,
                    concept_type: e.concept_type,
                    title: e.title || undefined,
                    description: e.description || undefined,
                    resource_uri: e.resource_uri || undefined,
                    tags: e.tags || undefined,
                    confidence: e.confidence,
                    human_confirmed: e.human_confirmed,
                    text: e.text,
                }));

                const links: { source: string; target: string }[] = [];
                const processedLinks = new Set<string>();

                for (const entry of entries) {
                    const targets = extractLinks(entry.text, nodeIds);
                    for (const target of targets) {
                        const linkKey = `${entry.id}->${target}`;
                        if (!processedLinks.has(linkKey)) {
                            links.push({
                                source: entry.id,
                                target: target,
                            });
                            processedLinks.add(linkKey);
                        }
                    }
                }

                set_data({
                    nodes: nodes as ExtendedGraphNode[],
                    links,
                });
            }
        } catch (err) {
            console.error(`[KnowledgeGraph] Failed to fetch graph (${view_mode}):`, err);
        } finally {
            set_loading(false);
        }
    }, [view_mode]);

    useEffect(() => {
        // eslint-disable-next-line react-hooks/set-state-in-effect
        fetch_graph();
        set_selected_node(null);
        set_affected_nodes(new Set());
    }, [fetch_graph]);

    // Transform data for force-graph
    const graph_data = useMemo(() => {
        if (!data) return { nodes: [], links: [] };

        const nodes: ExtendedGraphNode[] = data.nodes.map(node => {
            const id = view_mode === 'okf' ? (node as any).id : `${node.path}:${node.name}`;
            return {
                ...node,
                id,
                is_affected: affected_nodes.has(id)
            };
        });

        const links: ForceGraphLink[] = data.links.map(link => ({
            source: typeof link.source === 'string' ? link.source : (link.source as { id: string }).id,
            target: typeof link.target === 'string' ? link.target : (link.target as { id: string }).id
        }));

        return { nodes, links };
    }, [data, affected_nodes, view_mode]);

    const handle_node_click = async (node: ExtendedGraphNode) => {
        set_selected_node(node);
        try {
            if (view_mode === 'okf') {
                const peers = await intelligence_api_service.get_knowledge_peers(node.id, 5);
                const peer_ids = new Set(peers.map(p => p.id));
                set_affected_nodes(peer_ids);
            } else {
                const affected = await intelligence_api_service.get_blast_radius(node.name, node.path);
                const affected_ids = new Set(affected.map(n => `${n.path}:${n.name}`));
                set_affected_nodes(affected_ids);
            }

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
            <div className="absolute top-6 left-6 pointer-events-none select-none z-30">
                <div className="flex flex-col gap-2">
                    <div className="flex items-center gap-3">
                        <div className="w-2.5 h-2.5 rounded-full bg-cyan-500 shadow-[0_0_15px_#22d3ee]" />
                        <h2 className="text-xs font-black text-white uppercase tracking-[0.4em]">
                            {view_mode === 'okf' ? 'OKF Knowledge Graph' : 'Codebase Symbols'}
                        </h2>
                    </div>
                    <div className="flex items-center gap-4 ml-6">
                        <div className="flex items-center gap-2">
                            <Target size={10} className="text-zinc-500" />
                            <span className="text-[9px] font-bold text-zinc-500 uppercase tracking-widest">
                                {data?.nodes.length || 0} {view_mode === 'okf' ? 'Concepts' : 'Symbols'}
                            </span>
                        </div>
                        <div className="w-px h-2 bg-zinc-800" />
                        <div className="flex items-center gap-2">
                            <Zap size={10} className="text-zinc-500" />
                            <span className="text-[9px] font-bold text-zinc-500 uppercase tracking-widest">{data?.links.length || 0} Edges</span>
                        </div>
                    </div>
                </div>
            </div>

            {/* View Mode Toggle */}
            <div className="absolute top-20 left-6 flex items-center gap-2 z-30 bg-zinc-900/80 border border-zinc-800 rounded-lg p-0.5 backdrop-blur-md">
                <button
                    onClick={() => set_view_mode('symbols')}
                    className={`px-2.5 py-1 text-[9px] font-black uppercase tracking-wider rounded-md transition-all cursor-pointer ${
                        view_mode === 'symbols'
                            ? 'bg-zinc-800 text-cyan-400 border border-zinc-700'
                            : 'text-zinc-500 hover:text-zinc-300'
                    }`}
                >
                    Symbols Mode
                </button>
                <button
                    onClick={() => set_view_mode('okf')}
                    className={`px-2.5 py-1 text-[9px] font-black uppercase tracking-wider rounded-md transition-all cursor-pointer ${
                        view_mode === 'okf'
                            ? 'bg-zinc-800 text-cyan-400 border border-zinc-700'
                            : 'text-zinc-500 hover:text-zinc-300'
                    }`}
                >
                    Knowledge Mode
                </button>
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
                {view_mode === 'okf' ? (
                    <>
                        <div className="flex items-center gap-2">
                            <div className="w-1.5 h-1.5 rounded-full bg-emerald-500" />
                            <span className="text-[8px] font-bold text-zinc-400 uppercase tracking-widest font-mono">Confirmed</span>
                        </div>
                        <div className="flex items-center gap-2">
                            <div className="w-1.5 h-1.5 rounded-full bg-amber-500" />
                            <span className="text-[8px] font-bold text-zinc-400 uppercase tracking-widest font-mono">Expiring</span>
                        </div>
                        <div className="flex items-center gap-2">
                            <div className="w-1.5 h-1.5 rounded-full bg-red-500" />
                            <span className="text-[8px] font-bold text-zinc-400 uppercase tracking-widest font-mono">Broken</span>
                        </div>
                        <div className="flex items-center gap-2">
                            <div className="w-1.5 h-1.5 rounded-full bg-zinc-500" />
                            <span className="text-[8px] font-bold text-zinc-400 uppercase tracking-widest font-mono">Base</span>
                        </div>
                    </>
                ) : (
                    <>
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
                            {/* Replaced with cyan-500 */}
                            <div className="w-1.5 h-1.5 rounded-full bg-cyan-500" />
                            <span className="text-[8px] font-bold text-zinc-400 uppercase tracking-widest">Enum</span>
                        </div>
                    </>
                )}
            </div>

            {/* Code Anomalies Panel */}
            {data?.anomalies && data.anomalies.length > 0 && view_mode === 'symbols' && (
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

// Metadata: [KnowledgeGraph]
