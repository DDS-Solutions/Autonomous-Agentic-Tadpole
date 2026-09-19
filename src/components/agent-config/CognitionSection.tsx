/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **UI Component**: Neural orchestration hub for model slot management. 
 * Controls the active slot lifecycle (Pause/Resume) and facilitates sub-navigation between primary, secondary, and tertiary cognition layers.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: State flicker when switching active tabs while an agent is 'thinking', model list empty due to provider API failure, or 'suspended' status not reflecting in the LED indicator.
 * - **Telemetry Link**: Search for `[CognitionSection]` or `status_active` in UI tracing.
 */

import { Pause, Play, Shield, Globe, Award } from 'lucide-react';
import { ModelSlotConfig } from './ModelSlotConfig';
import { i18n } from '../../i18n';
import type { Model_Entry, Provider_Config } from '../../stores/provider_store';
import type { Skill_Manifest } from '../../services/tadpoleos_service';
import type { Skill_Definition, Mcp_Tool_Hub_Definition } from '../../stores/skill_store';
import type { Agent_Model_Slot_Key, Agent_Model_Slot_State } from '../../types';

interface CognitionSectionProps {
    activeTab: Agent_Model_Slot_Key;
    slots: Record<Agent_Model_Slot_Key, Agent_Model_Slot_State>;
    agentStatus: string;
    providers: Provider_Config[];
    models: Model_Entry[];
    allSkills: string[];
    allWorkflows: string[];
    manifests: Skill_Manifest[];
    scripts: Skill_Definition[];
    mcpTools: Mcp_Tool_Hub_Definition[];
    themeColor: string;
    activeModelSlot: 1 | 2 | 3;
    onSetTab: (tab: Agent_Model_Slot_Key) => void;
    onSetActiveSlot?: (slot: 1 | 2 | 3) => void;
    onUpdateSlotField: <K extends keyof Agent_Model_Slot_State>(slot: Agent_Model_Slot_Key, field: K, value: Agent_Model_Slot_State[K]) => void;
    onToggleSkill: (slot: Agent_Model_Slot_Key, kind: 'skills' | 'workflows', value: string) => void;
    onProviderChange: (slot: Agent_Model_Slot_Key, val: string) => void;
    onPause: () => void;
    onResume: () => void;
}

/**
 * Cognition_Section
 * Handles the cognitive configuration of an agent, including model slots and core logic.
 * Manages the high-level operational state and model orchestration.
 */
