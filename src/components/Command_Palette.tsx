/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **UI Component**: Global neural command hub for rapid navigation and memory search. 
 * Implements fuzzy matching and debounced SME data retrieval via `agent_api_service`.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Debounced search timeout (API drop), keyboard trap if focus loss occurs during animation, or memory result rendering of malformed DTOs.
 * - **Telemetry Link**: Search for `[Command_Palette]` or `search_memory` in service logs.
 */

import React, { useState, useEffect, useRef } from 'react';
import type { Agent_Memory_Entry } from '../contracts/agent';
import { motion, AnimatePresence } from 'framer-motion';
import { Search, Command, User, Users, FileText, Settings, Zap, Database, Store, ShoppingBag, Lock, Shield, SlidersHorizontal, Sparkles, Activity } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { use_agent_store } from '../stores/agent_store';
import { use_sovereign_store } from '../stores/sovereign_store';
import { use_settings_store } from '../stores/settings_store';
import { agent_api_service } from '../services/agent_api_service';
import { APP_ROUTES } from '../constants/routes';
import clsx from 'clsx';
import { i18n } from '../i18n';

interface Command_Item {
    id: string;
    title: string;
    description: string;
    icon: React.ElementType;
    category: 'Agent' | 'Page' | 'Action' | 'Memory';
    action: () => void;
}

/**
 * Command_Palette
 * Global command interface for rapid navigation, agent selection, and neural memory search.
 * Features fuzzy matching and debounced SME data retrieval.
 */
