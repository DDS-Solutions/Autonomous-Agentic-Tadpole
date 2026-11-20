/**
 * @docs ARCHITECTURE:Interface
 * @docs design:Components
 *
 * ### AI Assist Note
 * **Overlord Global Inbox Bar**: Pinned high-signal status pills rendered alongside the
 * telemetry ticker header (`Ticker_Header`). Surfaces live pending approval counts,
 * Focus/Expert cockpit mode switch, and quick setup access from any view.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Backend offline or unauthenticated; degrades gracefully to zero badge without throwing.
 * - **Telemetry Link**: Search `[Overlord_Inbox_Bar]` in UI traces.
 */

import React, { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Shield, SlidersHorizontal, Sparkles } from 'lucide-react';
import { use_settings_store } from '../../stores/settings_store';
import { tadpole_os_service } from '../../services/tadpoleos_service';
import { tadpole_os_socket } from '../../services/socket';
import { Tooltip } from '../ui';

export const Overlord_Inbox_Bar: React.FC = React.memo(() => {
    const navigate = useNavigate();
    const cockpit_mode = use_settings_store((s) => s.settings.cockpit_mode);
    const api_key = use_settings_store((s) => s.settings.tadpole_os_api_key);
    const update_setting = use_settings_store((s) => s.update_setting);
    const [pending_count, set_pending_count] = useState<number>(0);

    useEffect(() => {
        let mounted = true;
        const refresh_pending = async () => {
            if (!api_key || !api_key.trim()) {
                if (mounted) set_pending_count(0);
                return;
            }
            try {
                const items = await tadpole_os_service.get_pending_oversight();
                if (mounted && Array.isArray(items)) {
                    set_pending_count(items.length);
                }
            } catch {
                // Graceful degradation when engine is offline or unauthenticated
            }
        };

        void refresh_pending();
        const unsub = tadpole_os_socket.subscribe_custom_event((ev) => {
            const kind = String(ev.type || ev.event_type || '').toLowerCase();
            if (
                kind.includes('oversight') ||
                kind.includes('approval') ||
                kind.includes('kill') ||
                kind.includes('flagged_injection') ||
                kind.includes('circuit_breaker')
            ) {
                void refresh_pending();
            }
        });
        const timer = setInterval(() => {
            if (document.visibilityState === 'visible') {
                void refresh_pending();
            }
        }, 15000);

        return () => {
            mounted = false;
            unsub();
            clearInterval(timer);
        };
    }, [api_key]);

    const toggle_cockpit = () => {
        const next = cockpit_mode === 'focus' ? 'expert' : 'focus';
        update_setting('cockpit_mode', next);
        console.debug('[Overlord_Inbox_Bar] Cockpit mode switched to:', next);
    };

    return (
        <div className="flex items-center gap-2 px-3 bg-zinc-950 border-l border-zinc-800/80 h-full shrink-0 z-30">
            <Tooltip content="Pending Overlord approvals (click to triage)" position="bottom">
                <button
                    type="button"
                    onClick={() => navigate('/oversight')}
                    title="Open Oversight Inbox"
                    aria-label={`Open Oversight Inbox (${pending_count} pending)`}
                    className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md border text-[10px] font-mono font-bold uppercase tracking-wider transition-colors focus:outline-none focus:ring-2 focus:ring-emerald-500 ${
                        pending_count > 0
                            ? 'bg-amber-500/15 border-amber-500/40 text-amber-300'
                            : 'bg-zinc-900 border-zinc-800 text-zinc-400 hover:text-zinc-200 hover:border-zinc-700'
                    }`}
                >
                    <Shield size={11} className={pending_count > 0 ? 'text-amber-400' : 'text-zinc-500'} />
                    <span>INBOX</span>
                    <span className="tabular-nums">{pending_count}</span>
                </button>
            </Tooltip>

            <Tooltip content="Switch between Focus Mode (6 core views) and Expert Mode (all 18 views) [Ctrl+Shift+E]" position="bottom">
                <button
                    type="button"
                    onClick={toggle_cockpit}
                    title="Toggle Focus / Expert Cockpit Mode"
                    aria-label={`Switch cockpit mode (currently ${cockpit_mode})`}
                    data-testid="inbox-cockpit-mode-toggle"
                    className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md bg-zinc-900 hover:bg-zinc-800 border border-zinc-800 hover:border-zinc-700 text-[10px] font-mono font-bold uppercase tracking-wider text-zinc-300 transition-colors focus:outline-none focus:ring-2 focus:ring-emerald-500"
                >
                    <SlidersHorizontal size={11} className="text-[#22c55e]" />
                    <span>{cockpit_mode}</span>
                </button>
            </Tooltip>

            <Tooltip content="Open Day-1 Overlord Setup Wizard" position="bottom">
                <button
                    type="button"
                    onClick={() => window.dispatchEvent(new CustomEvent('app:open-setup-wizard'))}
                    title="Open Setup Wizard"
                    aria-label="Open Setup Wizard"
                    className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md bg-zinc-900 hover:bg-zinc-800 border border-zinc-800 hover:border-zinc-700 text-[10px] font-mono font-bold uppercase tracking-wider text-zinc-400 hover:text-zinc-100 transition-colors focus:outline-none focus:ring-2 focus:ring-emerald-500"
                >
                    <Sparkles size={11} className="text-[#22c55e]" />
                </button>
            </Tooltip>
        </div>
    );
});

// Metadata: [Overlord_Inbox_Bar]
