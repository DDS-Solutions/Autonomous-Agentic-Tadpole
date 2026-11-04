/**
 * @docs ARCHITECTURE:Interface
 *
 * ### AI Assist Note
 * Shared scheduled-job form and history views, with a fresh default form-state factory.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Invalid job edits or incomplete run-history data.
 * - **Telemetry Link**: Search [Scheduled_Jobs] in UI logs.
 */

import React, { useCallback } from 'react';
import { Activity, CheckCircle2, AlertCircle, Clock } from 'lucide-react';
import type { Scheduled_Job, Scheduled_Job_Run, Workflow_Entry } from '../../services/tadpoleos_service';
import { Tooltip } from '../ui';
import { Cron_Config_Mini_Panel } from './Cron_Config_Mini_Panel';
import { describe_cron } from '../../utils/cron_builder';
import { i18n } from '../../i18n';
import { get_safe_date } from '../../utils/date_utils';

export function create_empty_job_config(): job_config_state {
    return {
        name: '',
        agent_id: '',
        workflow_id: null,
        prompt: '',
        cron_expr: '0 * * * *',
        budget_usd: 0.10,
        max_failures: 3
    };
}

// --- TYPE DEFINITIONS ---

export interface job_config_state {
    name: string;
    agent_id: string;
    workflow_id: string | null;
    prompt: string;
    cron_expr: string;
    budget_usd: number;
    max_failures: number;
}

// --- Component: Job_Form_Manager ---

interface job_form_manager_props {
    job_type: 'agent' | 'workflow';
    set_job_type: (type: 'agent' | 'workflow') => void;
    current_job_config: job_config_state;
    set_job_config: React.Dispatch<React.SetStateAction<job_config_state>>;
    target_search: string;
    set_target_search: React.Dispatch<React.SetStateAction<string>>;
    filtered_agents: Array<{ id: string; name: string; role: string }>;
    filtered_workflows: Array<Workflow_Entry & { type: 'continuity' | 'passive' }>;
    is_creating: boolean;
    set_creating: React.Dispatch<React.SetStateAction<boolean>>;
    editing_job_id: string | null;
    set_editing_job_id: React.Dispatch<React.SetStateAction<string | null>>;
    handle_job_submit: (e: React.FormEvent) => Promise<void>;
}

/**
 * Component: Job_Form_Manager
 * Handles the configuration and submission of new automated jobs.
 * Supports both single-agent missions and multi-step workflows.
 */
