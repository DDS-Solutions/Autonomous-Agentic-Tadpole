/**
 * @docs ARCHITECTURE:UI
 * @docs ARCHITECTURE:Governance
 * 
 * ### AI Assist Note
 * Unit test suite verifying that Governance_View renders Kernel Defense telemetry,
 * Aletheia Verification states, Merkle Chain Integrity, and Browser Sentinel controls.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Quota sync failure, unhandled telemetry state, or element unmount.
 * - **Telemetry Link**: Search `[Governance_View_test]` in test logs.
 */

import { render, screen, waitFor, cleanup, act, fireEvent } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest';
import Governance_View from './Governance_View';
import { governance_service } from '../services/governance_service';
import { sentinel_daemon } from '../services/sentinel_daemon';
import { use_settings_store } from '../stores/settings_store';
import { use_browser_specialist_store } from '../stores/browser_specialist_store';

vi.mock('../services/governance_service', () => ({
    governance_service: {
        get_current_quotas: vi.fn(),
        sync: vi.fn(),
        get_manifest: vi.fn().mockResolvedValue('# Sovereign Manifest\nAletheia verification active.'),
        on_pulse: vi.fn(() => vi.fn()),
        update_quota: vi.fn(),
    }
}));

vi.mock('../services/sentinel_daemon', () => ({
    sentinel_daemon: {
        start: vi.fn(),
        stop: vi.fn(),
        scan_now: vi.fn(),
    }
}));

vi.mock('../services/browser_inference', () => ({
    browser_inference_service: {
        dispose: vi.fn(),
        init_specialist: vi.fn(),
        subscribe_status: vi.fn(() => vi.fn()),
        subscribe_progress: vi.fn(() => vi.fn()),
        get_status: vi.fn(() => 'idle'),
        get_active_device: vi.fn(() => 'none'),
    }
}));

