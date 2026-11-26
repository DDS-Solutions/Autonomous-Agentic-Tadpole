/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Oversight Hotkey Hook Unit Tests**: Verifies j/k navigation wrapping, Shift+A low-risk
 * batch trigger, `g` guidance input focus, and suppression when inactive or inside form inputs.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Unintended hotkey firing in inputs or out-of-bounds focus index.
 * - **Telemetry Link**: Search `[use_oversight_hotkeys_test]` in test logs.
 */

import { useState } from 'react';
import { render, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { useOversightHotkeys } from './use_oversight_hotkeys';
import type { OversightEntry } from '../../data/mock_oversight';

const SAMPLE_ENTRIES: OversightEntry[] = [
    { id: 'item-1', agent_id: 'a1', skill: 'read_file', description: 'Read 1', params: {}, decision: 'pending', created_at: new Date().toISOString(), timestamp: new Date().toISOString() },
    { id: 'item-2', agent_id: 'a2', skill: 'write_file', description: 'Write 2', params: {}, decision: 'pending', created_at: new Date().toISOString(), timestamp: new Date().toISOString() },
];

function Harness({
    is_active = true,
    is_blocked = false,
    low_risk_count = 1,
    on_decide = vi.fn().mockResolvedValue(true),
    on_request_batch_approve = vi.fn(),
}: {
    is_active?: boolean;
    is_blocked?: boolean;
    low_risk_count?: number;
    on_decide?: (id: string, decision: 'approved' | 'rejected') => Promise<boolean>;
    on_request_batch_approve?: () => void;
}) {
    const [focused_index, set_focused_index] = useState(0);
    useOversightHotkeys({
        is_active,
        is_blocked,
        filtered_pending: SAMPLE_ENTRIES,
        low_risk_count,
        focused_index,
        set_focused_index,
        on_decide,
        on_request_batch_approve,
    });
    return (
        <div>
            <span data-testid="focused-idx">{focused_index}</span>
            <input data-testid="oversight-guidance-input-item-1" />
        </div>
    );
}

describe('use_oversight_hotkeys', () => {
    it('wraps j/k focus navigation and triggers approve/reject/batch shortcuts (Happy Path)', () => {
        const on_decide = vi.fn().mockResolvedValue(true);
        const on_batch = vi.fn();
        const { getByTestId } = render(
            <Harness on_decide={on_decide} on_request_batch_approve={on_batch} />
        );

        expect(getByTestId('focused-idx').textContent).toBe('0');
        fireEvent.keyDown(window, { key: 'j' });
        expect(getByTestId('focused-idx').textContent).toBe('1');
        fireEvent.keyDown(window, { key: 'j' });
        expect(getByTestId('focused-idx').textContent).toBe('0');
        fireEvent.keyDown(window, { key: 'k' });
        expect(getByTestId('focused-idx').textContent).toBe('1');

        fireEvent.keyDown(window, { key: 'r' });
        expect(on_decide).toHaveBeenCalledWith('item-2', 'rejected');

        fireEvent.keyDown(window, { key: 'A', shiftKey: true });
        expect(on_batch).toHaveBeenCalledOnce();
    });

    it('suppresses hotkeys when inactive, blocked, or typing in an input (Failure / Guard Path)', () => {
        const on_decide = vi.fn().mockResolvedValue(true);
        const { getByTestId, rerender } = render(<Harness is_active={false} on_decide={on_decide} />);
        fireEvent.keyDown(window, { key: 'a' });
        expect(on_decide).not.toHaveBeenCalled();

        rerender(<Harness is_active={true} is_blocked={true} on_decide={on_decide} />);
        fireEvent.keyDown(window, { key: 'a' });
        expect(on_decide).not.toHaveBeenCalled();

        rerender(<Harness is_active={true} is_blocked={false} on_decide={on_decide} />);
        const input = getByTestId('oversight-guidance-input-item-1');
        fireEvent.keyDown(input, { key: 'a' });
        expect(on_decide).not.toHaveBeenCalled();
    });

    it('focuses the guidance input when pressing g (Edge Path)', () => {
        const { getByTestId } = render(<Harness />);
        fireEvent.keyDown(window, { key: 'g' });
        expect(document.activeElement).toBe(getByTestId('oversight-guidance-input-item-1'));
    });
});

// Metadata: [use_oversight_hotkeys_test]
