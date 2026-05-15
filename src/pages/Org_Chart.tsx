/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **Root View**: Visual hierarchy for the agent swarm organization. 
 * Orchestrates the rendering of reporting structures and team/departmental relationships.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Recursive rendering loop for circular reporting structures, or node occlusion on large swarms.
 * - **Telemetry Link**: Search for `[Org_Chart]` or `HIERARCHY_RENDER` in UI logs.
 */

/**
 * Org_Chart
 * The Neural Command Hierarchy page.
 * Renders a multi-level agent organization chart with real-time status updates,
 * dynamic pulsating connection animations, and integrated node configuration. 
 * Refactored for strict snake_case compliance for backend parity.
 */
import { useState, useEffect, useMemo, memo } from 'react';
import { resolve_provider, resolve_technical_model_id } from '../utils/model_utils';
import type { Agent } from '../types';
import { Hierarchy_Node } from '../components/Hierarchy_Node';
import AgentConfigPanel from '../components/AgentConfigPanel';
import { agents as mock_agents } from '../data/mock_agents';
import { use_workspace_store } from '../stores/workspace_store';
import { use_dropdown_store } from '../stores/dropdown_store';
import { use_agent_store } from '../stores/agent_store';
import { i18n } from '../i18n';
import { LD_Json } from '../components/ui/LD_Json';

