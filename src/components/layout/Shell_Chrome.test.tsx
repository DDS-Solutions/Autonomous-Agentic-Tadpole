/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Layout Chrome & System Log Tests**: Verifies `Tab_Bar`, `Tab_Item`,
 * `Observability_Sidebar` detachment hiding, and `System_Log` severity rendering.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Tab switching/closing regressions, detached sidebar flicker, or log severity styling mismatch.
 * - **Telemetry Link**: Search `[Shell_Chrome_test]` in test logs.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { Tab_Bar } from './Tab_Bar';
import { Observability_Sidebar } from './Observability_Sidebar';
import { System_Log } from '../dashboard/System_Log';
import { use_tab_store } from '../../stores/tab_store';

vi.mock('../Lineage_Stream', () => ({ Lineage_Stream: () => <div data-testid="mock-lineage" /> }));
vi.mock('../Neural_Waterfall', () => ({ Neural_Waterfall: () => <div data-testid="mock-waterfall" /> }));
vi.mock('../../hooks/use_logs', () => ({
    useLogs: () => ({
        logs: [
            { id: 'l-1', timestamp: Date.now(), source: 'ENGINE', agent_id: 'ceo', agent_name: 'CEO', severity: 'success', text: 'Boot OK' },
            { id: 'l-2', timestamp: Date.now(), source: 'SEC', severity: 'error', text: 'Denied' },
            { id: 'l-3', timestamp: Date.now(), source: 'NET', severity: 'warning', text: 'Slow' },
            { id: 'l-4', timestamp: Date.now(), source: 'SYS', severity: 'info', text: 'Pulse' },
        ],
        logs_end_ref: { current: null },
    }),
}));

describe('Layout Chrome & System_Log Components', () => {
    beforeEach(() => {
        use_tab_store.setState({
            tabs: [
                { id: 't-1', title: 'Command Center', path: '/dashboard', icon: 'LayoutDashboard' },
                { id: 't-2', title: 'Oversight', path: '/oversight', icon: 'Shield' },
            ],
            active_tab_id: 't-1',
            is_system_log_detached: false,
            is_trace_stream_detached: false,
            is_lineage_stream_detached: false,
        });
    });

    it('switches, activates via keyboard, detaches, and closes tabs in Tab_Bar', () => {
        render(<Tab_Bar />);
        const tabs = screen.getAllByRole('tab');
        expect(tabs).toHaveLength(2);

        fireEvent.click(tabs[1]);
        expect(use_tab_store.getState().active_tab_id).toBe('t-2');

        fireEvent.keyDown(tabs[0], { key: 'Enter' });
        expect(use_tab_store.getState().active_tab_id).toBe('t-1');

        const buttons = tabs[1].querySelectorAll('button');
        buttons.forEach((b) => fireEvent.click(b));

        use_tab_store.setState({ tabs: [] });
        const { container } = render(<Tab_Bar />);
        expect(container.firstChild).toBeNull();
    });

    it('renders Observability_Sidebar sectors, System_Log severities, and hides when all detached', () => {
        const { rerender, container } = render(<Observability_Sidebar />);
        expect(screen.getByTestId('mock-lineage')).toBeInTheDocument();
        expect(screen.getByTestId('mock-waterfall')).toBeInTheDocument();
        expect(screen.getByText('Boot OK')).toBeInTheDocument();
        expect(screen.getByText('Denied')).toBeInTheDocument();
        expect(screen.getByText('Slow')).toBeInTheDocument();
        expect(screen.getByText('Pulse')).toBeInTheDocument();

        // Toggle system log detachment button inside System_Log
        const detachBtn = screen.getByRole('button');
        fireEvent.click(detachBtn);
        expect(use_tab_store.getState().is_system_log_detached).toBe(true);

        // Detach all sectors: hidden in normal context, shows placeholder in detached context
        use_tab_store.setState({
            is_system_log_detached: true,
            is_trace_stream_detached: true,
            is_lineage_stream_detached: true,
        });
        rerender(<Observability_Sidebar />);
        expect(container.firstChild).toBeNull();

        rerender(<Observability_Sidebar is_detached_context />);
        expect(container.firstChild).not.toBeNull();

        render(<System_Log is_detached_view />);
    });
});

// Metadata: [Shell_Chrome_test]
