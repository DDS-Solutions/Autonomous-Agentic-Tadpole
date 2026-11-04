/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Context Alignment
 * - **Subsystem**: UI Components / Hierarchy / Node_Model_Slots.test
 *
 * ### ⚠️ Invariants & Non-Negotiables
 * - `[Structural]` Component state and props flow adhere strictly to unidirectional UI data bindings.
 * - `[Layout]` Tiered model slot layout must place Slot 1 at top-left, Slot 2 at middle-middle, and Slot 3 at lower-right.
 *
 * ### 🔍 Debugging & Observability
 * - **Local Errors**: none
 * - **Telemetry Targets**: none declared
 */

import '@testing-library/jest-dom';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { Node_Model_Slots } from '../Node_Model_Slots';
import type { Agent } from '../../../types';

vi.mock('../../../i18n', () => ({
    i18n: {
        t: (key: string) => key,
    },
}));

vi.mock('../../../stores/model_store', () => ({
    use_model_store: (selector: any) =>
        selector({
            models: [
                { id: 'gemini-1.5-pro', name: 'gemini-1.5-pro', capabilities: { supports_vision: true } },
                { id: 'gemini-1.5-flash', name: 'gemini-1.5-flash', capabilities: { supports_tools: true } },
                { id: 'claude-3-5-sonnet', name: 'claude-3-5-sonnet', capabilities: { supports_reasoning: true } },
            ],
        }),
}));

const mock_toggle_dropdown = vi.fn();
const mock_close_dropdown = vi.fn();
let mock_dropdown_state = { open_id: null as string | null, open_type: null as string | null };

vi.mock('../../../stores/dropdown_store', () => ({
    use_dropdown_store: (selector: any) =>
        selector({
            ...mock_dropdown_state,
            toggle_dropdown: mock_toggle_dropdown,
            close_dropdown: mock_close_dropdown,
            is_open: (id: string, type: string) => mock_dropdown_state.open_id === id && mock_dropdown_state.open_type === type,
        }),
}));

describe('Node_Model_Slots — Tiered Model Layout Verification', () => {
    const base_mock_agent: Agent = {
        id: 'agent-101',
        name: 'Specialist Alpha',
        role: 'Inference Specialist',
        status: 'active',
        theme_color: '#10b981',
        active_model_slot: 1,
        model: 'gemini-1.5-pro',
        model_2: 'gemini-1.5-flash',
        model_3: 'claude-3-5-sonnet',
        skills: [],
        workflows: [],
        tokens_used: 100,
        input_tokens: 50,
        output_tokens: 50,
        budget_usd: 10,
        cost_usd: 1.25,
        description: 'Test agent',
        category: 'ai',
        mcp_tools: [],
        requires_oversight: false,
        failure_count: 0,
        connector_configs: [],
        metadata: {},
        department: 'Operations' as any,
    };

    beforeEach(() => {
        vi.clearAllMocks();
        mock_dropdown_state = { open_id: null, open_type: null };
    });

    describe('Pillar IV — (a) Happy Path: Tiered Geometry & Active Slot Selection', () => {
        it('renders Slot 1 (top-left), Slot 2 (middle-middle), and Slot 3 (lower-right) with correct positioning classes', () => {
            const { container } = render(<Node_Model_Slots agent={base_mock_agent} />);

            const rows = container.querySelectorAll('.w-full');
            expect(rows.length).toBe(3);

            // Row 1: Top-Left (justify-start)
            expect(rows[0]).toHaveClass('justify-start');
            expect(rows[0]).toHaveTextContent('gemini-1.5-pro');

            // Row 2: Middle-Middle (justify-center)
            expect(rows[1]).toHaveClass('justify-center');
            expect(rows[1]).toHaveTextContent('gemini-1.5-flash');

            // Row 3: Lower-Right (justify-end)
            expect(rows[2]).toHaveClass('justify-end');
            expect(rows[2]).toHaveTextContent('claude-3-5-sonnet');
        });

        it('displays active emerald LED on currently selected slot and delegates slot selection on LED click', () => {
            const mock_on_update = vi.fn();
            render(<Node_Model_Slots agent={base_mock_agent} on_update={mock_on_update} />);

            // Slot 1 is active
            const activate_secondary_btn = screen.getByRole('button', {
                name: 'agent_card.tooltip_activate_secondary',
            });
            fireEvent.click(activate_secondary_btn);

            expect(mock_on_update).toHaveBeenCalledWith('agent-101', { active_model_slot: 2 });
        });
    });

    describe('Pillar IV — (b) Failure Path: Missing Models & Fallback Labels', () => {
        it('falls back to localized fallback labels when secondary and tertiary slots are undefined', () => {
            const sparse_agent: Agent = {
                ...base_mock_agent,
                model_2: undefined,
                model_3: undefined,
            };

            render(<Node_Model_Slots agent={sparse_agent} />);

            // Slot 1 has model
            expect(screen.getByText('gemini-1.5-pro')).toBeInTheDocument();
            // Slots 2 & 3 display add model fallback
            const add_model_labels = screen.getAllByText('agent_card.label_add_model');
            expect(add_model_labels.length).toBe(2);
        });

        it('falls back to unknown model label when primary slot 1 is missing', () => {
            const no_model_agent: Agent = {
                ...base_mock_agent,
                model: undefined,
                model_config: undefined,
            };

            render(<Node_Model_Slots agent={no_model_agent} />);

            expect(screen.getByText('agent_card.label_unknown_model')).toBeInTheDocument();
        });
    });

    describe('Pillar IV — (c) Edge Case Path: Dropdown Activation & Slot Hot-Swap', () => {
        it('activates slot and toggles dropdown when clicking an unselected model badge', () => {
            const mock_on_update = vi.fn();
            render(<Node_Model_Slots agent={base_mock_agent} on_update={mock_on_update} />);

            // Click Badge for Slot 2 (which is currently not selected, slot 1 is active)
            const slot2_badge_text = screen.getByText('gemini-1.5-flash');
            const slot2_button = slot2_badge_text.closest('button');
            expect(slot2_button).toBeInTheDocument();

            fireEvent.click(slot2_button!);

            // Should activate slot 2 and open dropdown for model_2
            expect(mock_on_update).toHaveBeenCalledWith('agent-101', { active_model_slot: 2 });
            expect(mock_toggle_dropdown).toHaveBeenCalledWith('agent-101', 'model_2');
        });
    });
});