export default function Org_Chart() {
    const { clusters } = use_workspace_store();
    const { agents: agents_list, fetch_agents, update_agent: store_update_agent } = use_agent_store();
    const dropdown_open_id = use_dropdown_store(s => s.open_id);
    const close_dropdowns = use_dropdown_store(s => s.close_dropdown);

    const [config_agent_id, set_config_agent_id] = useState<string | null>(null);

    const available_roles = useMemo(() =>
        Array.from(new Set(mock_agents.map(a => a.role))).sort()
        , []);

    useEffect(() => {
        if (agents_list.length === 0) {
            fetch_agents();
        }
    }, [agents_list.length, fetch_agents]);

    // ── Handlers ──────────────────────────────────────────────
    const handle_skill_trigger = (agent_id: string, skill: string) => {
        store_update_agent(agent_id, { 
            status: 'active' as const, 
            current_task: `⚡ Executing: ${skill}...` 
        });
    };

    const handle_role_change = (agent_id: string, new_role: string) => {
        handle_agent_update(agent_id, { role: new_role });
    };

    const handle_agent_update = (agent_id: string, updates: Partial<Agent>) => {
        store_update_agent(agent_id, updates);
    };

    const handle_model_change = (agent_id: string, new_model: string) => {
        const model_id = resolve_technical_model_id(new_model);
        const provider = resolve_provider(model_id);
        handle_agent_update(agent_id, { model: new_model, model_config: { modelId: model_id, provider } });
    };
 
    const handle_model_2_change = (agent_id: string, new_model: string) => {
        const model_id = resolve_technical_model_id(new_model);
        const provider = resolve_provider(model_id);
        handle_agent_update(agent_id, { model_2: new_model, model_config2: { modelId: model_id, provider } });
    };
 
    const handle_model_3_change = (agent_id: string, new_model: string) => {
        const model_id = resolve_technical_model_id(new_model);
        const provider = resolve_provider(model_id);
        handle_agent_update(agent_id, { model_3: new_model, model_config3: { modelId: model_id, provider } });
    };

    // ── Data Partitioning (Dynamic Hierarchy Builder) ────────────────────────
    const hierarchy_data = useMemo(() => {
        if (agents_list.length === 0) return null;
        const combined = [...agents_list];
        const cmd_cluster = clusters.find(c => c.id === 'cl-command');
        const roots = combined.filter(a => !a.reports_to || (cmd_cluster && cmd_cluster.collaborators.includes(a.id)));
        const alpha = roots.find(a => a.role.toLowerCase().includes('coordinator') || a.role.toLowerCase().includes('alpha')) || roots[0] || combined[0];
        const mission_clusters = clusters.filter(c => c.id !== 'cl-command');
        const chains = mission_clusters.map(cluster => ({
            id: cluster.id,
            name: cluster.name,
            theme: cluster.theme,
            alpha_id: cluster.alpha_id,
            objective: cluster.objective,
            is_active: cluster.is_active,
            agents: cluster.collaborators.filter(cid => cid !== alpha.id).map(cid => combined.find(a => a.id === cid)).filter((a): a is Agent => !!a)
        }));

        const chains_count = chains.length;
        const path_d = chains_count > 0 ? `M 500 0 L 500 40 ${chains.map((_, i) => {
            const x = 500 + (i - (chains_count - 1) * 0.5) * 400;
            return `M 500 40 L ${x} 40 L ${x} 62`;
        }).join(' ')}` : '';

        return { alpha, chains, path_d };
    }, [agents_list, clusters]);

    if (!hierarchy_data) {
        return (
            <div className="h-full flex items-center justify-center text-zinc-500 animate-pulse font-mono text-xs uppercase tracking-widest">
                {i18n.t('org_chart.label_initializing')}
            </div>
        );
    }

    return (
        <div className="h-full flex flex-col bg-zinc-950">
            <LD_Json data={{
              "@context": "https://schema.org",
              "@type": "SoftwareApplication",
              "name": "Tadpole OS Organization Chart",
              "description": "Dynamic visual hierarchy and reporting structure for the autonomous agent swarm. Real-time command-and-control visualization.",
              "author": { "@type": "Organization", "name": "Sovereign Engineering" },
              "applicationCategory": "Organization Management",
              "operatingSystem": "Tadpole OS"
            }} />
            <h1 className="sr-only">Tadpole OS Neural Command Hierarchy & Swarm Organization</h1>

            <div 
                className="flex-1 overflow-auto p-8 custom-scrollbar relative" 
                onClick={close_dropdowns}
                onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && close_dropdowns()}
                tabIndex={-1}
                role="presentation"
            >
                <div className="neural-grid" />

                <div className="min-w-max pt-1 pb-12 px-12 flex flex-col items-center gap-12 relative">

                    {/* Level 1: Root Command (Alpha) */}
                    <div className={`relative group w-[350px] ${dropdown_open_id === hierarchy_data.alpha?.id ? 'z-[100]' : 'z-30'}`}>
                        <div className="mb-4 text-center">
                            <h3 className="text-[10px] font-bold uppercase tracking-[0.2em] mb-1 text-green-400">
                                {i18n.t('org_chart.label_command_chain')}
                            </h3>
                            <p className="text-[9px] text-zinc-500 font-medium">{i18n.t('org_chart.label_strategic_command')}</p>
                        </div>
                        <div className="absolute -inset-4 bg-green-500/10 blur-2xl rounded-full opacity-0 group-hover:opacity-100 transition-opacity duration-700" />
                        {(() => {
                            const cluster = clusters.find(c => c.collaborators.includes(hierarchy_data.alpha?.id));
                            return (
                                <Hierarchy_Node
                                    is_root
                                    is_alpha
                                    available_roles={available_roles}
                                    on_role_change={handle_role_change}
                                    agent={hierarchy_data.alpha}
                                    theme_color="blue"
                                    is_active={cluster?.is_active}
                                    mission_objective={cluster?.objective}
                                    on_skill_trigger={handle_skill_trigger}
                                    on_configure_click={(id) => set_config_agent_id(id)}
                                    on_model_change={handle_model_change}
                                    on_model_2_change={handle_model_2_change}
                                    on_model_3_change={handle_model_3_change}
                                    on_update={handle_agent_update}
                                />
                            );
                        })()}

                        {/* Connection SVG to Mission Chains */}
                        {hierarchy_data.path_d && (
                            <svg 
                                aria-hidden="true"
                                className="absolute top-[100%] left-1/2 -translate-x-1/2 w-[1000px] h-[62px] overflow-visible pointer-events-none"
                            >
                                <path
                                    d={hierarchy_data.path_d}
                                    fill="none"
                                    stroke="rgba(16, 185, 129, 0.3)"
                                    strokeWidth="1.5"
                                    className={hierarchy_data.chains.some(c => c.is_active) ? 'neural-pulse text-green-500' : ''}
                                />
                            </svg>
                        )}
                    </div>

                    {/* Level 2: Active Mission Chains (Dynamic Grid) */}
                    <div className={`flex gap-16 relative mt-4 ${hierarchy_data.chains.some(c => c.agents.some(a => a.id === dropdown_open_id)) ? 'z-[100]' : 'z-10'}`}>
                        {hierarchy_data.chains.map(chain => (
                            <Agent_Chain
                                key={chain.id}
                                chain={chain}
                                dropdown_open_id={dropdown_open_id}
                                available_roles={available_roles}
                                handle_role_change={handle_role_change}
                                handle_skill_trigger={handle_skill_trigger}
                                set_config_agent_id={set_config_agent_id}
                                handle_agent_update={handle_agent_update}
                                clusters={clusters}
                                handle_model_change={handle_model_change}
                                handle_model_2_change={handle_model_2_change}
                                handle_model_3_change={handle_model_3_change}
                            />
                        ))}
                    </div>

                    {/* Agent Config Panel Overlay */}
                    {config_agent_id && (() => {
                            const agent = agents_list.find(a => a.id === config_agent_id) || mock_agents.find(a => a.id === config_agent_id);
                            return agent ? (
                                <AgentConfigPanel
                                    agent={agent as Agent}
                                    onClose={() => set_config_agent_id(null)}
                                    onUpdate={handle_agent_update}
                                />
                            ) : null;
                        })()}

                    {/* Overlay Indicators */}
                    <div className="fixed bottom-8 right-8 flex flex-col gap-2 items-end">
                        <div className="px-3 py-1 bg-black/60 border border-zinc-800 rounded-full backdrop-blur-md flex items-center gap-2 shadow-2xl">
                            <div className="w-1.5 h-1.5 rounded-full bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.5)]" />
                            <span className="text-[10px] font-mono text-zinc-400 uppercase tracking-widest">
                                {i18n.t('org_chart.label_swarm_active')}
                            </span>
                        </div>
                    </div>

                </div>
            </div>
        </div>
    );
}

