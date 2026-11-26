/**
 * @docs ARCHITECTURE:Interface
 *
 * ### AI Assist Note
 * **Oversight Hotkey Hook**: Encapsulates keyboard-first Overlord triage shortcuts
 * (`j`/`k`/arrows for focus, `a` approve, `r` reject, `g` guidance focus, `Shift+A` low-risk batch)
 * strictly gated to when `/oversight` is the active visible route and no dialog/input is focused.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Phantom approvals firing on inactive routes or while typing in a form input.
 * - **Telemetry Link**: Search `[use_oversight_hotkeys]` in UI traces.
 */

import { useEffect, type Dispatch, type SetStateAction } from 'react';
import type { OversightEntry } from '../../data/mock_oversight';

export interface Use_Oversight_Hotkeys_Params {
    is_active: boolean;
    is_blocked: boolean;
    filtered_pending: OversightEntry[];
    low_risk_count: number;
    focused_index: number;
    set_focused_index: Dispatch<SetStateAction<number>>;
    on_decide: (id: string, decision: 'approved' | 'rejected') => Promise<boolean>;
    on_request_batch_approve: () => void;
}

export function useOversightHotkeys({
    is_active,
    is_blocked,
    filtered_pending,
    low_risk_count,
    focused_index,
    set_focused_index,
    on_decide,
    on_request_batch_approve,
}: Use_Oversight_Hotkeys_Params): void {
    useEffect(() => {
        if (!is_active) return;

        const on_key_down = (e: KeyboardEvent) => {
            const target = e.target as HTMLElement | null;
            const tag = target?.tagName;
            if (
                tag === 'INPUT' ||
                tag === 'TEXTAREA' ||
                tag === 'SELECT' ||
                target?.isContentEditable ||
                e.altKey ||
                is_blocked ||
                document.querySelector('[role="dialog"]') !== null
            ) {
                return;
            }
            if (filtered_pending.length === 0) return;

            const safe_idx = Math.min(focused_index, filtered_pending.length - 1);
            const focused_item = filtered_pending[safe_idx];
            if (!focused_item) return;

            if (e.key === 'j' || e.key === 'ArrowDown') {
                e.preventDefault();
                set_focused_index((prev) => (prev + 1) % filtered_pending.length);
            } else if (e.key === 'k' || e.key === 'ArrowUp') {
                e.preventDefault();
                set_focused_index((prev) => (prev - 1 + filtered_pending.length) % filtered_pending.length);
            } else if (e.key === 'A' && e.shiftKey && low_risk_count > 0) {
                e.preventDefault();
                on_request_batch_approve();
            } else if (e.key === 'a' && !e.shiftKey && !e.ctrlKey && !e.metaKey) {
                e.preventDefault();
                void on_decide(focused_item.id, 'approved');
            } else if (e.key === 'r' && !e.ctrlKey && !e.metaKey) {
                e.preventDefault();
                void on_decide(focused_item.id, 'rejected');
            } else if (e.key === 'g' && !e.ctrlKey && !e.metaKey) {
                e.preventDefault();
                const input = document.querySelector<HTMLInputElement>(
                    `[data-testid="oversight-guidance-input-${focused_item.id}"]`
                );
                input?.focus();
            }
        };

        window.addEventListener('keydown', on_key_down);
        return () => window.removeEventListener('keydown', on_key_down);
    }, [
        filtered_pending,
        focused_index,
        is_active,
        is_blocked,
        low_risk_count,
        on_decide,
        on_request_batch_approve,
        set_focused_index,
    ]);
}

// Metadata: [use_oversight_hotkeys]
