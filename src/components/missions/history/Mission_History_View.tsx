/**
 * @docs ARCHITECTURE:Interface
 *
 * ### AI Assist Note
 * **Mission_History_View**: Master-detail Flight Recorder container uniting `Mission_Catalog_Sidebar`
 * and `Mission_Flight_Dossier`. Features request-ordering protection (S-07), debounced live Event Bus
 * synchronization (S-08), and Router-independent URL query synchronization (`?id=...`).
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Backend `/v1/missions` or `/v1/missions/{id}/dossier` request failure.
 * - **Telemetry Link**: Search `[Mission_History_View]` in console logs.
 */

import React, { useState, useEffect, useCallback, useRef } from 'react';
import { Layers, AlertCircle, RefreshCw } from 'lucide-react';
import { mission_history_service } from '../../../services/mission_history_service';
import type { MissionSummaryItem, MissionDossier } from '../../../types/mission_dossier';
import { use_notification_store } from '../../../stores/notification_store';
import { event_bus } from '../../../services/event_bus';
import { Mission_Catalog_Sidebar } from './Mission_Catalog_Sidebar';
import { Mission_Flight_Dossier } from './Mission_Flight_Dossier';

function read_url_mission_id(): string | null {
    if (typeof window === 'undefined') return null;
    try {
        return new URLSearchParams(window.location.search).get('id');
    } catch {
        return null;
    }
}

function write_url_mission_id(id: string | null): void {
    if (typeof window === 'undefined') return;
    try {
        const url = new URL(window.location.href);
        if (id) {
            url.searchParams.set('id', id);
        } else {
            url.searchParams.delete('id');
        }
        window.history.replaceState({}, '', url.toString());
    } catch {
        // Ignore in restricted test environments
    }
}

