/* eslint-disable react-hooks/preserve-manual-memoization */
/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **Root View**: High-level governance and oversight dashboard. 
 * Orchestrates the visualization of swarm-wide compliance, safety protocols, and operational constraints.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Oversight policy violation alert noise, or dashboard lockup during massive telemetry bursts.
 * - **Telemetry Link**: Search for `[Oversight_Dashboard]` or `GOVERNANCE_SYNC` in service logs.
 */

import { useEffect, useState, useMemo } from 'react';
import {
    Shield,
    CheckCircle,
    XCircle,
    Clock,
    AlertTriangle,
    Activity,
    Terminal as TerminalIcon,
    Cpu,
    Plus,
    WifiOff,
    ShieldCheck
} from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { Tw_Empty_State, Tooltip } from '../components/ui';
import { useEngineStatus } from '../hooks/use_engine_status';
import { use_workspace_store } from '../stores/workspace_store';
import { use_agent_store } from '../stores/agent_store';
import type { Agent } from '../contracts/agent/domain';
import { tadpole_os_service } from '../services/tadpoleos_service';
import { Command_Table } from '../components/Command_Table';
import { Auto_Approved_Panel } from '../components/oversight/Auto_Approved_Panel';
import { Action_Ledger } from '../components/oversight/Action_Ledger';
import { i18n } from '../i18n';
import { LD_Json } from '../components/ui/LD_Json';
import { get_safe_date } from '../utils/date_utils';
import type { OversightEntry, LedgerEntry } from '../data/mock_oversight';
import { MOCK_PENDING, MOCK_LEDGER } from '../data/mock_oversight';

