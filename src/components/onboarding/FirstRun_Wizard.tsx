/**
 * @docs ARCHITECTURE:Interface
 * @docs design:Components
 *
 * ### AI Assist Note
 * **First-Run Overlord Wizard**: 4-step Day-1 onboarding modal built strictly with Neural Glass
 * Zinc tokens (`docs/design.md`). Guides fresh operators from engine handshake and memory-only
 * session token entry (`INV-SEC-002`) through 1-click Model Tier Presets, safety posture,
 * and a verified pre-flight readiness check.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Engine unreachable or 401 token mismatch; provides actionable inline diagnostics without fake data.
 * - **Telemetry Link**: Search `[FirstRun_Wizard]` in browser logs.
 */

import React, { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { CheckCircle2, Cpu, KeyRound, Rocket, Shield, Sparkles, X } from 'lucide-react';
import {
    use_settings_store,
    set_session_api_key,
    is_valid_api_key,
    type ModelTierPreset,
} from '../../stores/settings_store';
import { tadpole_os_service } from '../../services/tadpoleos_service';
import { Readiness_Card } from '../model/Readiness_Card';
import { Z_INDEX_MAP } from '../ui/theme_tokens';

interface FirstRun_Wizard_Props {
    is_open: boolean;
    on_close: () => void;
    on_launch_starter_mission?: () => void;
}

const TIER_PRESETS: Array<{
    id: ModelTierPreset;
    title: string;
    badge: string;
    summary: string;
    max_depth: number;
    budget_usd: number;
    privacy_mode: boolean;
}> = [
    {
        id: 'local_sovereign',
        title: 'Local Sovereign',
        badge: 'OFFLINE · 8–16GB',
        summary: 'Local Ollama models across all slots, depth capped at 2 for crisp single-machine latency.',
        max_depth: 2,
        budget_usd: 2.0,
        privacy_mode: true,
    },
    {
        id: 'hybrid_overlord',
        title: 'Hybrid Overlord',
        badge: 'RECOMMENDED',
        summary: 'Cloud frontier model for CEO/COO planning paired with local Ollama specialists & cascade routing.',
        max_depth: 4,
        budget_usd: 5.0,
        privacy_mode: false,
    },
    {
        id: 'cloud_max',
        title: 'Full Cloud Max Autonomy',
        badge: 'DEEP DAG · BUDGET CAPPED',
        summary: 'Cloud frontier models across all tiers with strict per-mission USD caps and full oversight.',
        max_depth: 5,
        budget_usd: 10.0,
        privacy_mode: false,
    },
];

export const FirstRun_Wizard: React.FC<FirstRun_Wizard_Props> = ({
    is_open,
    on_close,
    on_launch_starter_mission,
}) => {
    const settings = use_settings_store((s) => s.settings);
    const update_setting = use_settings_store((s) => s.update_setting);

    const [step, set_step] = useState<1 | 2 | 3 | 4>(1);
    const [token_input, set_token_input] = useState(settings.tadpole_os_api_key || '');
    const [conn_status, set_conn_status] = useState<'idle' | 'checking' | 'ok' | 'error'>('idle');
    const [conn_message, set_conn_message] = useState('');
    const [selected_preset, set_selected_preset] = useState<ModelTierPreset>(
        settings.active_tier_preset || 'hybrid_overlord'
    );
    const [safe_mode, set_safe_mode] = useState(settings.is_safe_mode);
    const [auto_safe_skills, set_auto_safe_skills] = useState(settings.auto_approve_safe_skills);
    const [budget_usd, set_budget_usd] = useState(settings.default_budget_usd || 5.0);
    const [budget_draft, set_budget_draft] = useState(String(settings.default_budget_usd || 5.0));
    const [has_custom_budget, set_has_custom_budget] = useState(false);
    const [is_syncing_guardrails, set_is_syncing_guardrails] = useState(false);
    const [sync_notice, set_sync_notice] = useState<{ kind: 'ok' | 'warn'; text: string } | null>(null);
    const was_open_ref = useRef(false);

    const clamp_budget = (raw: string, fallback: number): number => {
        if (raw.trim() === '') return Math.max(0.1, fallback);
        const parsed = Number(raw);
        if (!Number.isFinite(parsed)) return Math.max(0.1, fallback);
        return Math.max(0.1, parsed);
    };

    // Reset transient modal step/status whenever the wizard is freshly opened
    useEffect(() => {
        if (is_open && !was_open_ref.current) {
            const current = use_settings_store.getState().settings;
            const initial_budget = current.default_budget_usd || 5.0;
            set_step(1);
            set_token_input(current.tadpole_os_api_key || '');
            set_conn_status('idle');
            set_conn_message('');
            set_selected_preset(current.active_tier_preset || 'hybrid_overlord');
            set_safe_mode(current.is_safe_mode);
            set_auto_safe_skills(current.auto_approve_safe_skills);
            set_budget_usd(initial_budget);
            set_budget_draft(String(initial_budget));
            set_has_custom_budget(false);
            set_sync_notice(null);
        }
        was_open_ref.current = is_open;
    }, [is_open]);

    if (!is_open) return null;

    const handle_verify_connection = async () => {
        const trimmed = token_input.trim();
        if (!trimmed) {
            set_conn_status('error');
            set_conn_message('Paste your NEURAL_TOKEN (or NEURAL_TOKEN_ADMIN if tiered tokens are enabled) from .env first.');
            return;
        }
        if (!is_valid_api_key(trimmed)) {
            set_conn_status('error');
            set_conn_message('Token must be at least 12 valid characters (run `npm run init` to generate one in .env).');
            return;
        }
        set_conn_status('checking');
        set_conn_message('');

        const base_url = (settings.tadpole_os_url || 'http://127.0.0.1:8000').replace(/\/$/, '');
        try {
            // 1. Check engine reachability on the public health endpoint WITHOUT mutating session token yet
            const healthy = await tadpole_os_service.check_health();
            if (!healthy) {
                set_conn_status('error');
                set_conn_message(`Engine unreachable at ${base_url}. Start it with \`npm run engine\` or \`start_AA_tadpole.bat\`.`);
                return;
            }

            // 2. Verify candidate token directly against /v1/agents before committing it to RAM/WS store
            if (typeof fetch === 'function') {
                const probe = await fetch(`${base_url}/v1/agents`, {
                    method: 'GET',
                    headers: { Authorization: `Bearer ${trimmed}` },
                });
                if (probe.status === 401 || probe.status === 403) {
                    set_conn_status('error');
                    set_conn_message(`Engine reachable, but token was rejected (HTTP ${probe.status}). Check NEURAL_TOKEN in .env.`);
                    return;
                }
                if (!probe.ok) {
                    set_conn_status('error');
                    set_conn_message(`Engine returned HTTP ${probe.status} during auth probe.`);
                    return;
                }
            }

            // 3. Only commit token to volatile RAM after server authentication succeeds (INV-SEC-002)
            set_session_api_key(trimmed);
            update_setting('tadpole_os_api_key', trimmed);
            await tadpole_os_service.get_agents();
            set_conn_status('ok');
            set_conn_message('Authenticated session established (token isolated in RAM).');
            console.debug('[FirstRun_Wizard] Handshake verified');
        } catch (err) {
            set_conn_status('error');
            set_conn_message(
                err instanceof Error
                    ? `Engine reachable, auth check failed: ${err.message}`
                    : 'Authentication failed. Check NEURAL_TOKEN in your .env file.'
            );
        }
    };

    const handle_apply_tier_and_continue = () => {
        const preset = TIER_PRESETS.find((p) => p.id === selected_preset) || TIER_PRESETS[1];
        update_setting('active_tier_preset', preset.id);
        update_setting('max_swarm_depth', preset.max_depth);
        update_setting('privacy_mode', preset.privacy_mode);
        if (!has_custom_budget) {
            set_budget_usd(preset.budget_usd);
            set_budget_draft(String(preset.budget_usd));
        }
        set_step(3);
    };

    const handle_save_safety_and_continue = async () => {
        const preset = TIER_PRESETS.find((p) => p.id === selected_preset) || TIER_PRESETS[1];
        const final_budget = clamp_budget(budget_draft, budget_usd);
        set_budget_usd(final_budget);
        set_budget_draft(String(final_budget));
        update_setting('is_safe_mode', safe_mode);
        update_setting('auto_approve_safe_skills', auto_safe_skills);
        update_setting('default_budget_usd', final_budget);

        set_is_syncing_guardrails(true);
        set_sync_notice(null);
        try {
            if (typeof tadpole_os_service.update_governance_settings === 'function') {
                await tadpole_os_service.update_governance_settings({
                    auto_approve_safe_skills: auto_safe_skills,
                    privacy_mode: preset.privacy_mode,
                    max_swarm_depth: preset.max_depth,
                    default_budget_usd: final_budget,
                });
                set_sync_notice({ kind: 'ok', text: 'Guardrails synced to live Rust engine (GovernanceHub).' });
            }
        } catch (err) {
            const msg = err instanceof Error ? err.message : 'Engine sync failed';
            set_sync_notice({
                kind: 'warn',
                text: `Saved locally, but Rust engine sync failed (${msg}). Verify an Admin token (NEURAL_TOKEN_ADMIN if tiered) is active in Step 1.`,
            });
        } finally {
            set_is_syncing_guardrails(false);
            set_step(4);
        }
    };

    const handle_finish = (launch_starter: boolean) => {
        update_setting('onboarding_completed', true);
        if (launch_starter && on_launch_starter_mission) {
            on_launch_starter_mission();
        }
        on_close();
    };

    const step_labels = ['1. Connect', '2. Model Tier', '3. Guardrails', '4. Pre-Flight'];

    return createPortal(
        <div
            className="fixed inset-0 flex items-center justify-center bg-zinc-950/80 backdrop-blur-md p-4"
            style={{ zIndex: Z_INDEX_MAP.dialog }}
            role="dialog"
            aria-modal="true"
            aria-labelledby="first-run-wizard-title"
        >
            <div
                className="w-full max-w-2xl rounded-xl border border-zinc-800 bg-zinc-900/95 backdrop-blur-md overflow-hidden"
                data-testid="first-run-wizard"
            >
                {/* Header */}
                <div className="flex items-center justify-between px-6 py-4 border-b border-zinc-800">
                    <div className="flex items-center gap-3">
                        <div className="w-9 h-9 rounded-lg bg-zinc-950 border border-zinc-800 flex items-center justify-center">
                            <Sparkles size={18} className="text-[#22c55e]" />
                        </div>
                        <div>
                            <p className="text-[10px] font-mono font-bold uppercase tracking-widest text-zinc-500">
                                DAY-1 OVERLORD SETUP
                            </p>
                            <h2 id="first-run-wizard-title" className="text-base font-semibold text-zinc-100">
                                Sovereign Command Initialization
                            </h2>
                        </div>
                    </div>
                    <button
                        type="button"
                        onClick={() => handle_finish(false)}
                        title="Skip setup wizard"
                        aria-label="Skip setup wizard"
                        className="p-1.5 rounded-lg text-zinc-500 hover:text-zinc-200 hover:bg-zinc-800 transition-colors focus:outline-none focus:ring-2 focus:ring-emerald-500"
                    >
                        <X size={16} />
                    </button>
                </div>

                {/* Progress Bar */}
                <div className="grid grid-cols-4 gap-2 px-6 py-3 bg-zinc-950/50 border-b border-zinc-800/80">
                    {step_labels.map((lbl, idx) => {
                        const s = (idx + 1) as 1 | 2 | 3 | 4;
                        const active = step === s;
                        const done = step > s;
                        return (
                            <button
                                key={lbl}
                                type="button"
                                onClick={() => set_step(s)}
                                className={`px-2 py-1.5 rounded-lg text-[10px] font-mono font-bold uppercase tracking-widest text-center border transition-colors ${
                                    active
                                        ? 'bg-zinc-800 text-zinc-100 border-zinc-700'
                                        : done
                                        ? 'bg-emerald-500/10 text-[#22c55e] border-emerald-500/20'
                                        : 'bg-zinc-900/60 text-zinc-500 border-zinc-800/60 hover:text-zinc-300'
                                }`}
                            >
                                {lbl}
                            </button>
                        );
                    })}
                </div>

                {/* Step Body */}
                <div className="p-6 space-y-5 max-h-[65vh] overflow-y-auto custom-scrollbar">
                    {step === 1 && (
                        <div className="space-y-4">
                            <div className="flex items-start gap-3">
                                <KeyRound size={18} className="text-zinc-300 mt-0.5 shrink-0" />
                                <div className="space-y-1">
                                    <h3 className="text-sm font-semibold text-zinc-100">Connect Engine & Session Token</h3>
                                    <p className="text-xs text-zinc-400">
                                        Enter your <code className="font-mono text-zinc-200">NEURAL_TOKEN</code> from <code className="font-mono text-zinc-200">.env</code> (generated via <code className="font-mono text-zinc-200">npm run init</code>). Held strictly in RAM—never persisted to browser storage.
                                    </p>
                                </div>
                            </div>

                            <div className="space-y-2">
                                <label htmlFor="wizard-neural-token" className="block text-[10px] font-mono font-bold uppercase tracking-widest text-zinc-400">
                                    Session Neural Token
                                </label>
                                <input
                                    id="wizard-neural-token"
                                    type="password"
                                    value={token_input}
                                    onChange={(e) => set_token_input(e.target.value)}
                                    placeholder="Paste NEURAL_TOKEN (memory-isolated)"
                                    aria-label="Session Neural Token"
                                    className="w-full px-3 py-2 rounded-lg bg-zinc-950 border border-zinc-800 text-sm font-mono text-zinc-100 placeholder:text-zinc-600 focus:outline-none focus:ring-2 focus:ring-emerald-500"
                                />
                            </div>

                            <div className="flex items-center gap-3">
                                <button
                                    type="button"
                                    onClick={() => void handle_verify_connection()}
                                    disabled={conn_status === 'checking'}
                                    className="px-3.5 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 border border-zinc-700 text-xs font-mono font-bold uppercase tracking-wider text-zinc-100 focus:outline-none focus:ring-2 focus:ring-emerald-500"
                                >
                                    {conn_status === 'checking' ? 'Verifying...' : 'Verify Handshake'}
                                </button>
                                {conn_status === 'ok' && (
                                    <span className="inline-flex items-center gap-1.5 text-xs font-mono text-[#22c55e]">
                                        <CheckCircle2 size={14} /> {conn_message}
                                    </span>
                                )}
                                {conn_status === 'error' && (
                                    <span className="text-xs font-mono text-rose-400">{conn_message}</span>
                                )}
                            </div>
                        </div>
                    )}

                    {step === 2 && (
                        <div className="space-y-4">
                            <div className="flex items-start gap-3">
                                <Cpu size={18} className="text-zinc-300 mt-0.5 shrink-0" />
                                <div className="space-y-1">
                                    <h3 className="text-sm font-semibold text-zinc-100">Choose Your Model Tier Preset</h3>
                                    <p className="text-xs text-zinc-400">
                                        Automatically calibrates swarm delegation depth, privacy posture, and daily budget cap.
                                    </p>
                                </div>
                            </div>

                            <div className="grid grid-cols-1 gap-3">
                                {TIER_PRESETS.map((preset) => {
                                    const active = selected_preset === preset.id;
                                    return (
                                        <button
                                            key={preset.id}
                                            type="button"
                                            onClick={() => set_selected_preset(preset.id)}
                                            className={`text-left p-4 rounded-xl border transition-colors focus:outline-none focus:ring-2 focus:ring-emerald-500 ${
                                                active
                                                    ? 'bg-zinc-800/80 border-emerald-500/60'
                                                    : 'bg-zinc-950/60 border-zinc-800 hover:border-zinc-700'
                                            }`}
                                        >
                                            <div className="flex items-center justify-between gap-2 mb-1">
                                                <span className="text-sm font-semibold text-zinc-100">{preset.title}</span>
                                                <span className="text-[10px] font-mono font-bold uppercase tracking-widest px-2 py-0.5 rounded-md bg-zinc-900 border border-zinc-700 text-zinc-300">
                                                    {preset.badge}
                                                </span>
                                            </div>
                                            <p className="text-xs text-zinc-400">{preset.summary}</p>
                                        </button>
                                    );
                                })}
                            </div>
                        </div>
                    )}

                    {step === 3 && (
                        <div className="space-y-4">
                            <div className="flex items-start gap-3">
                                <Shield size={18} className="text-[#22c55e] mt-0.5 shrink-0" />
                                <div className="space-y-1">
                                    <h3 className="text-sm font-semibold text-zinc-100">Set Overlord Safety Guardrails</h3>
                                    <p className="text-xs text-zinc-400">
                                        Keep human-in-the-loop oversight on mutations while removing approval fatigue on safe read-only lookups.
                                    </p>
                                </div>
                            </div>

                            <div className="space-y-3">
                                <label className="flex items-center justify-between p-3 rounded-xl bg-zinc-950/60 border border-zinc-800 cursor-pointer">
                                    <div>
                                        <p className="text-xs font-semibold text-zinc-100">Kernel Safe Mode (Recommended)</p>
                                        <p className="text-[11px] text-zinc-400">Enforces path boundaries, command whitelists, and mutation oversight gates.</p>
                                    </div>
                                    <input
                                        type="checkbox"
                                        checked={safe_mode}
                                        onChange={(e) => set_safe_mode(e.target.checked)}
                                        aria-label="Kernel Safe Mode"
                                        className="accent-emerald-500 w-4 h-4"
                                    />
                                </label>

                                <label className="flex items-center justify-between p-3 rounded-xl bg-zinc-950/60 border border-zinc-800 cursor-pointer">
                                    <div>
                                        <p className="text-xs font-semibold text-zinc-100">Auto-Approve Safe Read-Only Skills</p>
                                        <p className="text-[11px] text-zinc-400">Lets agents inspect files and search memory without blocking your inbox.</p>
                                    </div>
                                    <input
                                        type="checkbox"
                                        checked={auto_safe_skills}
                                        onChange={(e) => set_auto_safe_skills(e.target.checked)}
                                        aria-label="Auto-Approve Safe Read-Only Skills"
                                        className="accent-emerald-500 w-4 h-4"
                                    />
                                </label>

                                <div className="p-3 rounded-xl bg-zinc-950/60 border border-zinc-800 flex items-center justify-between gap-4">
                                    <div>
                                        <p className="text-xs font-semibold text-zinc-100">Default Agent Budget Cap (USD)</p>
                                        <p className="text-[11px] text-zinc-400">Hard spending ceiling enforced by BudgetGuard.</p>
                                    </div>
                                    <input
                                        type="number"
                                        step="0.50"
                                        min="0.10"
                                        value={budget_draft}
                                        onChange={(e) => {
                                            const raw = e.target.value;
                                            set_has_custom_budget(true);
                                            set_budget_draft(raw);
                                            const parsed = Number(raw);
                                            if (raw.trim() !== '' && Number.isFinite(parsed) && parsed >= 0.1) {
                                                set_budget_usd(parsed);
                                            }
                                        }}
                                        onBlur={() => {
                                            const clamped = clamp_budget(budget_draft, budget_usd);
                                            set_budget_usd(clamped);
                                            set_budget_draft(String(clamped));
                                        }}
                                        aria-label="Default Agent Budget Cap USD"
                                        className="w-24 px-2.5 py-1.5 rounded-lg bg-zinc-900 border border-zinc-800 text-xs font-mono text-zinc-100 text-right focus:outline-none focus:ring-2 focus:ring-emerald-500"
                                    />
                                </div>
                            </div>
                        </div>
                    )}

                    {step === 4 && (
                        <div className="space-y-4">
                            <div className="flex items-start gap-3">
                                <Rocket size={18} className="text-[#22c55e] mt-0.5 shrink-0" />
                                <div className="space-y-1">
                                    <h3 className="text-sm font-semibold text-zinc-100">Pre-Flight Check & Launch</h3>
                                    <p className="text-xs text-zinc-400">
                                        You are starting in <strong className="text-zinc-200">Focus Mode</strong> (6 core views). Switch to <strong className="text-zinc-200">Expert Mode</strong> (all 18 views) anytime via the sidebar toggle or <code className="font-mono text-zinc-200">Ctrl+Shift+E</code>.
                                    </p>
                                </div>
                            </div>

                            {sync_notice && (
                                <div
                                    data-testid="wizard-sync-notice"
                                    className={`rounded-lg border px-3 py-2 text-xs font-mono ${
                                        sync_notice.kind === 'ok'
                                            ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-300'
                                            : 'bg-amber-500/10 border-amber-500/30 text-amber-300'
                                    }`}
                                >
                                    {sync_notice.text}
                                </div>
                            )}

                            <Readiness_Card compact />
                        </div>
                    )}
                </div>

                {/* Footer */}
                <div className="flex items-center justify-between px-6 py-4 border-t border-zinc-800 bg-zinc-950/40">
                    <button
                        type="button"
                        onClick={() => (step > 1 ? set_step((step - 1) as 1 | 2 | 3 | 4) : handle_finish(false))}
                        className="px-3.5 py-2 rounded-lg bg-zinc-900 hover:bg-zinc-800 border border-zinc-800 text-xs font-mono uppercase tracking-wider text-zinc-400 hover:text-zinc-200 focus:outline-none focus:ring-2 focus:ring-emerald-500"
                    >
                        {step > 1 ? 'Back' : 'Skip for Now'}
                    </button>

                    <div className="flex items-center gap-2">
                        {step === 1 && (
                            <button
                                type="button"
                                onClick={() => set_step(2)}
                                className="px-4 py-2 rounded-lg bg-zinc-100 hover:bg-white text-zinc-950 text-xs font-mono font-bold uppercase tracking-wider focus:outline-none focus:ring-2 focus:ring-emerald-500"
                            >
                                Continue
                            </button>
                        )}
                        {step === 2 && (
                            <button
                                type="button"
                                onClick={handle_apply_tier_and_continue}
                                className="px-4 py-2 rounded-lg bg-zinc-100 hover:bg-white text-zinc-950 text-xs font-mono font-bold uppercase tracking-wider focus:outline-none focus:ring-2 focus:ring-emerald-500"
                            >
                                Apply Preset & Continue
                            </button>
                        )}
                        {step === 3 && (
                            <button
                                type="button"
                                disabled={is_syncing_guardrails}
                                onClick={() => void handle_save_safety_and_continue()}
                                className="px-4 py-2 rounded-lg bg-zinc-100 hover:bg-white text-zinc-950 text-xs font-mono font-bold uppercase tracking-wider focus:outline-none focus:ring-2 focus:ring-emerald-500 disabled:opacity-50"
                            >
                                {is_syncing_guardrails ? 'Syncing Engine...' : 'Save Guardrails & Continue'}
                            </button>
                        )}
                        {step === 4 && (
                            <>
                                <button
                                    type="button"
                                    onClick={() => handle_finish(false)}
                                    className="px-3.5 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 border border-zinc-700 text-xs font-mono font-bold uppercase tracking-wider text-zinc-100 focus:outline-none focus:ring-2 focus:ring-emerald-500"
                                >
                                    Enter Command Center
                                </button>
                                {on_launch_starter_mission && (
                                    <button
                                        type="button"
                                        onClick={() => handle_finish(true)}
                                        className="px-4 py-2 rounded-lg bg-[#22c55e] hover:bg-emerald-400 text-zinc-950 text-xs font-mono font-bold uppercase tracking-wider focus:outline-none focus:ring-2 focus:ring-emerald-500"
                                    >
                                        Launch Starter Mission
                                    </button>
                                )}
                            </>
                        )}
                    </div>
                </div>
            </div>
        </div>,
        document.body
    );
};

// Metadata: [FirstRun_Wizard]
