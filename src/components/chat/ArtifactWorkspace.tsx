import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Code, Box, Layers, Play, X, Zap } from 'lucide-react';
import { useStreamdown } from '../../hooks/useStreamdown';
import { i18n } from '../../i18n';
import clsx from 'clsx';

interface ArtifactWorkspaceProps {
    agentId: string;
    missionId: string;
    isOpen: boolean;
    onClose: () => void;
}

export const ArtifactWorkspace: React.FC<ArtifactWorkspaceProps> = ({
    agentId,
    missionId,
    isOpen,
    onClose
}) => {
    const { activeStreams } = useStreamdown();
    const streamKey = `${agentId}-${missionId}`;
    const liveText = activeStreams[streamKey] || '';
    
    // Simple regex to find the last code block in the stream
    const codeMatch = liveText.match(/```(?:python|py|javascript|js|bash|sh|ps1)\n([\s\S]*?)(?:```|$)/);
    const code = codeMatch ? codeMatch[1] : '';

    return (
        <AnimatePresence>
            {isOpen && (
                <motion.div
                    initial={{ x: '100%' }}
                    animate={{ x: 0 }}
                    exit={{ x: '100%' }}
                    transition={{ type: 'spring', damping: 25, stiffness: 200 }}
                    className="absolute inset-y-0 right-0 w-[500px] bg-zinc-950 border-l border-zinc-800 shadow-2xl z-50 flex flex-col"
                >
                    <div className="p-4 border-b border-zinc-800 flex items-center justify-between bg-zinc-900/50 backdrop-blur-md">
                        <div className="flex items-center gap-3">
                            <div className="p-2 rounded-lg bg-blue-500/10 text-blue-400 border border-blue-500/20">
                                <Box size={18} />
                            </div>
                            <div>
                                <h3 className="text-[11px] font-black uppercase tracking-[0.2em] text-zinc-100">
                                    {i18n.t('evolution.workspace_title')}
                                </h3>
                                <p className="text-[9px] text-zinc-500 font-mono mt-0.5 uppercase tracking-tighter">
                                    Live Stream :: {agentId} :: {missionId.slice(0, 8)}
                                </p>
                            </div>
                        </div>
                        <button 
                            onClick={onClose}
                            className="p-2 text-zinc-500 hover:text-white transition-colors"
                        >
                            <X size={18} />
                        </button>
                    </div>

                    <div className="flex-1 overflow-hidden flex flex-col">
                        {/* Tabs */}
                        <div className="flex items-center gap-1 p-2 bg-black/20">
                            <button className="px-3 py-1.5 rounded-lg bg-zinc-800 text-zinc-100 text-[10px] font-bold flex items-center gap-2">
                                <Code size={12} />
                                SOURCE
                            </button>
                            <button className="px-3 py-1.5 rounded-lg text-zinc-500 hover:text-zinc-300 text-[10px] font-bold flex items-center gap-2">
                                <Layers size={12} />
                                PREVIEW
                            </button>
                        </div>

                        {/* Code Editor Area */}
                        <div className="flex-1 p-4 font-mono text-[11px] leading-relaxed overflow-y-auto custom-scrollbar bg-black/40 text-zinc-400">
                            {code ? (
                                <pre className="whitespace-pre-wrap">
                                    {code.split('\n').map((line, i) => (
                                        <div key={i} className="flex gap-4 group">
                                            <span className="w-8 text-right text-zinc-800 select-none">{i + 1}</span>
                                            <span className={clsx(
                                                "flex-1",
                                                line.trim().startsWith('#') && "text-zinc-600 italic",
                                                line.includes('def ') && "text-blue-400",
                                                line.includes('import ') && "text-purple-400",
                                                line.includes('"') && "text-emerald-400"
                                            )}>
                                                {line}
                                            </span>
                                        </div>
                                    ))}
                                    <motion.span 
                                        animate={{ opacity: [1, 0, 1] }} 
                                        transition={{ repeat: Infinity, duration: 0.8 }}
                                        className="inline-block w-1 h-3 bg-zinc-500 ml-1 translate-y-0.5"
                                    />
                                </pre>
                            ) : (
                                <div className="h-full flex flex-col items-center justify-center text-zinc-700 opacity-30 gap-4">
                                    <Zap size={32} />
                                    <span className="text-[10px] font-black uppercase tracking-[0.3em]">Waiting for stream...</span>
                                </div>
                            )}
                        </div>
                    </div>

                    {/* Footer Controls */}
                    <div className="p-4 border-t border-zinc-800 bg-zinc-900/30 flex items-center justify-between">
                        <div className="flex items-center gap-4">
                            <div className="flex items-center gap-2">
                                <div className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse" />
                                <span className="text-[9px] text-zinc-500 font-bold uppercase tracking-widest">Compiler Ready</span>
                            </div>
                        </div>
                        <button className="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-[10px] font-bold uppercase tracking-widest rounded-lg transition-all shadow-lg shadow-blue-900/20 flex items-center gap-2">
                            <Play size={12} fill="currentColor" />
                            Run Logic
                        </button>
                    </div>
                </motion.div>
            )}
        </AnimatePresence>
    );
};
