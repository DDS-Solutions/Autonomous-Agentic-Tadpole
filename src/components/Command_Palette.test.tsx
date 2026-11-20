/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Command Palette Tests**: Verifies Overlord command execution (cockpit mode toggle, setup wizard
 * event dispatch, Oversight/Benchmarks navigation, agent selection), stale `selected_index` reset
 * after narrowing filter queries (`NEW-MED-1`), empty-query keyboard guard, and authenticated memory search.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Broken Ctrl+K actions, stale index Enter no-op after filtering, or unauth search spam.
 * - **Telemetry Link**: Search `[Command_Palette_test]` in test logs.
 */

import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { Command_Palette } from './Command_Palette';
import { use_settings_store } from '../stores/settings_store';
import { use_agent_registry_store } from '../stores/agent_registry_store';
import { use_sovereign_store } from '../stores/sovereign_store';
import { agent_api_service } from '../services/agent_api_service';

const navigate_mock = vi.fn();
vi.mock('react-router-dom', async (importOriginal) => {
    const actual = await importOriginal<typeof import('react-router-dom')>();
    return { ...actual, useNavigate: () => navigate_mock };
});

vi.mock('../services/agent_api_service', () => ({
    agent_api_service: {
        search_memory: vi.fn(),
    },
}));

describe('Command_Palette Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        use_settings_store.getState().reset_to_defaults();
        use_agent_registry_store.setState({
            agents: [
                { id: 'ag-1', name: 'Alpha Commander', role: 'Lead', department: 'Ops', status: 'active' } as never,
            ],
        });
    });

    it('renders dialog, executes Overlord quick actions, and toggles cockpit mode', () => {
        const on_close = vi.fn();
        const wizard_listener = vi.fn();
        window.addEventListener('app:open-setup-wizard', wizard_listener);

        render(
            <MemoryRouter>
                <Command_Palette is_open on_close={on_close} />
            </MemoryRouter>
        );

        expect(screen.getByRole('dialog', { name: /Command Palette/i })).toBeInTheDocument();

        fireEvent.click(screen.getByText(/Switch to Expert Cockpit Mode/i));
        expect(use_settings_store.getState().settings.cockpit_mode).toBe('expert');
        expect(on_close).toHaveBeenCalledTimes(1);

        fireEvent.click(screen.getByText(/Open Day-1 Overlord Setup Wizard/i));
        expect(wizard_listener).toHaveBeenCalledTimes(1);

        fireEvent.click(screen.getByText(/Open Oversight Approval Inbox/i));
        expect(navigate_mock).toHaveBeenCalledWith('/oversight');

        fireEvent.click(screen.getByText(/Run 30s Overlord Readiness Check/i));
        expect(navigate_mock).toHaveBeenCalledWith('/benchmarks');

        fireEvent.click(screen.getByText('Alpha Commander'));
        expect(use_sovereign_store.getState().selected_agent_id).toBe('ag-1');
        expect(use_sovereign_store.getState().active_scope).toBe('agent');

        window.removeEventListener('app:open-setup-wizard', wizard_listener);
    });

    it('resets selected_index when filtering, executes exact highlighted item on Enter, and gates memory search on auth', async () => {
        const on_close = vi.fn();
        (agent_api_service.search_memory as Mock).mockResolvedValue({
            status: 'success',
            entries: [{ id: 'm-1', text: 'Remember WAL mode pragma' }],
        });

        render(
            <MemoryRouter>
                <Command_Palette is_open on_close={on_close} />
            </MemoryRouter>
        );

        const input = screen.getByRole('textbox');

        // Move down 5 slots, then narrow query to 1 exact item -> selected_index MUST reset to 0 so Enter executes it!
        for (let i = 0; i < 5; i++) fireEvent.keyDown(input, { key: 'ArrowDown' });
        fireEvent.change(input, { target: { value: 'Alpha Commander' } });
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(use_sovereign_store.getState().selected_agent_id).toBe('ag-1');
        expect(on_close).toHaveBeenCalledTimes(1);

        // Unauthenticated query must NOT call search_memory
        expect(agent_api_service.search_memory).not.toHaveBeenCalled();

        // Authenticate and run debounced memory search -> clicking navigates to /missions
        use_settings_store.getState().update_setting('tadpole_os_api_key', 'valid-token-123456');
        fireEvent.change(input, { target: { value: 'WAL' } });
        await waitFor(
            () => {
                expect(agent_api_service.search_memory).toHaveBeenCalledWith('WAL');
                expect(screen.getByText('Remember WAL mode pragma')).toBeInTheDocument();
            },
            { timeout: 1500 }
        );
        fireEvent.click(screen.getByText('Remember WAL mode pragma'));
        expect(navigate_mock).toHaveBeenCalledWith('/missions');

        // Empty-result keyboard guard + Escape close
        (agent_api_service.search_memory as Mock).mockResolvedValue({ status: 'success', entries: [] });
        fireEvent.change(input, { target: { value: 'zzz_no_match_999' } });
        fireEvent.keyDown(input, { key: 'ArrowDown' });
        fireEvent.keyDown(input, { key: 'Escape' });
        expect(on_close).toHaveBeenCalled();
    });
});

// Metadata: [Command_Palette_test]
