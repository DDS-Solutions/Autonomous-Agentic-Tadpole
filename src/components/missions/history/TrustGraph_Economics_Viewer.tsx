/**
 * @docs ARCHITECTURE:Interface
 *
 * ### AI Assist Note
 * **TrustGraph_Economics_Viewer**: Forensic TrustGraph entity/relation inspector and 2PC double-entry
 * economic ledger (`a2a_ledger`) viewer for the Sovereign Observability Core (SOC).
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Empty entity graph or zero-transaction mission ledger.
 * - **Telemetry Link**: Search `[TrustGraph_Economics_Viewer]` in console logs.
 */

import React, { useState } from 'react';
import {
    Network,
    Coins,
    ArrowRight,
    Bot,
    FileText,
    Lightbulb,
    Flag,
    CheckCircle2,
} from 'lucide-react';
import type { TrustGraphPayload, EconomicsPayload } from '../../../types/mission_dossier';
import { Tooltip } from '../../ui';

interface TrustGraphEconomicsViewerProps {
    trust_graph: TrustGraphPayload;
    economics: EconomicsPayload;
}

export const TrustGraph_Economics_Viewer: React.FC<TrustGraphEconomicsViewerProps> = ({
    trust_graph,
    economics,
}) => {
    const [sub_tab, set_sub_tab] = useState<'trustgraph' | 'ledger'>('trustgraph');

    const render_node_icon = (type: string) => {
        const t = type.toUpperCase();
        if (t === 'AGENT') return <Bot size={13} className="text-cyan-400" />;
        if (t === 'FILE') return <FileText size={13} className="text-purple-400" />;
        if (t === 'CONCEPT') return <Lightbulb size={13} className="text-amber-400" />;
        return <Flag size={13} className="text-emerald-400" />;
    };

    return (
        <div className="flex flex-col h-full bg-zinc-950 border border-zinc-800/80 rounded-xl overflow-hidden">
            {/* Header Switcher */}
            <div className="p-3 border-b border-zinc-800/80 bg-zinc-900/50 flex items-center justify-between gap-3 flex-wrap">
                <div className="flex items-center gap-1 bg-zinc-950 p-0.5 rounded-lg border border-zinc-800 text-xs">
                    <Tooltip content="Topological map of executing agents, files, and concept entities" position="bottom">
                        <button
                            onClick={() => set_sub_tab('trustgraph')}
                            className={`flex items-center gap-1.5 px-3 py-1 rounded-md font-semibold transition-colors ${
                                sub_tab === 'trustgraph'
                                    ? 'bg-zinc-800 text-cyan-300 shadow-sm'
                                    : 'text-zinc-500 hover:text-zinc-300'
                            }`}
                        >
                            <Network size={13} />
                            <span>TrustGraph Topology ({trust_graph.entities.length} Nodes)</span>
                        </button>
                    </Tooltip>
                    <Tooltip content="Two-Phase Commit (2PC) atomic micro-credit transactions and token expenditure" position="bottom">
                        <button
                            onClick={() => set_sub_tab('ledger')}
                            className={`flex items-center gap-1.5 px-3 py-1 rounded-md font-semibold transition-colors ${
                                sub_tab === 'ledger'
                                    ? 'bg-zinc-800 text-amber-300 shadow-sm'
                                    : 'text-zinc-500 hover:text-zinc-300'
                            }`}
                        >
                            <Coins size={13} />
                            <span>2PC Economic Ledger ({economics.transactions.length} Tx)</span>
                        </button>
                    </Tooltip>
                </div>

                <div className="flex items-center gap-3 text-[11px] font-mono text-zinc-400">
                    {sub_tab === 'trustgraph' ? (
                        <span>{trust_graph.relations.length} relations mapped</span>
                    ) : (
                        <span>
                            Total Burn: ${economics.total_cost_usd.toFixed(4)} ({economics.total_tokens ?? 0} tokens)
                        </span>
                    )}
                </div>
            </div>

            {/* Sub-tab 1: TrustGraph Topology */}
            {sub_tab === 'trustgraph' && (
                <div className="flex-1 overflow-y-auto custom-scrollbar p-4 flex flex-col gap-4">
                    {trust_graph.entities.length === 0 ? (
                        <div className="flex flex-col items-center justify-center p-12 text-center text-zinc-500">
                            <Network size={36} className="text-zinc-600 mb-2" />
                            <span className="text-xs font-semibold text-zinc-400">No TrustGraph Nodes Indexed</span>
                            <span className="text-[11px] text-zinc-600 mt-1">
                                Entity relationships are generated during multi-agent collaboration
                            </span>
                        </div>
                    ) : (
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                            {/* Entity Nodes */}
                            <div className="flex flex-col gap-2">
                                <span className="text-[10px] font-bold text-zinc-400 uppercase tracking-widest px-1">
                                    Entity Nodes ({trust_graph.entities.length})
                                </span>
                                <div className="flex flex-col gap-1.5">
                                    {trust_graph.entities.map((node) => (
                                        <div
                                            key={node.id}
                                            className="p-3 rounded-lg bg-zinc-900/40 border border-zinc-800/80 flex items-center justify-between"
                                        >
                                            <div className="flex items-center gap-2.5">
                                                <div className="p-1.5 rounded-md bg-zinc-950 border border-zinc-800">
                                                    {render_node_icon(node.node_type)}
                                                </div>
                                                <div className="flex flex-col">
                                                    <span className="text-xs font-semibold text-zinc-200">
                                                        {node.name}
                                                    </span>
                                                    <span className="font-mono text-[10px] text-zinc-500">
                                                        {node.node_type} • ID: {node.id.slice(0, 8)}
                                                    </span>
                                                </div>
                                            </div>
                                            {node.description && (
                                                <span className="text-[11px] text-zinc-400 max-w-xs truncate text-right">
                                                    {node.description}
                                                </span>
                                            )}
                                        </div>
                                    ))}
                                </div>
                            </div>

                            {/* Relationship Edges */}
                            <div className="flex flex-col gap-2">
                                <span className="text-[10px] font-bold text-zinc-400 uppercase tracking-widest px-1">
                                    Relationship Edges ({trust_graph.relations.length})
                                </span>
                                <div className="flex flex-col gap-1.5">
                                    {trust_graph.relations.map((rel) => (
                                        <div
                                            key={rel.id}
                                            className="p-3 rounded-lg bg-zinc-900/40 border border-zinc-800/80 flex items-center justify-between text-xs"
                                        >
                                            <div className="flex items-center gap-2 font-mono text-[11px]">
                                                <span className="text-cyan-400">{rel.source_entity_id.slice(0, 10)}</span>
                                                <ArrowRight size={12} className="text-zinc-600" />
                                                <span className="text-purple-400">{rel.target_entity_id.slice(0, 10)}</span>
                                            </div>
                                            <div className="flex items-center gap-2">
                                                <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-zinc-950 border border-zinc-800 text-zinc-300">
                                                    {rel.relation_type}
                                                </span>
                                                <span className="font-mono text-[10px] text-zinc-500">
                                                    w={rel.weight.toFixed(1)}
                                                </span>
                                            </div>
                                        </div>
                                    ))}
                                </div>
                            </div>
                        </div>
                    )}
                </div>
            )}

            {/* Sub-tab 2: 2PC Economic Ledger */}
            {sub_tab === 'ledger' && (
                <div className="flex-1 overflow-y-auto custom-scrollbar p-4 flex flex-col gap-3">
                    {economics.transactions.length === 0 ? (
                        <div className="flex flex-col items-center justify-center p-12 text-center text-zinc-500">
                            <Coins size={36} className="text-zinc-600 mb-2" />
                            <span className="text-xs font-semibold text-zinc-400">No Micro-Transactions Recorded</span>
                            <span className="text-[11px] text-zinc-600 mt-1">
                                Autonomous 2PC atomic economic settlement events appear here
                            </span>
                        </div>
                    ) : (
                        economics.transactions.map((tx) => (
                            <div
                                key={tx.id}
                                className="p-3.5 rounded-xl bg-zinc-900/40 border border-zinc-800/80 flex items-center justify-between text-xs"
                            >
                                <div className="flex items-center gap-3">
                                    <div className="p-2 rounded-lg bg-amber-500/10 border border-amber-500/20 text-amber-400 font-bold font-mono text-[11px]">
                                        {tx.transaction_type}
                                    </div>
                                    <div className="flex flex-col">
                                        <div className="flex items-center gap-2 font-mono text-[11px] text-zinc-300">
                                            <span>{tx.buyer_address ?? tx.debit_agent_id ?? 'Agent'}</span>
                                            <ArrowRight size={12} className="text-zinc-500" />
                                            <span>{tx.seller_address ?? tx.credit_agent_id ?? 'Sovereign Treasury'}</span>
                                        </div>
                                        <span className="font-mono text-[10px] text-zinc-500 mt-0.5">
                                            TxID: {tx.id} • {new Date(tx.created_at * 1000).toLocaleString()}
                                        </span>
                                    </div>
                                </div>

                                <div className="flex items-center gap-3">
                                    <span className="font-mono text-sm font-bold text-amber-300">
                                        {tx.amount} micro-credits
                                    </span>
                                    <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-bold bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                                        <CheckCircle2 size={10} /> {tx.status}
                                    </span>
                                </div>
                            </div>
                        ))
                    )}
                </div>
            )}
        </div>
    );
};

// Metadata: [TrustGraph_Economics_Viewer]

