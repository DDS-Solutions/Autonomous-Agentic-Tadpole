/**
 * @docs ARCHITECTURE:UI-Components
 * 
 * ### AI Assist Note
 * **Core technical resource for the Tadpole OS Sovereign infrastructure.**
 * Handles reactive state and high-fidelity user interactions.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI regression, hook desync, or API timeout.
 * - **Telemetry Link**: Search `[AnomalyPanel]` in observability traces.
 */

import React from 'react';
import { AlertTriangle } from 'lucide-react';
import type { ExtendedGraphNode } from './types';

interface AnomalyPanelProps {
    anomalies: string[];
    nodes: ExtendedGraphNode[];
    selected_node: ExtendedGraphNode | null;
    on_anomaly_click: (node: ExtendedGraphNode) => void;
}

export const AnomalyPanel: React.FC<AnomalyPanelProps> = ({
    anomalies,
    nodes,
    selected_node,
    on_anomaly_click
}) => {
    return (
        <div className="absolute top-36 right-6 w-80 bg-zinc-900/80 backdrop-blur-xl border border-zinc-800 p-4 rounded-2xl flex flex-col gap-3 max-h-[300px] overflow-y-auto custom-scrollbar z-40 shadow-2xl">
            <div className="flex items-center gap-2 border-b border-zinc-850 pb-2">
                <AlertTriangle size={12} className="text-amber-500" />
                <span className="text-[10px] font-black text-amber-500 uppercase tracking-widest">
                    Code Anomalies ({anomalies.length})
                </span>
            </div>
            <div className="flex flex-col gap-2">
                {anomalies.map((anomaly, idx) => {
                    const match = anomaly.match(/Unused symbol \(0 incoming references\): (\w+) in (.+)/);
                    const name = match ? match[1] : anomaly;
                    const path = match ? match[2] : 'Unknown Path';
                    
                    const is_selected = selected_node && selected_node.name === name && selected_node.path === path;
                    
                    return (
                        <div
                            key={idx}
                            onClick={() => {
                                if (name && path) {
                                    const node = nodes.find(n => n.name === name && n.path === path);
                                    if (node) {
                                        on_anomaly_click(node);
                                    }
                                }
                            }}
                            className={`p-2.5 rounded-xl border transition-all cursor-pointer text-left flex flex-col gap-1 group/anom ${
                                is_selected 
                                    ? 'bg-amber-500/10 border-amber-500 shadow-[0_0_15px_rgba(245,158,11,0.15)]' 
                                    : 'bg-zinc-950/60 hover:bg-zinc-950 border-zinc-800 hover:border-amber-500/50'
                            }`}
                        >
                            <div className="flex items-center gap-1.5 justify-between">
                                <span className="text-[9px] font-bold text-amber-400 font-mono">Unused Symbol</span>
                                <span className="text-[8px] text-zinc-500 font-mono font-bold truncate max-w-[150px]" title={path || ''}>
                                    {path}
                                </span>
                            </div>
                            <span className="text-[11px] font-bold text-zinc-200 font-mono truncate group-hover/anom:text-white">
                                {name}
                            </span>
                        </div>
                    );
                })}
            </div>
        </div>
    );
};

// Metadata: [AnomalyPanel]