export const Job_Form_Manager: React.FC<job_form_manager_props> = ({ 
    job_type, 
    set_job_type,
    current_job_config, 
    set_job_config, 
    target_search, 
    set_target_search, 
    filtered_agents, 
    filtered_workflows, 
    set_creating,
    editing_job_id,
    set_editing_job_id,
    handle_job_submit
}) => {
    const handle_input_change = useCallback((key: keyof job_config_state, value: string | number | boolean | string[] | null) => {
        set_job_config(prev => ({ ...prev, [key]: value }));
    }, [set_job_config]);

    return (
        <div className="bg-zinc-900 border border-green-500/30 rounded-xl p-6 shadow-2xl max-h-[85vh] overflow-y-auto custom-scrollbar">
            <div className="flex justify-between items-center mb-4 sticky top-0 bg-zinc-900 z-10 pb-2">
                <h2 className="text-lg font-bold text-green-400 font-mono uppercase">
                    {editing_job_id ? i18n.t('scheduled_jobs.modify_config') : i18n.t('scheduled_jobs.configure_new')}
                </h2>
                <div className="flex bg-zinc-950 p-1 rounded-lg border border-zinc-800">
                    <button
                        onClick={() => { 
                            set_job_type('agent'); 
                            set_target_search('');
                            set_job_config(prev => ({ ...prev, workflow_id: null }));
                        }}
                        className={`px-3 py-1 text-[10px] font-bold rounded ${job_type === 'agent' ? 'bg-green-600 text-white' : 'text-zinc-500 hover:text-zinc-300'}`}
                    >
                        {i18n.t('scheduled_jobs.single_agent')}
                    </button>
                    <button
                        onClick={() => { 
                            set_job_type('workflow'); 
                            set_target_search('');
                        }}
                        className={`px-3 py-1 text-[10px] font-bold rounded ${job_type === 'workflow' ? 'bg-green-600 text-white' : 'text-zinc-500 hover:text-zinc-300'}`}
                    >
                        {i18n.t('scheduled_jobs.multi_step_workflow')}
                    </button>
                </div>
            </div>
            <form onSubmit={handle_job_submit} className="space-y-4">
                {/* Job Name */}
                <div>
                    <Tooltip content={i18n.t('scheduled_jobs.job_name_tooltip')} position="top">
                        <label htmlFor="job_name" className="block text-[10px] font-bold font-mono text-zinc-400 uppercase tracking-widest mb-1.5 cursor-help">{i18n.t('scheduled_jobs.job_name')}</label>
                    </Tooltip>
                    <input 
                        id="job_name" required 
                        type="text" 
                        className="w-full bg-zinc-950 border border-zinc-800 rounded-lg p-2.5 text-xs font-mono text-zinc-100 placeholder:text-zinc-600 focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/30 outline-none transition-all" 
                        value={current_job_config.name} 
                        onChange={e => handle_input_change('name', e.target.value)} 
                        placeholder={i18n.t('scheduled_jobs.job_name_placeholder')} 
                    />
                </div>
                
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4 min-w-0">
                    {/* Target Selector */}
                    <div className="min-w-0">
                        {job_type === 'agent' ? (
                            <>
                                <Tooltip content={i18n.t('scheduled_jobs.target_agent_tooltip')} position="top">
                                    <label htmlFor="target_agent" className="block text-[10px] font-bold font-mono text-zinc-400 uppercase tracking-widest mb-1.5 cursor-help">{i18n.t('scheduled_jobs.target_agent')}</label>
                                </Tooltip>
                                <div className="space-y-2">
                                    <input 
                                        type="text" 
                                        placeholder={i18n.t('scheduled_jobs.filter_agents')} 
                                        className="w-full bg-zinc-950 border border-zinc-800 rounded-lg p-2 text-xs font-mono text-zinc-100 placeholder:text-zinc-600 focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/30 outline-none transition-all" 
                                        value={target_search}
                                        onChange={e => set_target_search(e.target.value)}
                                    />
                                    <select 
                                        id="target_agent" 
                                        required 
                                        className="w-full bg-zinc-950 border border-zinc-800 rounded-lg p-2 text-xs font-mono text-zinc-100 focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/30 outline-none transition-all" 
                                        value={current_job_config.agent_id || ''} 
                                        onChange={e => {
                                            handle_input_change('agent_id', e.target.value);
                                            handle_input_change('workflow_id', null);
                                        }}
                                    >
                                        <option value="">{i18n.t('scheduled_jobs.select_agent')}</option>
                                        {filtered_agents.map(a => <option key={a.id} value={a.id}>{a.name} ({a.role})</option>)}
                                    </select>
                                </div>
                            </>
                        ) : (
                            <>
                                <Tooltip content={i18n.t('scheduled_jobs.target_workflow_tooltip')} position="top">
                                    <label htmlFor="target_workflow" className="block text-[10px] font-bold font-mono text-zinc-400 uppercase tracking-widest mb-1.5 cursor-help">{i18n.t('scheduled_jobs.target_workflow')}</label>
                                </Tooltip>
                                <div className="space-y-2">
                                    <input 
                                        type="text" 
                                        placeholder={i18n.t('scheduled_jobs.filter_workflows')} 
                                        className="w-full bg-zinc-950 border border-zinc-800 rounded-lg p-2 text-xs font-mono text-zinc-100 placeholder:text-zinc-600 focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/30 outline-none transition-all" 
                                        value={target_search}
                                        onChange={(e) => set_target_search(e.target.value)}
                                    />
                                    <select 
                                        id="target_workflow" 
                                        required 
                                        className="w-full bg-zinc-950 border border-zinc-800 rounded-lg p-2 text-xs font-mono text-zinc-100 focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/30 outline-none transition-all" 
                                        value={current_job_config.workflow_id || ''} 
                                        onChange={e => {
                                            handle_input_change('workflow_id', e.target.value || null);
                                        }}
                                    >
                                        <option value="">{i18n.t('scheduled_jobs.select_workflow')}</option>
                                        <optgroup label={i18n.t('scheduled_jobs.mission_sequences')} className="bg-zinc-900 text-green-400 font-mono text-[10px]">
                                            {filtered_workflows.filter(w => w.type === 'continuity').map(w => (
                                                <option key={w.id} value={w.id} className="text-zinc-300 bg-zinc-950">{w.name}</option>
                                            ))}
                                            {filtered_workflows.filter(w => w.type === 'continuity').length === 0 && (
                                                <option value="" disabled className="text-zinc-600 bg-zinc-950">No executable continuity workflows found</option>
                                            )}
                                        </optgroup>
                                        <optgroup label={i18n.t('scheduled_jobs.guiding_protocols')} className="bg-zinc-900 text-amber-400 font-mono text-[10px]">
                                            {filtered_workflows.filter(w => w.type === 'passive').map(w => (
                                                <option key={w.id} value={w.id} disabled className="text-zinc-600 bg-zinc-950">{w.name} (Guidance Only)</option>
                                            ))}
                                        </optgroup>
                                    </select>
                                </div>
                            </>
                        )}
                    </div>

                    {/* Schedule Helper Mini-Panel (Right Column) */}
                    <div className="min-w-0">
                        <Cron_Config_Mini_Panel 
                            current_cron={current_job_config.cron_expr}
                            on_cron_change={(new_cron) => handle_input_change('cron_expr', new_cron)}
                        />
                    </div>
                </div>

                {/* Cron Expression */}
                <div>
                    <Tooltip content={i18n.t('scheduled_jobs.cron_tooltip')} position="top">
                        <label htmlFor="cron_expr" className="block text-[10px] font-bold font-mono text-zinc-400 uppercase tracking-widest mb-1.5 cursor-help">{i18n.t('scheduled_jobs.cron_expression')}</label>
                    </Tooltip>
                    <input 
                        id="cron_expr" required 
                        type="text" 
                        className="w-full bg-zinc-950 border border-zinc-800 rounded-lg p-2.5 text-xs font-mono text-zinc-100 placeholder:text-zinc-600 focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/30 outline-none transition-all" 
                        value={current_job_config.cron_expr} 
                        onChange={e => handle_input_change('cron_expr', e.target.value)} 
                        placeholder={i18n.t('scheduled_jobs.cron_placeholder')} 
                    />
                    {/* Live Human-Readable Schedule Preview */}
                    <div className="mt-1.5 flex items-center gap-1.5 text-xs text-green-400 font-mono">
                        <Clock size={12} className="text-green-500 shrink-0" />
                        <span className="text-[11px] text-zinc-400">
                            {i18n.t('scheduled_jobs.estimated_schedule')}: <strong className="text-green-400 font-semibold">{describe_cron(current_job_config.cron_expr)}</strong>
                        </span>
                    </div>
                </div>
                
                {/* Mission Prompt (if Agent job) */}
                {job_type === 'agent' && (
                    <div>
                        <Tooltip content={i18n.t('scheduled_jobs.mission_prompt_tooltip')} position="top">
                            <label htmlFor="mission_prompt" className="block text-[10px] font-bold font-mono text-zinc-400 uppercase tracking-widest mb-1.5 cursor-help">{i18n.t('scheduled_jobs.mission_prompt')}</label>
                        </Tooltip>
                        <textarea 
                            id="mission_prompt" 
                            required 
                            rows={3} 
                            className="w-full bg-zinc-950 border border-zinc-800 rounded-lg p-2.5 text-xs font-mono text-zinc-100 placeholder:text-zinc-600 focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/30 outline-none transition-all resize-none custom-scrollbar" 
                            value={current_job_config.prompt} 
                            onChange={e => handle_input_change('prompt', e.target.value)} 
                            placeholder={i18n.t('scheduled_jobs.mission_prompt_placeholder')} 
                        />
                    </div>
                )}

                {/* Budget and Max Failures */}
                <div className="grid grid-cols-2 gap-4">
                    <div>
                        <Tooltip content={i18n.t('scheduled_jobs.budget_cap_tooltip')} position="top">
                            <label htmlFor="budget_usd" className="block text-[10px] font-bold font-mono text-zinc-400 uppercase tracking-widest mb-1.5 cursor-help">{i18n.t('scheduled_jobs.budget_cap')}</label>
                        </Tooltip>
                        <input 
                            id="budget_usd" 
                            type="number" step="0.01" 
                            className="w-full bg-zinc-950 border border-zinc-800 rounded-lg p-2 text-xs font-mono text-zinc-100 focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/30 outline-none transition-all" 
                            value={current_job_config.budget_usd} 
                            onChange={e => handle_input_change('budget_usd', parseFloat(e.target.value) || 0)} 
                        />
                    </div>
                    <div>
                        <Tooltip content={i18n.t('scheduled_jobs.max_failures_tooltip')} position="top">
                            <label htmlFor="max_failures" className="block text-[10px] font-bold font-mono text-zinc-400 uppercase tracking-widest mb-1.5 cursor-help">{i18n.t('scheduled_jobs.max_failures_label')}</label>
                        </Tooltip>
                        <input 
                            id="max_failures" 
                            type="number" 
                            className="w-full bg-zinc-950 border border-zinc-800 rounded-lg p-2 text-xs font-mono text-zinc-100 focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/30 outline-none transition-all" 
                            value={current_job_config.max_failures} 
                            onChange={e => handle_input_change('max_failures', parseInt(e.target.value, 10) || 0)} 
                        />
                    </div>
                </div>
                
                {/* Submission Buttons */}
                <div className="flex gap-2 pt-2 border-t border-zinc-800">
                    <button type="submit" className="px-5 py-2.5 bg-emerald-600 hover:bg-emerald-500 rounded-lg text-xs font-bold text-white shadow-lg shadow-emerald-500/20 uppercase tracking-wider transition-all">
                        {editing_job_id ? i18n.t('scheduled_jobs.update_sync') : i18n.t('scheduled_jobs.save_job')}
                    </button>
                    <button type="button" onClick={() => {
                        set_creating(false);
                        set_editing_job_id(null);
                    }} className="px-5 py-2.5 bg-zinc-800 hover:bg-zinc-700 rounded-lg text-xs font-bold text-zinc-300 uppercase tracking-wider transition-all">
                        {i18n.t('common.btn_cancel')}
                    </button>
                </div>
            </form>
        </div>
    );
};