export default function Oversight_Dashboard() {
    const navigate = useNavigate();
    const { is_online } = useEngineStatus();
    const [pending, set_pending] = useState<OversightEntry[]>(() => {
        const saved = localStorage.getItem('tadpole_oversight_pending');
        if (saved) {
            try { return JSON.parse(saved); } catch { return []; }
        }
        return [];
    });
    const [ledger, set_ledger] = useState<LedgerEntry[]>(() => {
        const saved = localStorage.getItem('tadpole_oversight_ledger');
        if (saved) {
            try { return JSON.parse(saved); } catch { return []; }
        }
        return [];
    });
    const [filter, set_filter] = useState('');
    const [is_simulated, set_is_simulated] = useState(false);
    const [has_attempted_fetch, set_has_attempted_fetch] = useState(false);
    const [selected_cluster_id, set_selected_cluster_id] = useState<string>('all');
    const { clusters, active_proposals, apply_proposal, dismiss_proposal } = use_workspace_store();
    const { agents: all_agents, update_agent: store_update_agent } = use_agent_store();

    // Persistence: Save to localStorage on change
    useEffect(() => {
        if (pending.length > 0 || has_attempted_fetch) {
            localStorage.setItem('tadpole_oversight_pending', JSON.stringify(pending));
        }
    }, [pending, has_attempted_fetch]);

    useEffect(() => {
        if (ledger.length > 0 || has_attempted_fetch) {
            localStorage.setItem('tadpole_oversight_ledger', JSON.stringify(ledger));
        }
    }, [ledger, has_attempted_fetch]);

    // Poll for data (WebSocket would be better, but polling is simpler for Phase 3 start)
    useEffect(() => {
        const fetch_data = async () => {
            if (is_simulated) {
                if (pending.length === 0 && !has_attempted_fetch) {
                    set_pending(MOCK_PENDING);
                    set_ledger(MOCK_LEDGER);
                }
                return;
            }

            try {
                const [pending_data, ledger_data] = await Promise.all([
                    tadpole_os_service.get_pending_oversight(),
                    tadpole_os_service.get_oversight_ledger()
                ]);

                set_pending(pending_data as OversightEntry[]);
                set_ledger(ledger_data as LedgerEntry[]);
                set_is_simulated(false); // We got real data (even if empty)

                // Use the data just fetched to update stats, rather than waiting for next render
                // This is slightly tricky without closure data, so we let the useEffect [ledger, pending] handle it
                set_has_attempted_fetch(true);
            } catch {
                if (!has_attempted_fetch) {
                    set_pending(MOCK_PENDING);
                    set_ledger(MOCK_LEDGER);
                    set_is_simulated(true);
                    set_has_attempted_fetch(true);
                }
            }
        };

        // updateStats removed - handled by separate effect below

        fetch_data();
        const interval = setInterval(() => {
            if (document.visibilityState === 'visible') fetch_data();
        }, is_simulated ? 5000 : 2000);
        return () => clearInterval(interval);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- pending.length used inside but adding it would restart polling on each decision
    }, [is_simulated, has_attempted_fetch]);

    // Use useMemo for stats to avoid cascading renders
    // Reactive stats that respect the current cluster filter
    const active_collaborators = useMemo(() => {
        return new Set(clusters.find(c => c.id === selected_cluster_id)?.collaborators || []);
    }, [clusters, selected_cluster_id]);

    const stats = useMemo(() => {
        const base_ledger = selected_cluster_id === 'all' 
            ? ledger 
            : ledger.filter(l => {
                const tc = l.tool_call || l;
                return tc.cluster_id === selected_cluster_id || 
                       active_collaborators.has(tc.agent_id || '');
            });
            
        const base_pending = selected_cluster_id === 'all'
            ? pending
            : pending.filter(p => {
                const tc = p.tool_call || p;
                return tc.cluster_id === selected_cluster_id ||
                       active_collaborators.has(tc.agent_id || '');
            });

        return {
            pending: base_pending.length,
            approved: base_ledger.filter((entry) => entry.decision === 'approved').length,
            rejected: base_ledger.filter((entry) => entry.decision === 'rejected').length
        };
    }, [pending, ledger, selected_cluster_id, active_collaborators]);

    const handle_decide = async (id: string, decision: 'approved' | 'rejected') => {
        if (is_simulated) {
            // Simulated local move
            const entry = pending.find(p => p.id === id);
            if (entry) {
                set_ledger(prev => [{ ...entry, decision, timestamp: new Date().toISOString() } as LedgerEntry, ...prev]);
                set_pending(prev => prev.filter(p => p.id !== id));
            }
            return;
        }

        try {
            await tadpole_os_service.decide_oversight(id, decision);
            // Optimistic update
            set_pending(prev => prev.filter(p => p.id !== id));
        } catch {
            // Silently fail, would be logged in a real environment
        }
    };

    const handle_apply_proposal = async (cluster_id: string) => {
        const proposal = Object.values(active_proposals || {}).find(p => p.cluster_id === cluster_id);
        if (!proposal) return;

        // Commit all proposed changes to the live agent registry
        for (const change of (proposal.changes || [])) {
            const updates: Partial<Agent> = {};
            if (change.proposed_role) updates.role = change.proposed_role;
            if (change.proposed_model) {
                // Assuming primary slot update for swarm optimization
                updates.model = change.proposed_model;
            }
            if (change.added_skills) {
                const agent = all_agents.find(a => a.id === change.agent_id);
                if (agent) {
                    updates.skills = [...new Set([...(agent.skills || []), ...change.added_skills])];
                }
            }

            if (Object.keys(updates).length > 0) {
                await store_update_agent(change.agent_id, updates);
            }
        }

        apply_proposal(cluster_id);
    };

    const handle_kill_switch = async () => {
        if (!confirm(i18n.t('oversight.confirm_halt_agents'))) return;

        if (is_simulated) {
            set_pending([]);
            return;
        }

        try {
            await tadpole_os_service.kill_agents();
            alert(i18n.t('oversight.agents_halted'));
        } catch {
            alert(i18n.t('oversight.halt_agents_failed'));
        }
    };

    const handle_kill_engine = async () => {
        if (!confirm(i18n.t('oversight.confirm_kill_engine'))) return;

        const userInput = prompt(i18n.t('oversight.shutdown_confirm_prompt'));
        if (userInput !== 'SHUTDOWN') return;

        try {
            await tadpole_os_service.shutdown_engine();
            alert(i18n.t('oversight.engine_shutting_down'));
        } catch (e: unknown) {
            alert(i18n.t('oversight.kill_engine_failed', { error: e instanceof Error ? e.message : 'Unknown error' }));
        }
    };




    const filtered_pending = pending.filter(p => {
        const tool_call = p.tool_call || p;
        return selected_cluster_id === 'all' || tool_call.cluster_id === selected_cluster_id;
    });

    return (
        <div className="p-6 space-y-6 max-w-7xl mx-auto">
            {/* GEO Optimization: Structured Data & Semantic Header */}
            <LD_Json data={{
              "@context": "https://schema.org",
              "@type": "SoftwareApplication",
              "name": "Tadpole OS Oversight Dashboard",
              "description": "High-level governance and oversight dashboard for swarm-wide compliance and safety protocols.",
              "provider": { "@type": "Organization", "name": "Sovereign Engineering" },
              "applicationCategory": "Governance System"
            }} />
            <h1 className="sr-only">Tadpole OS Oversight & Governance Command</h1>
            {/* Fallback / Simulation Banner */}
            {is_simulated && (
                <div className="bg-amber-500/10 border border-amber-500/30 p-3 rounded-xl flex items-center justify-between animate-in fade-in slide-in-from-top-4">
                    <div className="flex items-center gap-3">
                        <div className="p-1.5 bg-amber-500/20 rounded-lg">
                            <WifiOff size={16} className="text-amber-500" />
                        </div>
                        <div>
                            <p className="text-xs font-bold text-amber-200 uppercase tracking-widest">{i18n.t('oversight.disconnected_title')}</p>
                            <p className="text-[10px] text-amber-500/70 font-mono">{i18n.t('oversight.disconnected_subtitle')}</p>
                        </div>
                    </div>
                    <button
                        onClick={() => set_is_simulated(false)}
                        className="text-[10px] px-3 py-1 bg-amber-500/20 hover:bg-amber-500/30 text-amber-400 rounded-full border border-amber-500/30 transition-colors uppercase font-bold tracking-tighter"
                    >
                        {i18n.t('oversight.retry_connection')}
                    </button>
                </div>
            )}

            {/* Header / Stats */}
            <div className="grid grid-cols-1 md:grid-cols-5 gap-4">
                <Tooltip content={i18n.t('oversight.pending_actions_tooltip')} position="top" class_name="w-full">
                    <div className="bg-zinc-900 border border-zinc-800 p-4 rounded-lg flex items-center justify-between cursor-help w-full">
                        <div>
                            <p className="text-zinc-400 text-sm">{i18n.t('oversight.pending_actions_label')}</p>
                            <p className="text-2xl font-bold text-yellow-400">{stats.pending}</p>
                        </div>
                        <Clock className="w-8 h-8 text-yellow-500/20" />
                    </div>
                </Tooltip>
                <Tooltip content={i18n.t('oversight.approved_tooltip')} position="top" class_name="w-full">
                    <div className="bg-zinc-900 border border-zinc-800 p-4 rounded-lg flex items-center justify-between cursor-help w-full">
                        <div>
                            <p className="text-zinc-400 text-sm">{i18n.t('oversight.approved_label')}</p>
                            <p className="text-2xl font-bold text-green-400">{stats.approved}</p>
                        </div>
                        <CheckCircle className="w-8 h-8 text-green-500/20" />
                    </div>
                </Tooltip>
                <Tooltip content={i18n.t('oversight.rejected_tooltip')} position="top" class_name="w-full">
                    <div className="bg-zinc-900 border border-zinc-800 p-4 rounded-lg flex items-center justify-between cursor-help w-full">
                        <div>
                            <p className="text-zinc-400 text-sm">{i18n.t('oversight.rejected_label')}</p>
                            <p className="text-2xl font-bold text-red-400">{stats.rejected}</p>
                        </div>
                        <XCircle className="w-8 h-8 text-red-500/20" />
                    </div>
                </Tooltip>
                <Tooltip content={i18n.t('oversight.halt_agents_tooltip')} position="top" class_name="w-full">
                    <button
                        onClick={handle_kill_switch}
                        className={`p-4 rounded-lg flex items-center justify-center gap-2 font-bold transition-colors cursor-pointer group border w-full ${is_online ? 'bg-amber-500/10 hover:bg-amber-500/20 border-amber-500/50 text-amber-400' : 'bg-zinc-800/10 border-zinc-700/50 text-zinc-600 opacity-50'}`}
                        disabled={!is_online}
                    >
                        <Shield className="w-5 h-5 group-hover:scale-110 transition-transform" />
                        {is_online ? i18n.t('oversight.halt_agents_button') : i18n.t('oversight.offline_label')}
                    </button>
                </Tooltip>
                <Tooltip content={i18n.t('oversight.kill_engine_tooltip')} position="top" class_name="w-full">
                    <button
                        onClick={handle_kill_engine}
                        className={`p-4 rounded-lg flex items-center justify-center gap-2 font-bold transition-colors cursor-pointer group border w-full ${is_online ? 'bg-red-600/10 hover:bg-red-600/20 border-red-600/50 text-red-500' : 'bg-zinc-800/10 border-zinc-700/50 text-zinc-600 opacity-50'}`}
                        disabled={!is_online}
                    >
                        <WifiOff className="w-5 h-5 group-hover:scale-110 transition-transform" />
                        {is_online ? i18n.t('oversight.kill_engine_button') : i18n.t('oversight.offline_label')}
                    </button>
                </Tooltip>

                {/* Row 2 (Partial) */}
                <div className="md:col-start-4 md:col-span-2">
                    <Tooltip content={i18n.t('oversight.security_dashboard_tooltip')} position="top" class_name="w-full">
                        <button
                            onClick={() => navigate('/security')}
                            className="w-full p-4 bg-zinc-950 border border-zinc-700 hover:border-green-500/50 hover:bg-green-500/5 text-zinc-300 hover:text-green-400 rounded-lg flex items-center justify-center gap-2 font-bold transition-all group"
                        >
                            <ShieldCheck className="w-5 h-5 group-hover:scale-110 transition-transform text-green-500" />
                            {i18n.t('oversight.security_dashboard_button')}
                        </button>
                    </Tooltip>
                </div>
            </div>

            {/* Pending Queue */}
            {pending.length > 0 && (
                <div className="bg-zinc-900 border border-yellow-500/30 rounded-lg overflow-hidden">
                    <div className="bg-yellow-500/10 p-3 border-b border-yellow-500/20 flex items-center gap-2">
                        <AlertTriangle className="w-4 h-4 text-yellow-500" />
                        <h2 className="font-semibold text-yellow-100">{i18n.t('oversight.awaiting_approval_title', { count: filtered_pending.length })}</h2>
                    </div>
                    <div className="divide-y divide-zinc-800">
                        {filtered_pending.map(entry => (
                            <div key={entry.id} className="p-4 flex flex-col md:flex-row gap-4 items-start md:items-center justify-between animate-in fade-in slide-in-from-top-2">
                                <div className="space-y-1">
                                    <div className="flex items-center gap-2">
                                        <span className="text-xs font-mono bg-zinc-800 text-zinc-300 px-2 py-0.5 rounded">
                                            {entry.tool_call?.agent_id || entry.agent_id || i18n.t('oversight.unknown_agent')}
                                        </span>
                                        <span className="text-sm font-medium text-green-400 flex items-center gap-1">
                                            <TerminalIcon className="w-3 h-3" />
                                            {entry.tool_call?.skill || entry.skill || i18n.t('oversight.capability_proposal')}
                                        </span>
                                        <span className="text-xs text-zinc-500">
                                            {get_safe_date(entry)?.toLocaleTimeString() || '--:--:--'}
                                        </span>
                                    </div>
                                    <p className="text-zinc-300">{entry.tool_call?.description || entry.description || i18n.t('oversight.awaiting_authorization')}</p>
                                    <pre className="text-xs bg-black/50 p-2 rounded text-zinc-400 font-mono overflow-auto max-w-2xl">
                                        {JSON.stringify(entry.tool_call?.params || entry.params || {}, null, 2)}
                                    </pre>
                                </div>
                                <div className="flex gap-2 w-full md:w-auto">
                                    <Tooltip content={i18n.t('oversight.approve_action_tooltip')} position="top">
                                        <button
                                            onClick={() => handle_decide(entry.id, 'approved')}
                                            className="flex-1 md:flex-none bg-green-500/20 hover:bg-green-500/30 text-green-400 px-4 py-2 rounded border border-green-500/30 font-medium transition-colors"
                                        >
                                            {i18n.t('oversight.approve_button')}
                                        </button>
                                    </Tooltip>
                                    <Tooltip content={i18n.t('oversight.reject_action_tooltip')} position="top">
                                        <button
                                            onClick={() => handle_decide(entry.id, 'rejected')}
                                            className="flex-1 md:flex-none bg-red-500/20 hover:bg-red-500/30 text-red-400 px-4 py-2 rounded border border-red-500/30 font-medium transition-colors"
                                        >
                                            {i18n.t('oversight.reject_button')}
                                        </button>
                                    </Tooltip>
                                </div>
                            </div>
                        ))}
                    </div>
                </div>
            )}

            {/* Auto-Approved Observability Panel */}
            <Auto_Approved_Panel
                ledger={ledger}
                resolve_agent_name={(id) => {
                    const agent = all_agents.find(a => a.id === id);
                    return agent ? `${agent.name} (${id})` : (id || i18n.t('oversight.unknown_agent'));
                }}
            />

            {/* Action Ledger */}
            <Action_Ledger
                ledger={ledger}
                filter={filter}
                set_filter={set_filter}
                selected_cluster_id={selected_cluster_id}
                set_selected_cluster_id={set_selected_cluster_id}
                clusters={clusters || []}
                resolve_agent_name={(id) => {
                    const agent = all_agents.find(a => a.id === id);
                    return agent ? `${agent.name} (${id})` : (id || i18n.t('oversight.unknown_agent'));
                }}
            />

            {/* Swarm Intelligence / Alpha Reasoning (Option A Enhancement) */}
            <div className="bg-zinc-900 border border-green-500/30 rounded-lg overflow-hidden">
                <div className="bg-green-500/10 p-3 border-b border-green-500/20 flex items-center gap-2">
                    <Tooltip content={i18n.t('oversight.swarm_intel_tooltip')} position="right" class_name="w-full">
                        <Cpu className="w-4 h-4 text-green-400 cursor-help" />
                    </Tooltip>
                    <h2 className="font-semibold text-blue-100">{i18n.t('oversight.swarm_intel_title')}</h2>
                </div>
                <div className="p-6">
                    {Object.values(active_proposals || {}).length > 0 ? (
                        <div className="grid grid-cols-1 gap-6">
                            {Object.values(active_proposals || {}).map((proposal) => {
                                const cluster = (clusters || []).find(c => c.id === proposal.cluster_id);
                                return (
                                    <div key={proposal.cluster_id} className="bg-zinc-950 border border-zinc-800 rounded-xl p-4 space-y-4">
                                        <div className="flex justify-between items-center">
                                            <div className="flex items-center gap-2">
                                                <div className="w-2 h-2 rounded-full bg-green-500 animate-pulse" />
                                                <span className="text-xs font-bold text-zinc-300 uppercase tracking-widest">{cluster?.name || i18n.t('oversight.unknown_cluster')}</span>
                                            </div>
                                            <div className="flex items-center gap-2">
                                                <span className="text-[10px] text-zinc-600 font-mono">
                                                    {i18n.t('oversight.alpha_node_prefix', { alpha_id: cluster?.alpha_id ?? '?' })} • {get_safe_date(proposal)?.toLocaleTimeString() || '--:--:--'}
                                                </span>
                                                <div className="flex gap-2 ml-4">
                                                    <button
                                                        onClick={() => dismiss_proposal(proposal.cluster_id)}
                                                        className="text-[9px] px-2 py-0.5 bg-zinc-800 hover:bg-zinc-700 text-zinc-400 rounded border border-zinc-700 transition-colors uppercase font-bold"
                                                    >
                                                        {i18n.t('oversight.dismiss_button') || 'Dismiss'}
                                                    </button>
                                                    <button
                                                        onClick={() => handle_apply_proposal(proposal.cluster_id)}
                                                        className="text-[9px] px-2 py-0.5 bg-green-500/10 hover:bg-green-500/20 text-green-400 rounded border border-green-500/30 transition-colors uppercase font-bold"
                                                    >
                                                        {i18n.t('missions.btn_authorize')}
                                                    </button>
                                                </div>
                                            </div>
                                        </div>

                                        <div className="bg-black/40 p-3 rounded-lg border border-zinc-800/50">
                                            <div className="text-[9px] font-bold text-zinc-500 mb-2 uppercase tracking-wide flex items-center gap-2">
                                                <Activity size={10} /> {i18n.t('oversight.neural_trace_label')}
                                            </div>
                                            <p className="text-xs text-zinc-400 leading-relaxed font-mono whitespace-pre-wrap">
                                                {proposal.reasoning}
                                            </p>
                                        </div>

                                        <div className="space-y-2">
                                            <div className="text-[9px] font-bold text-zinc-500 uppercase tracking-wide">{i18n.t('oversight.proposed_reallocations_label')}</div>
                                            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
                                                {(proposal.changes || []).map(change => {
                                                    const agent = all_agents.find(a => a.id === change.agent_id);
                                                    return (
                                                        <div key={change.agent_id} className="p-2.5 bg-zinc-900 border border-zinc-800 rounded-lg flex flex-col gap-1.5">
                                                            <div className="flex justify-between items-start">
                                                                <span className="text-[10px] font-bold text-zinc-200">{agent?.name}</span>
                                                                <span className="text-[8px] px-1 rounded bg-zinc-800 text-zinc-500 font-mono uppercase">{i18n.t('oversight.mod_req_label')}</span>
                                                            </div>
                                                            <div className="space-y-1">
                                                                {change.proposed_role && (
                                                                    <div className="flex items-center gap-2 text-[9px]">
                                                                        <span className="text-zinc-600 uppercase">{i18n.t('oversight.role_label')}</span>
                                                                        <span className="text-green-400">{change.proposed_role}</span>
                                                                    </div>
                                                                )}
                                                                {change.proposed_model && (
                                                                    <div className="flex items-center gap-2 text-[9px]">
                                                                        <span className="text-zinc-600 uppercase">{i18n.t('oversight.model_label')}</span>
                                                                        <span className="text-green-400">{change.proposed_model}</span>
                                                                    </div>
                                                                )}
                                                                {change.added_skills && (
                                                                    <div className="flex items-center gap-2 text-[9px]">
                                                                        <span className="text-zinc-600 uppercase">{i18n.t('oversight.skills_label')}</span>
                                                                        <span className="text-emerald-400">+{change.added_skills.length}</span>
                                                                    </div>
                                                                )}
                                                            </div>
                                                        </div>
                                                    );
                                                })}
                                            </div>
                                        </div>
                                    </div>
                                );
                            })}
                        </div>
                    ) : (
                        <Tw_Empty_State
                            icon={<Plus size={32} />}
                            title={i18n.t('oversight.no_optimization_traces')}
                        />
                    )}
                </div>
            </div>

            {/* Neural Footprint Monitoring (Moved from Ops_Dashboard) */}
            <Command_Table />
        </div>
    );
}


// Metadata: [Oversight_Dashboard]
