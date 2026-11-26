/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Execution Mode Selector Tests**: Validates 3-mode (`chat` | `plan` | `swarm`) switching,
 * ARIA pressed state, disabled guard, and compact layout rendering.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Incorrect `aria-pressed` state or ignored click handler.
 * - **Trace Scope**: `src/components/ui/Execution_Mode_Selector.test`
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { Execution_Mode_Selector } from './Execution_Mode_Selector';

describe('Execution_Mode_Selector', () => {
    it('renders all three execution modes and marks active mode as pressed', () => {
        const on_change = vi.fn();
        render(<Execution_Mode_Selector mode="plan" on_change={on_change} />);

        expect(screen.getByRole('button', { name: /Chat/i })).toHaveAttribute('aria-pressed', 'false');
        expect(screen.getByRole('button', { name: /Plan/i })).toHaveAttribute('aria-pressed', 'true');
        expect(screen.getByRole('button', { name: /Swarm/i })).toHaveAttribute('aria-pressed', 'false');

        fireEvent.click(screen.getByRole('button', { name: /Swarm/i }));
        expect(on_change).toHaveBeenCalledWith('swarm');
    });

    it('blocks changes when disabled', () => {
        const on_change = vi.fn();
        render(<Execution_Mode_Selector mode="chat" on_change={on_change} disabled compact />);

        const swarm_btn = screen.getByRole('button', { name: /Swarm/i });
        expect(swarm_btn).toBeDisabled();
        fireEvent.click(swarm_btn);
        expect(on_change).not.toHaveBeenCalled();
    });
});