// --- Component: Job_History_View ---

interface job_history_view_props {
    job: Scheduled_Job;
    runs: Scheduled_Job_Run[] | undefined;
}

/**
 * Component: Job_History_View
 * Displays the execution log for a specific job, including cost and mission ID links.
 */
export const Job_History_View: React.FC<job_history_view_props> = ({ job, runs }) => {
    return (
        <div className="p-6 ml-12 border-l-2 border-zinc-800 my-4 space-y-4">
            <div className="bg-black/20 rounded p-4 font-mono text-xs text-zinc-400 border border-zinc-900/50">
                <span className="text-zinc-600 block mb-2 uppercase tracking-widest text-[9px]">{i18n.t('scheduled_jobs.mission_prompt')}</span>
                {job.prompt || i18n.t('common.no_data')}
            </div>
    
            <h4 className="font-mono text-[10px] text-zinc-500 uppercase tracking-widest pt-2">{i18n.t('scheduled_jobs.run_history')}</h4>
            {!runs ? (
                <div className="text-zinc-600 text-xs flex items-center gap-2">
                    <Activity className="animate-pulse" size={14} /> {i18n.t('scheduled_jobs.fetching_history')}
                </div>
            ) : runs.length === 0 ? (
                <div className="text-zinc-600 text-xs italic">{i18n.t('scheduled_jobs.no_history')}</div>
            ) : (
                <div className="space-y-2">
                    {runs.map(run => (
                        <div key={run.id} className="flex items-center justify-between bg-zinc-950 p-2 rounded border border-zinc-800 text-xs font-mono group">
                            <div className="flex items-center gap-4">
                                {run.status === 'completed' ? <CheckCircle2 size={14} className="text-emerald-500" /> : <AlertCircle size={14} className="text-rose-500" />}
                                <span className="text-zinc-400">{get_safe_date(run)?.toLocaleString() || i18n.t('common.unknown')}</span>
                                <span className={`${run.status === 'completed' ? 'text-emerald-400' : 'text-rose-400'}`}>{run.status.toUpperCase()}</span>
                            </div>
                            <div className="flex items-center gap-4 text-zinc-500">
                                <span>${run.cost_usd.toFixed(4)}</span>
                                <span className="text-zinc-700 group-hover:text-amber-500/50 transition-colors w-16 truncate max-w-[8rem] text-right" title={run.mission_id || ''}>
                                    {run.mission_id || i18n.t('scheduled_jobs.no_mission')}
                                </span>
                            </div>
                        </div>
                    ))}
                </div>
            )}
        </div>
    );
}