export const Command_Palette: React.FC<{ is_open: boolean; on_close: () => void }> = ({ is_open, on_close }): React.ReactElement | null => {
    const [query, set_query] = useState('');
    const [selected_index, set_selected_index] = useState(0);
    const [memory_results, set_memory_results] = useState<Command_Item[]>([]);
    const [is_searching, set_is_searching] = useState(false);
    const { agents } = use_agent_store();
    const { set_selected_agent_id, set_scope } = use_sovereign_store();
    const cockpit_mode = use_settings_store((s) => s.settings.cockpit_mode);
    const update_setting = use_settings_store((s) => s.update_setting);
    const navigate = useNavigate();
    const input_ref = useRef<HTMLInputElement>(null);

    const handle_close = React.useCallback(() => {
        set_query('');
        set_selected_index(0);
        set_memory_results([]);
        on_close();
    }, [on_close]);

    // Debounced memory search (skip when unauthenticated to avoid console error spam)
    useEffect(() => {
        let is_active = true;
        const api_key = (use_settings_store.getState().settings.tadpole_os_api_key || '').trim();
        if (!is_open || !api_key || !query || query.length < 3) {
            void Promise.resolve().then(() => set_memory_results([]));
            return;
        }

        const timeout = setTimeout(async (): Promise<void> => {
            set_is_searching(true);
            try {
                const data = await agent_api_service.search_memory(query);
                if (is_active && data.status === 'success') {
                    const results = data.entries.map((m: Agent_Memory_Entry): Command_Item => ({
                        id: `mem-${m.id}`,
                        title: m.text,
                        description: i18n.t('command.memory_found'),
                        icon: Database,
                        category: 'Memory',
                        action: () => {
                            navigate('/missions');
                            handle_close();
                        }
                    }));
                    set_memory_results(results);
                }
            } catch (err) {
                if (is_active) console.error("Memory search failed:", err);
            } finally {
                if (is_active) set_is_searching(false);
            }
        }, 300);

        return () => {
            is_active = false;
            clearTimeout(timeout);
        };
    }, [is_open, navigate, query, handle_close]);

    const route_icon_map: Record<string, React.ElementType> = {
        '/dashboard': Zap,
        '/org-chart': Users,
        '/skills': Settings,
        '/infra/model-store': Store,
        '/store': ShoppingBag,
        '/security': Lock,
        '/oversight': Shield,
        '/benchmarks': Activity,
    };

    const static_items: Command_Item[] = [
        {
            id: 'action-toggle-cockpit',
            title: cockpit_mode === 'focus' ? 'Switch to Expert Cockpit Mode (18 Views)' : 'Switch to Focus Cockpit Mode (6 Core Views)',
            description: 'Toggle progressive sidebar disclosure [Ctrl+Shift+E]',
            icon: SlidersHorizontal,
            category: 'Action',
            action: () => {
                update_setting('cockpit_mode', cockpit_mode === 'focus' ? 'expert' : 'focus');
                handle_close();
            },
        },
        {
            id: 'action-open-wizard',
            title: 'Open Day-1 Overlord Setup Wizard',
            description: 'Configure session token, model tier preset, guardrails, and pre-flight check',
            icon: Sparkles,
            category: 'Action',
            action: () => {
                window.dispatchEvent(new CustomEvent('app:open-setup-wizard'));
                handle_close();
            },
        },
        {
            id: 'action-oversight-inbox',
            title: 'Open Oversight Approval Inbox',
            description: 'Triage pending agent actions with keyboard shortcuts (j/k/a/r/Shift+A)',
            icon: Shield,
            category: 'Action',
            action: () => {
                navigate('/oversight');
                handle_close();
            },
        },
        {
            id: 'action-readiness-check',
            title: 'Run 30s Overlord Readiness Check',
            description: 'Verify SQLite, provider tiers, security guardrails, and fleet health',
            icon: Activity,
            category: 'Action',
            action: () => {
                navigate('/benchmarks');
                handle_close();
            },
        },
        ...(agents || []).map((agent): Command_Item => ({
            id: `agent-${agent.id}`,
            title: agent.name,
            description: agent.role,
            icon: User,
            category: 'Agent' as const,
            action: () => {
                set_selected_agent_id(agent.id);
                set_scope('agent');
                handle_close();
            }
        })),
        ...APP_ROUTES.map((route): Command_Item => ({
            id: `page-${route.path}`,
            title: route.label,
            description: `Navigate to ${route.path}`,
            icon: route_icon_map[route.path] || Zap,
            category: 'Page',
            action: () => {
                navigate(route.path);
                handle_close();
            },
        })),
        { id: 'action-clear', title: i18n.t('command.clear_history'), description: i18n.t('command.reset_chat'), icon: FileText, category: 'Action', action: () => { use_sovereign_store.getState().clear_history(); handle_close(); } },
    ];

    const filtered_static = static_items.filter((item): boolean =>
        (item.title?.toLowerCase() || '').includes(query.toLowerCase()) ||
        (item.description?.toLowerCase() || '').includes(query.toLowerCase())
    );

    const all_items = [...filtered_static, ...(memory_results || [])].slice(0, 10);
    const active_index = all_items.length > 0 ? Math.min(selected_index, all_items.length - 1) : 0;

    useEffect(() => {
        if (is_open) {
            setTimeout(() => input_ref.current?.focus(), 10);
        }
    }, [is_open]);

    const handle_key_down = (e: React.KeyboardEvent): void => {
        if (e.key === 'Escape') {
            handle_close();
            return;
        }
        if (all_items.length === 0) return;

        if (e.key === 'ArrowDown') {
            set_selected_index(prev => (prev + 1) % all_items.length);
            e.preventDefault();
        } else if (e.key === 'ArrowUp') {
            set_selected_index(prev => (prev - 1 + all_items.length) % all_items.length);
            e.preventDefault();
        } else if (e.key === 'Enter') {
            all_items[active_index]?.action();
            e.preventDefault();
        } else if (e.key === 'Tab') {
            // Focus Trap / Cycle results
            if (e.shiftKey) {
                set_selected_index(prev => (prev - 1 + all_items.length) % all_items.length);
            } else {
                set_selected_index(prev => (prev + 1) % all_items.length);
            }
            e.preventDefault();
        }
    };

    return (
        <AnimatePresence>
            {is_open && (
                <div className="fixed inset-0 z-[200] flex items-start justify-center pt-[15vh] px-4">
                    <motion.div
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        className="fixed inset-0 bg-black/40 backdrop-blur-sm z-0"
                        onClick={handle_close}
                        onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && handle_close()}
                        role="button"
                        tabIndex={-1}
                        aria-hidden="true"
                    />

                    <motion.div
                        role="dialog"
                        aria-modal="true"
                        aria-label="Command Palette"
                        initial={{ opacity: 0, scale: 0.95, y: -20 }}
                        animate={{ opacity: 1, scale: 1, y: 0 }}
                        exit={{ opacity: 0, scale: 0.95, y: -20 }}
                        className="relative w-full max-w-xl bg-zinc-900 border border-zinc-800 rounded-2xl overflow-hidden"
                    >
                        <div className="p-4 border-b border-zinc-800 flex items-center gap-3">
                            <Search className={clsx("transition-colors", is_searching ? "text-green-500 animate-pulse" : "text-zinc-500")} size={20} />
                            <input
                                ref={input_ref}
                                type="text"
                                value={query}
                                onChange={e => {
                                    set_query(e.target.value);
                                    set_selected_index(0);
                                }}
                                onKeyDown={handle_key_down}
                                placeholder={i18n.t('command.search_placeholder')}
                                aria-label={i18n.t('command.search_aria')}
                                className="flex-1 bg-transparent border-none focus:ring-0 text-zinc-100 placeholder:text-zinc-600 font-medium"
                            />
                            <div className="px-1.5 py-0.5 rounded border border-zinc-800 text-[10px] text-zinc-600 font-mono">
                                {i18n.t('command.esc')}
                            </div>
                        </div>

                        <div className="max-h-[400px] overflow-y-auto p-2 custom-scrollbar">
                            {(all_items || []).map((item, index): React.ReactElement => {
                                const Icon = item.icon;
                                return (
                                    <button
                                        key={item.id}
                                        onMouseEnter={() => set_selected_index(index)}
                                        onClick={item.action}
                                        className={clsx(
                                            "w-full flex items-center gap-3 p-3 rounded-xl transition-all text-left group",
                                            index === active_index ? "bg-zinc-800/80 text-zinc-100" : "text-zinc-400 hover:text-zinc-200"
                                        )}
                                        role="option"
                                        aria-selected={index === active_index}
                                        id={`command-item-${item.id}`}
                                    >
                                        <div className={clsx(
                                            "p-2 rounded-lg transition-colors",
                                            index === active_index ? "bg-zinc-700 text-green-400" : "bg-zinc-950 text-zinc-600"
                                        )}>
                                            <Icon size={18} aria-hidden="true" />
                                        </div>
                                        <div className="flex-1 min-w-0">
                                            <div className="text-sm font-bold truncate">{item.title}</div>
                                            <div className="text-[10px] text-zinc-600 font-medium truncate uppercase tracking-wider">{item.description}</div>
                                        </div>
                                        <div className="text-[9px] text-zinc-700 font-mono uppercase tracking-widest">{i18n.t(`command.cat_${item.category.toLowerCase()}`)}</div>
                                    </button>
                                );
                            })}
                            {(all_items || []).length === 0 && (
                                <div className="p-12 text-center text-zinc-600 font-mono text-xs uppercase tracking-widest">
                                    {i18n.t('command.no_results', { query })}
                                </div>
                            )}
                        </div>

                        <div className="p-3 border-t border-zinc-800 bg-zinc-950/50 flex items-center justify-between text-[10px] text-zinc-600 font-bold uppercase tracking-widest">
                            <div className="flex gap-4">
                                <span className="flex items-center gap-1"><Command size={10} /> {i18n.t('command.hint_close')}</span>
                                <span className="flex items-center gap-1">{i18n.t('command.hint_navigate')}</span>
                                <span className="flex items-center gap-1">{i18n.t('command.hint_select')}</span>
                            </div>
                            <span className="text-zinc-800">{i18n.t('command.hub_footer')}</span>
                        </div>
                    </motion.div>
                </div>
            )}
        </AnimatePresence>
    );
};


// Metadata: [Command_Palette]