/**
 * Agent_Chain
 * Optimized sub-component for rendering an individual agent chain.
 */
const Agent_Chain = memo(({
    chain,
    dropdown_open_id,
    available_roles,
    handle_role_change,
    handle_skill_trigger,
    set_config_agent_id,
    handle_agent_update,
    clusters,
    handle_model_change,
    handle_model_2_change,
    handle_model_3_change
}: {
    chain: {
        id: string;
        name: string;
        theme: string;
        alpha_id?: string;
        objective?: string;
        is_active?: boolean;
        agents: Agent[];
    },
    dropdown_open_id: string | null,
    available_roles: string[],
    handle_role_change: (id: string, role: string) => void,
    handle_skill_trigger: (id: string, skill: string) => void,
    set_config_agent_id: (id: string) => void,
    handle_agent_update: (id: string, updates: Partial<Agent>) => void,
    handle_model_change: (id: string, m: string) => void,
    handle_model_2_change: (id: string, m: string) => void,
    handle_model_3_change: (id: string, m: string) => void,
    clusters: { id: string, name: string, theme: string, alpha_id?: string, collaborators: string[], is_active?: boolean }[]
}) => {
    return (
        <div className="flex flex-col items-center gap-12 relative">
            <div className="mb-4 text-center">
                <h3 className={`text-[10px] font-bold uppercase tracking-[0.2em] mb-1 
                ${chain.theme === 'cyan' ? 'text-cyan-400' :
                        chain.theme === 'zinc' ? 'text-zinc-400' : 'text-amber-400'}`}>
                    Chain {chain.id}
                </h3>
                <p className="text-[9px] text-zinc-500 font-medium">{chain.name}</p>
            </div>

            <div className="flex flex-col gap-12 relative">
                {chain.agents.map((agent: Agent, idx: number) => (
                    <div key={agent.id} className="relative w-[350px]" style={{ zIndex: dropdown_open_id === agent.id ? 110 : (100 - idx) }}>
                        <Hierarchy_Node
                            agent={agent}
                            available_roles={available_roles}
                            on_role_change={handle_role_change}
                            theme_color={chain.theme}
                            is_alpha={agent.id === chain.alpha_id}
                            is_active={clusters.find(c => c.id === chain.id)?.is_active}
                            mission_objective={chain.objective}
                            on_skill_trigger={handle_skill_trigger}
                            on_configure_click={(id) => set_config_agent_id(id)}
                            on_model_change={handle_model_change}
                            on_model_2_change={handle_model_2_change}
                            on_model_3_change={handle_model_3_change}
                            on_update={handle_agent_update}
                        />

                        {idx < chain.agents.length - 1 && (() => {
                            const is_pulse = chain.is_active || (chain.agents[idx].status !== 'offline' && chain.agents[idx].status !== 'idle') || (chain.agents[idx + 1].status !== 'offline' && chain.agents[idx + 1].status !== 'idle');
                            const pulse_class = is_pulse ? `vertical-pulse text-${chain.theme}-500` : '';
                            const theme_bg = chain.theme === 'cyan' ? 'bg-cyan-500/20' : chain.theme === 'zinc' ? 'bg-zinc-500/20' : 'bg-amber-500/20';

                            return (
                                <div 
                                    aria-hidden="true"
                                    className={`absolute top-full left-1/2 -translate-x-1/2 h-12 w-px ${theme_bg} ${pulse_class}`}
                                />
                            );
                        })()}
                    </div>
                ))}
            </div>
        </div>
    );
});


// Metadata: [Org_Chart]