describe('Governance_View - Kernel Defense Integration', () => {
    const mock_quotas = {
        total_budget: 100,
        total_spent: 25,
        efficiency: 0.25,
        remaining: 75,
        agent_quotas: [
            {
                entity_id: 'agent-1',
                budget_usd: 50,
                used_usd: 12.5,
                reset_period: 'daily' as const,
                last_reset_at: new Date().toISOString(),
                next_reset_at: new Date().toISOString()
            }
        ],
        system_defense: {
            merkle_integrity: 1.0,
            aletheia_status: 'verified' as const,
            drift_status: 'stable' as const,
            memory_pressure: 0.22,
            cpu_load: 0.15,
            sandbox_status: 'ACTIVE' as const,
            sandbox_type: 'Docker Sandbox'
        },
        last_sync: new Date().toISOString()
    };

    beforeEach(() => {
        vi.clearAllMocks();
        (governance_service.get_current_quotas as Mock).mockReturnValue(mock_quotas);
        (governance_service.sync as Mock).mockResolvedValue(mock_quotas);
        (governance_service.get_manifest as Mock).mockResolvedValue('# Sovereign Manifest');
        use_settings_store.getState().update_setting('is_safe_mode', true);
        use_settings_store.getState().update_setting('sentinel_mode', false);
        use_settings_store.getState().update_setting('browser_specialist_model_id', 'HuggingFaceTB/SmolLM-360M-Instruct');
        use_browser_specialist_store.setState({
            status: 'idle',
            active_device: 'none',
            last_error: null,
            anomaly_count: 0
        });
    });

    afterEach(() => {
        cleanup();
    });

    it('renders Kernel Defense card with Aletheia Verified status when safe_mode is active', async () => {
        render(
            <MemoryRouter>
                <Governance_View />
            </MemoryRouter>
        );

        await waitFor(() => {
            expect(screen.getByText(/Kernel Defense/i)).toBeInTheDocument();
        });

        expect(screen.getByText('Aletheia Verification')).toBeInTheDocument();
        expect(screen.getByText('Verified')).toBeInTheDocument();
        expect(screen.getByText(/Mode: Secure/i)).toBeInTheDocument();
        expect(screen.getByText('100.00%')).toBeInTheDocument();
    });

    it('reflects Bypassed status when safe_mode is disabled in settings', async () => {
        use_settings_store.getState().update_setting('is_safe_mode', false);

        render(
            <MemoryRouter>
                <Governance_View />
            </MemoryRouter>
        );

        await waitFor(() => {
            expect(screen.getByText('Bypassed')).toBeInTheDocument();
        });

        expect(screen.getByText(/Mode: Unrestricted/i)).toBeInTheDocument();
    });

    it('renders host containment and memory guard telemetry accurately', async () => {
        render(
            <MemoryRouter>
                <Governance_View />
            </MemoryRouter>
        );

        await waitFor(() => {
            expect(screen.getByText('Docker Sandbox')).toBeInTheDocument();
            expect(screen.getByText('22.0%')).toBeInTheDocument();
        });
    });

    it('renders degraded Merkle Chain Integrity when cryptographic verification is sub-optimal', async () => {
        const degraded_quotas = {
            ...mock_quotas,
            system_defense: {
                ...mock_quotas.system_defense,
                merkle_integrity: 0.85
            }
        };
        (governance_service.get_current_quotas as Mock).mockReturnValue(degraded_quotas);
        (governance_service.sync as Mock).mockResolvedValue(degraded_quotas);

        render(
            <MemoryRouter>
                <Governance_View />
            </MemoryRouter>
        );

        await waitFor(() => {
            expect(screen.getByText('85.00%')).toBeInTheDocument();
        });
    });

    it('renders the Sovereign State Manifest terminal with loaded manifest content', async () => {
        (governance_service.get_manifest as Mock).mockResolvedValue(
            '### SOVEREIGN STATE MANIFEST\n- **Swarm Vitality**: 4 Active, 2 Idle\n- **Financial Governance**: $75.00 remaining\n- **Security Pulse**: Last Audit: Verified'
        );

        render(
            <MemoryRouter>
                <Governance_View />
            </MemoryRouter>
        );

        await waitFor(() => {
            expect(screen.getByRole('heading', { name: /Sovereign State Manifest/i })).toBeInTheDocument();
            expect(screen.getByText(/Swarm Vitality/i)).toBeInTheDocument();
        }, { timeout: 2000 });

        expect(screen.getByText(/Real-time Telemetry Injected/i)).toBeInTheDocument();
        expect(screen.getByText(/SEC-01 Manifest Validation Active/i)).toBeInTheDocument();
    });

    it('renders fallback waiting message when manifest is null or empty', async () => {
        (governance_service.get_manifest as Mock).mockResolvedValue('');

        render(
            <MemoryRouter>
                <Governance_View />
            </MemoryRouter>
        );

        await waitFor(() => {
            expect(screen.getByText('Waiting for system pulse...')).toBeInTheDocument();
        });
    });

    it('renders Mission Budget Pulse card with spent, limit, efficiency score, and remaining funds', async () => {
        render(
            <MemoryRouter>
                <Governance_View />
            </MemoryRouter>
        );

        await waitFor(() => {
            expect(screen.getByRole('heading', { name: /Mission Budget Pulse/i })).toBeInTheDocument();
        });

        expect(screen.getByText('$25.00')).toBeInTheDocument();
        expect(screen.getByText('of $100.00 limit')).toBeInTheDocument();
        expect(screen.getByText('25.0%')).toBeInTheDocument();
        expect(screen.getByText('$75.00')).toBeInTheDocument();
    });

    it('updates Mission Budget Pulse reactively when an event-driven pulse is received', async () => {
        let pulse_listener: ((quotas: any) => void) | null = null;
        (governance_service.on_pulse as Mock).mockImplementation((cb) => {
            pulse_listener = cb;
            return vi.fn();
        });

        render(
            <MemoryRouter>
                <Governance_View />
            </MemoryRouter>
        );

        await waitFor(() => {
            expect(screen.getByText('$25.00')).toBeInTheDocument();
        });

        const updated_pulse = {
            ...mock_quotas,
            total_spent: 45.5,
            remaining: 54.5,
            efficiency: 0.455,
        };

        act(() => {
            pulse_listener!(updated_pulse);
        });

        await waitFor(() => {
            expect(screen.getByText('$45.50')).toBeInTheDocument();
            expect(screen.getByText('$54.50')).toBeInTheDocument();
            expect(screen.getByText('45.5%')).toBeInTheDocument();
        });
    });

    it('reconciles persisted sentinel_mode=true on mount by starting daemon and initializing specialist', async () => {
        use_settings_store.getState().update_setting('sentinel_mode', true);
        use_browser_specialist_store.setState({ status: 'idle' });
        const init_spy = vi.fn().mockResolvedValue(undefined);
        use_browser_specialist_store.setState({ init: init_spy });

        render(
            <MemoryRouter>
                <Governance_View />
            </MemoryRouter>
        );

        await waitFor(() => {
            expect(sentinel_daemon.start).toHaveBeenCalled();
            expect(init_spy).toHaveBeenCalled();
        });
    });

    it('disables pipeline Reset button while specialist is loading or thinking', async () => {
        use_browser_specialist_store.setState({ status: 'loading' });

        render(
            <MemoryRouter>
                <Governance_View />
            </MemoryRouter>
        );

        await waitFor(() => {
            const reset_btn = screen.getByRole('button', { name: /Reset/i });
            expect(reset_btn).toBeDisabled();
        });
    });

    it('renders sentinel toggle switch with accessible role and aria-checked attribute', async () => {
        use_settings_store.getState().update_setting('sentinel_mode', true);

        render(
            <MemoryRouter>
                <Governance_View />
            </MemoryRouter>
        );

        await waitFor(() => {
            const toggle = screen.getByRole('switch');
            expect(toggle).toBeInTheDocument();
            expect(toggle).toHaveAttribute('aria-checked', 'true');
        });
    });

    it('handles scan_now failure gracefully without throwing unhandled rejection', async () => {
        use_settings_store.getState().update_setting('sentinel_mode', true);
        use_browser_specialist_store.setState({ status: 'ready' });
        (sentinel_daemon.scan_now as Mock).mockRejectedValue(new Error('GPU context lost'));

        render(
            <MemoryRouter>
                <Governance_View />
            </MemoryRouter>
        );

        const audit_btn = screen.getByRole('button', { name: /Run Audit Now/i });
        fireEvent.click(audit_btn);

        await waitFor(() => {
            expect(use_browser_specialist_store.getState().status).toBe('error');
            expect(use_browser_specialist_store.getState().last_error).toBe('GPU context lost');
        });
    });
});