export function CognitionSection({
    activeTab,
    slots,
    agentStatus,
    providers,
    models,
    allSkills,
    allWorkflows,
    manifests,
    scripts,
    mcpTools,
    themeColor,
    activeModelSlot,
    onSetTab,
    onSetActiveSlot,
    onUpdateSlotField,
    onToggleSkill,
    onProviderChange,
    onPause,
    onResume
}: CognitionSectionProps) {
    const isPaused = agentStatus === 'suspended';
    const isOffline = agentStatus === 'offline';
    const statusColor = isPaused
        ? 'bg-amber-500 shadow-amber-500/50'
        : isOffline
            ? 'bg-zinc-500 shadow-zinc-500/50'
            : 'bg-emerald-500 shadow-emerald-500/50';
    const statusLabel = isPaused
        ? i18n.t('agent_config.status_suspended')
        : isOffline
            ? 'OFFLINE'
            : i18n.t('agent_config.status_active');
    const currentSlotIdx = activeTab === 'primary' ? 1 : activeTab === 'secondary' ? 2 : 3;
    const isCurrentSlotActive = activeModelSlot === currentSlotIdx;

    const renderTabButton = (id: Agent_Model_Slot_Key, label: string, icon: React.ReactNode) => {
        const slotIdx = id === 'primary' ? 1 : id === 'secondary' ? 2 : 3;
        const isActiveForAgent = activeModelSlot === slotIdx;

        return (
            <div
                onClick={() => onSetTab(id)}
                role="tab"
                tabIndex={0}
                onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        onSetTab(id);
                    }
                }}
                className={`flex-1 flex flex-col items-center gap-1.5 py-3 rounded-xl border transition-all relative overflow-hidden group cursor-pointer ${activeTab === id ? 'bg-zinc-800 border-zinc-700 shadow-lg' : 'bg-transparent border-transparent text-zinc-600 hover:text-zinc-400 hover:bg-zinc-800/30'}`}
            >
                {activeTab === id && (
                    <div 
                        className="absolute top-0 left-0 w-full h-0.5" 
                        style={{ background: `linear-gradient(to right, transparent, ${themeColor}80, transparent)` }}
                    />
                )}
                
                {/* Active Slot LED Indicator */}
                <button
                    type="button"
                    onClick={(e) => {
                        e.stopPropagation();
                        onSetActiveSlot?.(slotIdx);
                    }}
                    title={isActiveForAgent ? i18n.t('agent_config.status_active') : `Activate Slot ${slotIdx}`}
                    aria-label={isActiveForAgent ? i18n.t('agent_config.status_active') : `Activate Slot ${slotIdx}`}
                    className={`absolute top-2 right-2 flex items-center justify-center p-1 rounded-full transition-all cursor-pointer ${isActiveForAgent ? 'opacity-100 hover:scale-110' : 'opacity-40 hover:opacity-100 hover:bg-zinc-700/50'}`}
                >
                    <div 
                        className={`w-2 h-2 rounded-full transition-all ${isActiveForAgent ? 'shadow-[0_0_8px_rgba(79,209,197,0.8)] scale-110' : 'bg-zinc-600'}`}
                        style={{ backgroundColor: isActiveForAgent ? themeColor : undefined }}
                    />
                </button>

                <div className="flex items-center gap-1.5">
                    <span className={activeTab === id ? 'text-zinc-100' : 'text-zinc-500 group-hover:text-zinc-300'}>
                        {icon}
                    </span>
                    <span className={`text-[11px] font-bold uppercase tracking-wider ${activeTab === id ? 'text-zinc-100' : 'text-zinc-500 group-hover:text-zinc-300'}`}>
                        {label}
                    </span>
                </div>
            </div>
        );
    };

    return (
        <div className="space-y-6">
            {/* Slot Tabs */}
            <div className="space-y-3">
                <div className="flex gap-2 p-1 bg-zinc-900/50 border border-zinc-800/80 rounded-2xl">
                    {renderTabButton('primary', i18n.t('agent_config.slot_primary'), <Shield size={14} />)}
                    {renderTabButton('secondary', i18n.t('agent_config.slot_secondary'), <Globe size={14} />)}
                    {renderTabButton('tertiary', i18n.t('agent_config.slot_tertiary'), <Award size={14} />)}
                </div>

                {/* Slot Details Card */}
                <div className="p-4 bg-zinc-900/30 border border-zinc-800/40 rounded-2xl space-y-4">
                    <div className="flex items-center justify-between pb-3 border-b border-zinc-800/40">
                        <div className="flex items-center gap-2">
                            <span className="text-xs font-bold text-zinc-400 capitalize">
                                {activeTab} {i18n.t('agent_config.slot_cognition')}
                            </span>
                            {isCurrentSlotActive ? (
                                <span 
                                    className="text-[9px] uppercase px-1.5 py-0.2 rounded border font-mono font-bold"
                                    style={{ 
                                        color: themeColor, 
                                        borderColor: `${themeColor}40`,
                                        backgroundColor: `${themeColor}10` 
                                    }}
                                >
                                    Current Active Slot
                                </span>
                            ) : (
                                <button
                                    type="button"
                                    onClick={() => onSetActiveSlot?.(currentSlotIdx)}
                                    className="text-[9px] uppercase px-2 py-0.5 rounded border border-emerald-500/30 text-emerald-400 hover:bg-emerald-500/10 transition-colors cursor-pointer"
                                >
                                    Activate Slot
                                </button>
                            )}
                        </div>
                        <div className="flex items-center gap-1.5">
                            <div className={`w-1.5 h-1.5 rounded-full ${isOffline ? '' : 'animate-pulse'} shadow-[0_0_8px] ${statusColor}`} />
                            <span className="text-[10px] font-bold text-zinc-300 uppercase tracking-[0.2em]">{statusLabel}</span>
                            <div className="h-4 w-px bg-zinc-800 mx-1.5" />
                             <button
                            onClick={isPaused || isOffline ? onResume : onPause}
                            aria-label={isPaused || isOffline ? i18n.t('agent_config.btn_resume') : i18n.t('agent_config.btn_pause')}
                            title={isPaused || isOffline ? i18n.t('agent_config.btn_resume') : i18n.t('agent_config.btn_pause')}
                            className={`p-1.5 rounded-lg transition-all ${isPaused || isOffline ? 'bg-emerald-500/10 text-emerald-500 hover:bg-emerald-500/20' : 'bg-amber-500/10 text-amber-500 hover:bg-amber-500/20'}`}
                        >
                            {isPaused || isOffline ? <Play size={14} /> : <Pause size={14} />}
                        </button>
                        </div>
                    </div>

                    <ModelSlotConfig
                        slotKey={activeTab}
                        slot={slots[activeTab]}
                        providers={providers}
                        models={models}
                        allSkills={allSkills}
                        allWorkflows={allWorkflows}
                        manifests={manifests}
                        scripts={scripts}
                        mcpTools={mcpTools}
                        themeColor={themeColor}
                        onUpdateField={(field, value) => onUpdateSlotField(activeTab, field, value)}
                        onToggleCapability={(kind: 'skills' | 'workflows', value: string) => onToggleSkill(activeTab, kind, value)}
                        onProviderChange={(val: string) => onProviderChange(activeTab, val)}
                    />
                </div>
            </div>
        </div>
    );
}


// Metadata: [Cognition_Section]



// Metadata: [CognitionSection]
