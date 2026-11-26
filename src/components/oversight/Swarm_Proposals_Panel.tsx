/**
 * @docs ARCHITECTURE:Interface
 *
 * ### AI Assist Note
 * **Swarm Proposals Panel**: Displays Alpha-node swarm optimization reasoning traces and
 * proposed agent role/model/skill reallocations with Dismiss and Authorize controls.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Missing cluster metadata or failed agent registry mutation on proposal approval.
 * - **Telemetry Link**: Search `[Swarm_Proposals_Panel]` in UI logs.
 */

import React from 'react';
import { Activity, Cpu, Plus } from 'lucide-react';
import { Tw_Empty_State, Tooltip } from '../ui';
import { i18n } from '../../i18n';
import { get_safe_date } from '../../utils/date_utils';
import type { Agent } from '../../contracts/agent/domain';
import type { Mission_Cluster, Swarm_Proposal } from '../../stores/workspace_store';

export interface Swarm_Proposals_Panel_Props {
    clusters: Mission_Cluster[];
    active_proposals: Record<string, Swarm_Proposal>;
    all_agents: Agent[];
    on_dismiss_proposal: (cluster_id: string) => void;
    on_apply_proposal: (cluster_id: string) => Promise<void>;
}

export const Swarm_Proposals_Panel: React.FC<Swarm_Proposals_Panel_Props> = ({
    clusters,
    active_proposals,
    all_agents,
    on_dismiss_proposal,
    on_apply_proposal,
}) => {
    const proposals = Object.values(active_proposals || {});

    return (
        <div className="bg-zinc-900 border border-green-500/30 rounded-lg overflow-hidden">
            <div className="bg-green-500/10 p-3 border-b border-green-500/20 flex items-center gap-2">
                <Tooltip content={i18n.t('oversight.swarm_intel_tooltip')} position="right" class_name="w-full">
                    <Cpu className="w-4 h-4 text-green-400 cursor-help" />
                </Tooltip>
                <h2 className="font-semibold text-blue-100">{i18n.t('oversight.swarm_intel_title')}</h2>
            </div>
            <div className="p-6">
                {proposals.length > 0 ? (
                    <div className="grid grid-cols-1 gap-6">
                        {proposals.map((proposal) => {
                            const cluster = (clusters || []).find(c => c.id === proposal.cluster_id);
                            return (
                                <div key={proposal.cluster_id} className="bg-zinc-950 border border-zinc-800 rounded-xl p-4 space-y-4">
                                    <div className="flex justify-between items-center">
                                        <div className="flex items-center gap-2">
                                            <div className="w-2 h-2 rounded-full bg-green-500 animate-pulse" />
                                            <span className="text-xs font-bold text-zinc-300 uppercase tracking-widest">
                                                {cluster?.name || i18n.t('oversight.unknown_cluster')}
                                            </span>
                                        </div>
                                        <div className="flex items-center gap-2">
                                            <span className="text-[10px] text-zinc-500 font-mono">
                                                {i18n.t('oversight.alpha_node_prefix', { alpha_id: cluster?.alpha_id ?? '?' })} • {get_safe_date(proposal)?.toLocaleTimeString() || '--:--:--'}
                                            </span>
                                            <div className="flex gap-2 ml-4">
                                                <button
                                                    type="button"
                                                    onClick={() => on_dismiss_proposal(proposal.cluster_id)}
                                                    className="text-[9px] px-2 py-0.5 bg-zinc-800 hover:bg-zinc-700 text-zinc-400 rounded border border-zinc-700 transition-colors uppercase font-bold"
                                                >
                                                    {i18n.t('oversight.dismiss_button') || 'Dismiss'}
                                                </button>
                                                <button
                                                    type="button"
                                                    onClick={() => void on_apply_proposal(proposal.cluster_id)}
                                                    className="text-[9px] px-2 py-0.5 bg-green-500/10 hover:bg-green-500/20 text-green-400 rounded border border-green-500/30 transition-colors uppercase font-bold"
                                                >
                                                    {i18n.t('missions.btn_authorize')}
                                                </button>
                                            </div>
                                        </div>
                                    </div>

                                    <div className="bg-zinc-900/60 p-3 rounded-lg border border-zinc-800/50">
                                        <div className="text-[9px] font-bold text-zinc-500 mb-2 uppercase tracking-wide flex items-center gap-2">
                                            <Activity size={10} /> {i18n.t('oversight.neural_trace_label')}
                                        </div>
                                        <p className="text-xs text-zinc-400 leading-relaxed font-mono whitespace-pre-wrap">
                                            {proposal.reasoning}
                                        </p>
                                    </div>

                                    <div className="space-y-2">
                                        <div className="text-[9px] font-bold text-zinc-500 uppercase tracking-wide">
                                            {i18n.t('oversight.proposed_reallocations_label')}
                                        </div>
                                        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
                                            {(proposal.changes || []).map(change => {
                                                const agent = all_agents.find(a => a.id === change.agent_id);
                                                return (
                                                    <div key={change.agent_id} className="p-2.5 bg-zinc-900 border border-zinc-800 rounded-lg flex flex-col gap-1.5">
                                                        <div className="flex justify-between items-start">
                                                            <span className="text-[10px] font-bold text-zinc-200">{agent?.name}</span>
                                                            <span className="text-[8px] px-1 rounded bg-zinc-800 text-zinc-500 font-mono uppercase">
                                                                {i18n.t('oversight.mod_req_label')}
                                                            </span>
                                                        </div>
                                                        <div className="space-y-1">
                                                            {change.proposed_role && (
                                                                <div className="flex items-center gap-2 text-[9px]">
                                                                    <span className="text-zinc-500 uppercase">{i18n.t('oversight.role_label')}</span>
                                                                    <span className="text-green-400">{change.proposed_role}</span>
                                                                </div>
                                                            )}
                                                            {change.proposed_model && (
                                                                <div className="flex items-center gap-2 text-[9px]">
                                                                    <span className="text-zinc-500 uppercase">{i18n.t('oversight.model_label')}</span>
                                                                    <span className="text-green-400">{change.proposed_model}</span>
                                                                </div>
                                                            )}
                                                            {change.added_skills && (
                                                                <div className="flex items-center gap-2 text-[9px]">
                                                                    <span className="text-zinc-500 uppercase">{i18n.t('oversight.skills_label')}</span>
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
    );
};

// Metadata: [Swarm_Proposals_Panel]
