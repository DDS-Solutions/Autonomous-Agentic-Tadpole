/**
 * @docs ARCHITECTURE:Core
 *
 * ### AI Assist Note
 * **LedgerSection**: Core technical resource for the Tadpole OS infrastructure.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI errors or callback stack traces.
 * - **Telemetry Link**: Search `[LedgerSection]` in console logs.
 */

/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **UI Component**: Agent status ledger monitor, context packet configurer, and skill subscription manager.
 * Allows viewing living automation states, modifying context packets, and managing/approving skill scope approvals.
 */

import React, { useState, useEffect, useCallback } from 'react';
import { Activity, RefreshCw, Save, ShieldCheck, CheckCircle2, AlertTriangle, Play, Database } from 'lucide-react';
import { tadpole_os_service } from '../../services/tadpoleos_service';
import { i18n } from '../../i18n';

interface SkillSubscription {
    id: string;
    agentId: string;
    skillId: string;
    approvedAt?: number | null;
    scopeHash: string;
    subscriptionStatus: string;
    installedAt?: number | null;
    lastUpdatedAt?: number | null;
    notes?: string | null;
}

interface StatusLedger {
    agentId: string;
    agentCode: string;
    runtime?: string;
    automationState: string;
    lastHeartbeat?: number;
    lastQueueResult: string;
    lastTaskId?: string;
    lastSuccessfulRun?: number;
    contextVersion: number;
    contextPacket: Record<string, unknown>;
    subscribedSkills: unknown;
    notes?: string;
}

interface LedgerSectionProps {
    agentId: string;
    allSkills: string[];
    themeColor: string;
}

