/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Overlord Inbox Bar Tests**: Verifies token-gated polling (zero unauthenticated spam),
 * live pending approval count updates on WS events, cockpit mode toggle, and setup wizard trigger.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Unauthenticated 401 console spam or stale inbox badge.
 * - **Telemetry Link**: Search `[Overlord_Inbox_Bar_test]` in test logs.
 */

import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { Overlord_Inbox_Bar } from './Overlord_Inbox_Bar';
import { use_settings_store } from '../../stores/settings_store';
import { tadpole_os_service } from '../../services/tadpoleos_service';

let custom_cb: ((ev: Record<string, unknown>) => void) | null = null;

vi.mock('../../services/tadpoleos_service', () => ({
    tadpole_os_service: {
        get_pending_oversight: vi.fn(),
    },
}));

vi.mock('../../services/socket', () => ({
    tadpole_os_socket: {
        subscribe_custom_event: vi.fn((cb: (ev: Record<string, unknown>) => void) => {
            custom_cb = cb;
            return () => {
                custom_cb = null;
            };
        }),
    },
}));

describe('Overlord_Inbox_Bar Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        custom_cb = null;
        use_settings_store.getState().reset_to_defaults();
    });

    it('skips polling when unauthenticated (no session token)', () => {
        use_settings_store.getState().update_setting('tadpole_os_api_key', '');
        render(<MemoryRouter><Overlord_Inbox_Bar /></MemoryRouter>);
        expect(tadpole_os_service.get_pending_oversight).not.toHaveBeenCalled();
    });

    it('displays pending count when authenticated, refreshes on WS events, and toggles cockpit mode', async () => {
        use_settings_store.getState().update_setting('tadpole_os_api_key', 'valid-token-123456');
        (tadpole_os_service.get_pending_oversight as Mock)
            .mockResolvedValueOnce([{ id: 'p1' }, { id: 'p2' }])
            .mockResolvedValueOnce([{ id: 'p1' }]);

        render(<MemoryRouter><Overlord_Inbox_Bar /></MemoryRouter>);

        await waitFor(() => {
            expect(screen.getByLabelText(/Open Oversight Inbox \(2 pending\)/i)).toBeInTheDocument();
        });

        // Trigger WS kill/oversight event -> refreshes badge to 1
        custom_cb?.({ type: 'engine_kill' });
        await waitFor(() => {
            expect(screen.getByLabelText(/Open Oversight Inbox \(1 pending\)/i)).toBeInTheDocument();
        });

        // Toggle Focus <-> Expert
        const modeBtn = screen.getByTestId('inbox-cockpit-mode-toggle');
        expect(modeBtn).toHaveTextContent(/focus/i);
        fireEvent.click(modeBtn);
        expect(use_settings_store.getState().settings.cockpit_mode).toBe('expert');

        // Dispatch setup wizard custom event
        const listener = vi.fn();
        window.addEventListener('app:open-setup-wizard', listener);
        fireEvent.click(screen.getByRole('button', { name: /Open Setup Wizard/i }));
        expect(listener).toHaveBeenCalledTimes(1);
        window.removeEventListener('app:open-setup-wizard', listener);
    });
});

// Metadata: [Overlord_Inbox_Bar_test]
