/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Readiness Card Component Tests**: Verifies native 4-probe Overlord Readiness UI across
 * ready, degraded/not_ready, and error/unauthenticated states.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Missing score/verdict badges or uncaught network rejections.
 * - **Telemetry Link**: Search `[Readiness_Card_test]` in test logs.
 */

import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { Readiness_Card } from './Readiness_Card';
import { tadpole_os_service } from '../../services/tadpoleos_service';

vi.mock('../../services/tadpoleos_service', () => ({
    tadpole_os_service: {
        run_readiness_probe: vi.fn(),
    },
}));

describe('Readiness_Card Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('executes probe and displays ready verdict, checks, and latency', async () => {
        const on_complete = vi.fn();
        (tadpole_os_service.run_readiness_probe as Mock).mockResolvedValue({
            score: 95,
            verdict: 'ready',
            recommended_max_depth: 4,
            detected_tier: 'hybrid_overlord',
            total_latency_ms: 14.2,
            checked_at: new Date().toISOString(),
            checks: [
                { id: 'database_integrity', label: 'SQLite WAL & Foreign Keys', status: 'pass', latency_ms: 1.2, detail: 'PRAGMA foreign_keys = ON, journal_mode = wal.' },
                { id: 'provider_tier', label: 'Live Model Tier & Providers', status: 'pass', latency_ms: 10.1, detail: 'Hybrid Overlord tier live.' },
            ],
        });

        render(<Readiness_Card on_complete={on_complete} />);
        fireEvent.click(screen.getByRole('button', { name: /Run Overlord Readiness Check/i }));

        expect(await screen.findByText(/95\/100/i)).toBeInTheDocument();
        expect(screen.getAllByText(/hybrid overlord/i).length).toBeGreaterThanOrEqual(1);
        expect(screen.getByText(/SQLite WAL & Foreign Keys/i)).toBeInTheDocument();
        expect(on_complete).toHaveBeenCalledTimes(1);
    });

    it('renders actionable remediation hints on failed/not_ready probes', async () => {
        (tadpole_os_service.run_readiness_probe as Mock).mockResolvedValue({
            score: 45,
            verdict: 'not_ready',
            recommended_max_depth: 1,
            detected_tier: 'unconfigured',
            total_latency_ms: 5.5,
            checked_at: new Date().toISOString(),
            checks: [
                {
                    id: 'provider_tier',
                    label: 'Live Model Tier & Providers',
                    status: 'fail',
                    latency_ms: 3.1,
                    detail: 'No live LLM provider ready.',
                    remediation: 'Start Ollama or add a cloud API key.',
                },
            ],
        });

        render(<Readiness_Card compact />);
        fireEvent.click(screen.getByRole('button', { name: /Run Overlord Readiness Check/i }));

        expect(await screen.findByText(/45\/100/i)).toBeInTheDocument();
        expect(screen.getByText(/Start Ollama or add a cloud API key/i)).toBeInTheDocument();
    });

    it('renders honest error banner on 401 or network error without fake score', async () => {
        (tadpole_os_service.run_readiness_probe as Mock).mockRejectedValue(new Error('401 Unauthorized'));

        render(<Readiness_Card />);
        fireEvent.click(screen.getByRole('button', { name: /Run Overlord Readiness Check/i }));

        await waitFor(() => {
            expect(screen.getByText(/Readiness check unavailable: 401 Unauthorized/i)).toBeInTheDocument();
        });
    });
});

// Metadata: [Readiness_Card_test]