export function LedgerSection({ agentId, allSkills, themeColor }: LedgerSectionProps) {
    const [ledger, setLedger] = useState<StatusLedger | null>(null);
    const [subscriptions, setSubscriptions] = useState<SkillSubscription[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [isSavingContext, setIsSavingContext] = useState(false);
    const [isApproving, setIsApproving] = useState<string | null>(null);
    const [isSubscribing, setIsSubscribing] = useState(false);
    const [errorMessage, setErrorMessage] = useState<string | null>(null);
    const [successMessage, setSuccessMessage] = useState<string | null>(null);

    // Edit context fields
    const [contextVersionInput, setContextVersionInput] = useState('1');
    const [contextPacketJson, setContextPacketJson] = useState('{}');

    // Subscribe skill fields
    const [selectedSkill, setSelectedSkill] = useState('');
    const [subscribeNotes, setSubscribeNotes] = useState('');

    const loadLedgerData = useCallback(async () => {
        setIsLoading(true);
        setErrorMessage(null);
        try {
            const data = await tadpole_os_service.get_status_ledger(agentId);
            setLedger(data);
            setContextVersionInput(data.contextVersion.toString());
            setContextPacketJson(JSON.stringify(data.contextPacket, null, 2));

            const subs = await tadpole_os_service.get_subscribed_skills(agentId);
            setSubscriptions(subs);
        } catch (err) {
            setErrorMessage(err instanceof Error ? err.message : 'Failed to load status ledger data.');
        } finally {
            setIsLoading(false);
        }
    }, [agentId]);

    useEffect(() => {
        // eslint-disable-next-line react-hooks/set-state-in-effect
        loadLedgerData();
    }, [loadLedgerData]);

    const handleSaveContext = async () => {
        setErrorMessage(null);
        setSuccessMessage(null);
        setIsSavingContext(true);
        try {
            const version = parseInt(contextVersionInput, 10);
            if (isNaN(version)) {
                throw new Error('Context version must be a valid integer.');
            }
            let packetObj = {};
            try {
                packetObj = JSON.parse(contextPacketJson);
            } catch (e) {
                throw new Error('Context packet must be a valid JSON object.', { cause: e });
            }

            await tadpole_os_service.update_context_packet(agentId, version, packetObj);
            setSuccessMessage('Context packet updated successfully.');
            // Reload ledger
            const updatedLedger = await tadpole_os_service.get_status_ledger(agentId);
            setLedger(updatedLedger);
        } catch (err) {
            setErrorMessage(err instanceof Error ? err.message : 'Failed to save context packet.');
        } finally {
            setIsSavingContext(false);
        }
    };

    const handleApproveSkill = async (skillId: string) => {
        setErrorMessage(null);
        setSuccessMessage(null);
        setIsApproving(skillId);
        try {
            await tadpole_os_service.approve_skill(agentId, skillId);
            setSuccessMessage(`Skill ${skillId} approved successfully.`);
            const subs = await tadpole_os_service.get_subscribed_skills(agentId);
            setSubscriptions(subs);
        } catch (err) {
            setErrorMessage(err instanceof Error ? err.message : 'Failed to approve skill.');
        } finally {
            setIsApproving(null);
        }
    };

    const handleSubscribeSkill = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!selectedSkill) return;
        setErrorMessage(null);
        setSuccessMessage(null);
        setIsSubscribing(true);
        try {
            await tadpole_os_service.subscribe_skill(agentId, selectedSkill, subscribeNotes || undefined);
            setSuccessMessage(`Subscribed to skill ${selectedSkill} successfully.`);
            setSelectedSkill('');
            setSubscribeNotes('');
            const subs = await tadpole_os_service.get_subscribed_skills(agentId);
            setSubscriptions(subs);
        } catch (err) {
            setErrorMessage(err instanceof Error ? err.message : 'Failed to subscribe to skill.');
        } finally {
            setIsSubscribing(false);
        }
    };

    // Skills available to subscribe to (not yet subscribed)
    const unsubscribedSkills = allSkills.filter(
        skillId => !subscriptions.some(sub => sub.skillId === skillId)
    );

    const formatTimestamp = (ts?: number | null) => {
        if (!ts) return 'Never';
        return new Date(ts * 1000).toLocaleString();
    };

    return (
        <div className="flex-1 overflow-y-auto custom-scrollbar p-6 space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-500">
            {/* Header */}
            <div className="flex justify-between items-center">
                <div className="space-y-1">
                    <h3 className="text-xs font-bold text-zinc-400 uppercase tracking-[0.3em]">
                        {i18n.t('agent_config.tab_ledger')}
                    </h3>
                    <p className="text-[10px] text-zinc-500 font-mono uppercase tracking-wider">
                        Operational health status ledger & scope verification
                    </p>
                </div>
                <button
                    onClick={loadLedgerData}
                    disabled={isLoading}
                    className="p-2 rounded-lg bg-zinc-900 border border-zinc-800 text-zinc-400 hover:text-white hover:border-zinc-700 transition-colors disabled:opacity-50"
                    title="Refresh Ledger"
                >
                    <RefreshCw size={14} className={isLoading ? 'animate-spin' : ''} />
                </button>
            </div>

            {/* Error & Success Alerts */}
            {errorMessage && (
                <div className="p-4 rounded-xl bg-red-500/10 border border-red-500/20 text-red-400 text-xs flex items-start gap-3">
                    <AlertTriangle size={16} className="shrink-0 mt-0.5" />
                    <div>{errorMessage}</div>
                </div>
            )}
            {successMessage && (
                <div className="p-4 rounded-xl bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 text-xs flex items-start gap-3">
                    <CheckCircle2 size={16} className="shrink-0 mt-0.5" />
                    <div>{successMessage}</div>
                </div>
            )}

            {/* Grid of Living Metrics */}
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                <div className="bg-zinc-900/40 border border-zinc-800/50 rounded-2xl p-5 space-y-2">
                    <div className="text-[10px] text-zinc-500 font-bold uppercase tracking-wider">Automation Status</div>
                    <div className="flex items-center gap-2">
                        <Activity size={16} style={{ color: themeColor }} />
                        <span className="text-sm font-bold text-zinc-200 capitalize">
                            {ledger?.automationState || 'manual'}
                        </span>
                    </div>
                </div>

                <div className="bg-zinc-900/40 border border-zinc-800/50 rounded-2xl p-5 space-y-2">
                    <div className="text-[10px] text-zinc-500 font-bold uppercase tracking-wider">Last Heartbeat</div>
                    <div className="text-xs font-mono text-zinc-300">
                        {formatTimestamp(ledger?.lastHeartbeat)}
                    </div>
                </div>

                <div className="bg-zinc-900/40 border border-zinc-800/50 rounded-2xl p-5 space-y-2">
                    <div className="text-[10px] text-zinc-500 font-bold uppercase tracking-wider">Queue Result</div>
                    <div className="text-xs font-semibold">
                        <span className={`px-2 py-0.5 rounded text-[10px] font-mono ${
                            ledger?.lastQueueResult === 'success' 
                                ? 'bg-emerald-500/10 text-emerald-400' 
                                : ledger?.lastQueueResult === 'none' 
                                    ? 'bg-zinc-800 text-zinc-400' 
                                    : 'bg-amber-500/10 text-amber-400'
                        }`}>
                            {ledger?.lastQueueResult || 'none'}
                        </span>
                    </div>
                </div>

                <div className="bg-zinc-900/40 border border-zinc-800/50 rounded-2xl p-5 space-y-2">
                    <div className="text-[10px] text-zinc-500 font-bold uppercase tracking-wider">Runtime Host</div>
                    <div className="text-xs font-mono text-zinc-300 truncate">
                        {ledger?.runtime || 'Not Reporting'}
                    </div>
                </div>
            </div>

            {/* Context Packet Management */}
            <div className="bg-zinc-900/40 border border-zinc-800/50 rounded-2xl p-6 space-y-4">
                <div className="flex items-center gap-3">
                    <div className="p-2 bg-blue-500/10 text-blue-400 rounded-xl">
                        <Database size={16} />
                    </div>
                    <div>
                        <h4 className="text-sm font-bold text-zinc-200">Context Packet Ledger</h4>
                        <p className="text-xs text-zinc-500">Living variables and context constraints injected into execution runs</p>
                    </div>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
                    <div className="space-y-1">
                        <label className="text-[10px] text-zinc-400 font-bold uppercase tracking-wider">Context Version</label>
                        <input
                            type="number"
                            value={contextVersionInput}
                            onChange={(e) => setContextVersionInput(e.target.value)}
                            className="w-full bg-zinc-950 border border-zinc-800 rounded-lg px-3 py-2 text-xs font-mono text-zinc-200 focus:outline-none focus:border-zinc-700"
                        />
                    </div>
                    <div className="md:col-span-3 space-y-1">
                        <label className="text-[10px] text-zinc-400 font-bold uppercase tracking-wider">Variables JSON</label>
                        <textarea
                            value={contextPacketJson}
                            onChange={(e) => setContextPacketJson(e.target.value)}
                            rows={6}
                            className="w-full bg-zinc-950 border border-zinc-800 rounded-lg p-3 text-xs font-mono text-zinc-200 focus:outline-none focus:border-zinc-700 custom-scrollbar"
                            placeholder="{}"
                        />
                    </div>
                </div>

                <div className="flex justify-end">
                    <button
                        onClick={handleSaveContext}
                        disabled={isSavingContext || isLoading}
                        className="flex items-center gap-2 px-4 py-2 rounded-lg bg-blue-600 hover:bg-blue-700 text-white text-xs font-semibold transition-colors disabled:opacity-50"
                    >
                        <Save size={14} />
                        {isSavingContext ? 'Saving...' : 'Save Context Packet'}
                    </button>
                </div>
            </div>

            {/* Skill Subscriptions & Reapproval Gate */}
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                {/* List Subscribed Skills */}
                <div className="lg:col-span-2 bg-zinc-900/40 border border-zinc-800/50 rounded-2xl p-6 space-y-4">
                    <div className="flex items-center gap-3">
                        <div className="p-2 bg-emerald-500/10 text-emerald-400 rounded-xl">
                            <ShieldCheck size={16} />
                        </div>
                        <div>
                            <h4 className="text-sm font-bold text-zinc-200">Skill Subscription approvals</h4>
                            <p className="text-xs text-zinc-500">Security-scoped authorizations matching code SHA-256 manifests</p>
                        </div>
                    </div>

                    <div className="space-y-3">
                        {subscriptions.length === 0 ? (
                            <div className="text-center py-6 text-xs text-zinc-500 italic">No skill subscriptions active.</div>
                        ) : (
                            subscriptions.map((sub) => {
                                const isPending = sub.subscriptionStatus === 'pending' || sub.subscriptionStatus === 'pending_reapproval';
                                return (
                                    <div 
                                        key={sub.id} 
                                        className="flex flex-col md:flex-row md:items-center justify-between p-4 bg-zinc-950/50 border border-zinc-900 rounded-xl gap-4 hover:border-zinc-800 transition-colors"
                                    >
                                        <div className="space-y-1">
                                            <div className="flex items-center gap-2">
                                                <span className="text-xs font-bold text-zinc-200 font-mono">{sub.skillId}</span>
                                                <span className={`px-2 py-0.5 rounded text-[8px] font-black uppercase tracking-wider ${
                                                    sub.subscriptionStatus === 'approved'
                                                        ? 'bg-emerald-500/10 text-emerald-400'
                                                        : sub.subscriptionStatus === 'pending_reapproval'
                                                            ? 'bg-amber-500/10 text-amber-400 border border-amber-500/20'
                                                            : 'bg-zinc-800 text-zinc-400'
                                                }`}>
                                                    {sub.subscriptionStatus.replace('_', ' ')}
                                                </span>
                                            </div>
                                            <div className="text-[10px] text-zinc-500 font-mono flex flex-col gap-0.5">
                                                <span className="truncate max-w-xs md:max-w-md">Approved Hash: <span className="text-zinc-400">{sub.scopeHash.substring(0, 16)}...</span></span>
                                                {sub.approvedAt && <span>Approved At: {formatTimestamp(sub.approvedAt)}</span>}
                                                {sub.notes && <span className="text-zinc-400 italic">Note: "{sub.notes}"</span>}
                                            </div>
                                        </div>

                                        {isPending && (
                                            <button
                                                onClick={() => handleApproveSkill(sub.skillId)}
                                                disabled={isApproving !== null}
                                                className="px-3 py-1.5 rounded-lg bg-emerald-600 hover:bg-emerald-700 text-white text-[10px] font-bold uppercase tracking-wider transition-colors disabled:opacity-50 self-end md:self-center"
                                            >
                                                {isApproving === sub.skillId ? 'Approving...' : 'Approve Scope'}
                                            </button>
                                        )}
                                    </div>
                                );
                            })
                        )}
                    </div>
                </div>

                {/* Subscribe to New Skill */}
                <div className="bg-zinc-900/40 border border-zinc-800/50 rounded-2xl p-6 space-y-4 h-fit">
                    <h4 className="text-sm font-bold text-zinc-200">Subscribe to New Skill</h4>
                    <p className="text-xs text-zinc-500 leading-relaxed">
                        Add a skill from the ecosystem. It will register in a pending state until its security permissions are approved.
                    </p>

                    <form onSubmit={handleSubscribeSkill} className="space-y-4">
                        <div className="space-y-1">
                            <label className="text-[10px] text-zinc-400 font-bold uppercase tracking-wider">Select Skill</label>
                            {unsubscribedSkills.length === 0 ? (
                                <div className="text-xs text-zinc-500 italic p-2 bg-zinc-950 border border-zinc-900 rounded-lg">
                                    All available skills subscribed
                                </div>
                            ) : (
                                <select
                                    value={selectedSkill}
                                    onChange={(e) => setSelectedSkill(e.target.value)}
                                    className="w-full bg-zinc-950 border border-zinc-850 rounded-lg px-3 py-2 text-xs text-zinc-300 focus:outline-none focus:border-zinc-700"
                                    required
                                >
                                    <option value="">-- Choose a Skill --</option>
                                    {unsubscribedSkills.map(skillId => (
                                        <option key={skillId} value={skillId}>{skillId}</option>
                                    ))}
                                </select>
                            )}
                        </div>

                        <div className="space-y-1">
                            <label className="text-[10px] text-zinc-400 font-bold uppercase tracking-wider">Oversight Notes (Optional)</label>
                            <input
                                type="text"
                                value={subscribeNotes}
                                onChange={(e) => setSubscribeNotes(e.target.value)}
                                placeholder="Explain subscription reason..."
                                className="w-full bg-zinc-950 border border-zinc-850 rounded-lg px-3 py-2 text-xs text-zinc-300 focus:outline-none focus:border-zinc-700"
                            />
                        </div>

                        <button
                            type="submit"
                            disabled={isSubscribing || !selectedSkill}
                            className="w-full flex items-center justify-center gap-2 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-white text-xs font-semibold transition-colors disabled:opacity-50"
                        >
                            <Play size={12} style={{ color: themeColor }} />
                            Subscribe Skill
                        </button>
                    </form>
                </div>
            </div>
        </div>
    );
}

// Metadata: [LedgerSection]