export const Mission_History_View: React.FC = () => {
    const add_notification = use_notification_store((s) => s.add_notification);

    const [missions, set_missions] = useState<MissionSummaryItem[]>([]);
    const [total, set_total] = useState(0);
    const [page, set_page] = useState(1);
    const [status_filter, set_status_filter] = useState<string>('all');
    const [search_query, set_search_query] = useState('');
    const [is_catalog_loading, set_is_catalog_loading] = useState(true);
    const [catalog_error, set_catalog_error] = useState<string | null>(null);

    const [selected_mission_id, set_selected_mission_id] = useState<string | null>(() => read_url_mission_id());
    const [dossier, set_dossier] = useState<MissionDossier | null>(null);
    const [is_dossier_loading, set_is_dossier_loading] = useState(false);
    const [dossier_error, set_dossier_error] = useState<string | null>(null);
    const [is_live_syncing, set_is_live_syncing] = useState(false);

    const sync_timer_ref = useRef<ReturnType<typeof setTimeout> | null>(null);
    const bus_debounce_timer_ref = useRef<ReturnType<typeof setTimeout> | null>(null);
    const latest_catalog_req_id = useRef<number>(0);
    const latest_dossier_req_id = useRef<number>(0);
    const initial_auto_selected = useRef<boolean>(false);
    const selected_mission_id_ref = useRef<string | null>(selected_mission_id);

    // 1. Fetch catalog with request ordering protection (S-07)
    const load_catalog = useCallback(async () => {
        const req_id = ++latest_catalog_req_id.current;
        set_is_catalog_loading(true);
        set_catalog_error(null);
        try {
            const res = await mission_history_service.fetch_catalog({
                page,
                per_page: 25,
                status: status_filter,
                search: search_query.trim() || undefined,
            });
            if (req_id !== latest_catalog_req_id.current) return;
            if (res.success) {
                const items = res.data ?? res.missions ?? [];
                set_missions(items);
                set_total(res.total ?? items.length);

                if (
                    !initial_auto_selected.current &&
                    !selected_mission_id_ref.current &&
                    items.length > 0
                ) {
                    initial_auto_selected.current = true;
                    const first_id = items[0].id;
                    selected_mission_id_ref.current = first_id;
                    set_selected_mission_id(first_id);
                    write_url_mission_id(first_id);
                }
            } else {
                set_catalog_error((res as { error?: string }).error || 'Failed to load mission catalog');
            }
        } catch (err: unknown) {
            if (req_id !== latest_catalog_req_id.current) return;
            const msg = err instanceof Error ? err.message : 'Failed to connect to backend engine';
            console.error('[Mission_History_View] Failed to load mission catalog:', err);
            set_catalog_error(msg);
        } finally {
            if (req_id === latest_catalog_req_id.current) {
                set_is_catalog_loading(false);
            }
        }
    }, [page, status_filter, search_query]);

    useEffect(() => {
        void load_catalog();
    }, [load_catalog]);

    // 2. Fetch specific dossier when selected_mission_id changes with request ordering protection (S-07)
    const load_dossier = useCallback(async (id: string) => {
        const req_id = ++latest_dossier_req_id.current;
        set_is_dossier_loading(true);
        set_dossier_error(null);
        try {
            const data = await mission_history_service.fetch_dossier(id);
            if (req_id !== latest_dossier_req_id.current) return;
            set_dossier(data);
        } catch (err: unknown) {
            if (req_id !== latest_dossier_req_id.current) return;
            console.error(`[Mission_History_View] Failed to load dossier for mission ${id}:`, err);
            set_dossier_error(err instanceof Error ? err.message : 'Mission dossier could not be loaded');
        } finally {
            if (req_id === latest_dossier_req_id.current) {
                set_is_dossier_loading(false);
            }
        }
    }, []);

    useEffect(() => {
        if (selected_mission_id) {
            void load_dossier(selected_mission_id);
        } else {
            latest_dossier_req_id.current++;
            set_dossier(null);
            set_is_dossier_loading(false);
            set_dossier_error(null);
        }
    }, [selected_mission_id, load_dossier]);

    // 3. Hybrid Streaming: Subscribe to native `event_bus` logs with 800ms debounce (S-08)
    useEffect(() => {
        if (typeof event_bus.subscribe_logs !== 'function') return;
        const unsubscribe = event_bus.subscribe_logs((entry) => {
            set_is_live_syncing(true);
            if (sync_timer_ref.current) clearTimeout(sync_timer_ref.current);
            sync_timer_ref.current = setTimeout(() => {
                set_is_live_syncing(false);
            }, 1200);

            if (bus_debounce_timer_ref.current) clearTimeout(bus_debounce_timer_ref.current);
            bus_debounce_timer_ref.current = setTimeout(() => {
                void load_catalog();
                const current_id = selected_mission_id_ref.current;
                if (current_id && (!entry.mission_id || entry.mission_id === current_id)) {
                    void load_dossier(current_id);
                }
            }, 800);
        });

        return () => {
            unsubscribe();
            if (sync_timer_ref.current) clearTimeout(sync_timer_ref.current);
            if (bus_debounce_timer_ref.current) clearTimeout(bus_debounce_timer_ref.current);
        };
    }, [load_catalog, load_dossier]);

    const handle_select_mission = (id: string) => {
        selected_mission_id_ref.current = id;
        set_selected_mission_id(id);
        write_url_mission_id(id);
    };

    const handle_toggle_pin = async (id: string) => {
        try {
            const res = await mission_history_service.toggle_pin(id);
            if (res.success) {
                set_missions((prev) =>
                    prev.map((m) => (m.id === id ? { ...m, is_pinned: res.is_pinned } : m))
                );
                if (dossier && dossier.mission.id === id) {
                    set_dossier({
                        ...dossier,
                        mission: { ...dossier.mission, is_pinned: res.is_pinned },
                    });
                }
                add_notification({
                    title: res.is_pinned ? 'Mission Pinned' : 'Mission Unpinned',
                    message: `Mission ${id.slice(0, 8)} retention lock has been ${
                        res.is_pinned ? 'enabled' : 'removed'
                    }.`,
                    severity: 'info',
                    persistent: false,
                });
            } else {
                add_notification({
                    title: 'Pin Update Failed',
                    message: (res as { error?: string }).error || 'Backend rejected mission pin update.',
                    severity: 'error',
                    persistent: false,
                });
            }
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : 'Failed to toggle retention status';
            console.error(`[Mission_History_View] Failed to toggle pin for ${id}:`, err);
            add_notification({
                title: 'Pin Update Failed',
                message: msg,
                severity: 'error',
                persistent: false,
            });
        }
    };

    return (
        <div className="flex h-full w-full overflow-hidden bg-zinc-950 rounded-2xl border border-zinc-800">
            {/* Left Sidebar Catalog */}
            <Mission_Catalog_Sidebar
                missions={missions}
                selected_id={selected_mission_id}
                is_loading={is_catalog_loading}
                error={catalog_error}
                total={total}
                page={page}
                per_page={25}
                search_query={search_query}
                status_filter={status_filter}
                on_select_mission={handle_select_mission}
                on_search_change={(q) => {
                    set_search_query(q);
                    set_page(1);
                }}
                on_status_change={(s) => {
                    set_status_filter(s);
                    set_page(1);
                }}
                on_page_change={set_page}
                on_toggle_pin={handle_toggle_pin}
                on_refresh={load_catalog}
            />

            {/* Right Master Dossier Pane */}
            <div className="flex-1 flex flex-col h-full overflow-hidden">
                {is_dossier_loading ? (
                    <div className="flex-1 flex flex-col items-center justify-center p-12 text-zinc-400 gap-3">
                        <RefreshCw size={28} className="animate-spin text-cyan-400" />
                        <span className="text-xs font-mono">Compiling Flight Recorder Dossier...</span>
                    </div>
                ) : dossier_error ? (
                    <div className="flex-1 flex flex-col items-center justify-center p-12 text-center text-rose-400 gap-3">
                        <AlertCircle size={32} />
                        <span className="text-sm font-semibold">{dossier_error}</span>
                        <button
                            onClick={() => selected_mission_id && void load_dossier(selected_mission_id)}
                            className="px-3 py-1.5 rounded-lg bg-zinc-900 border border-zinc-800 text-xs text-zinc-300 hover:text-zinc-100"
                        >
                            Retry Loading Dossier
                        </button>
                    </div>
                ) : dossier ? (
                    <Mission_Flight_Dossier
                        key={dossier.mission.id}
                        dossier={dossier}
                        on_toggle_pin={handle_toggle_pin}
                        is_live_syncing={is_live_syncing}
                    />
                ) : (
                    <div className="flex-1 flex flex-col items-center justify-center p-12 text-center text-zinc-500">
                        <Layers size={36} className="text-zinc-600 mb-2" />
                        <span className="text-sm font-semibold text-zinc-400">Select a mission</span>
                        <span className="text-xs text-zinc-600 mt-1">
                            Choose a mission from the catalog sidebar to inspect its flight records
                        </span>
                    </div>
                )}
            </div>
        </div>
    );
};

// Metadata: [Mission_History_View]

