/**
 * @docs ARCHITECTURE:TestSuites
 * ### AI Assist Note
 * Verifies artifact promotion uses the authenticated service and reports failures.
 * ### 🔍 Debugging & Observability
 * - Failure Path: Promotion bypasses the service or falsely displays success.
 * - Telemetry Link: Vitest component assertions.
 */
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { ArtifactPromotionCard } from './ArtifactPromotionCard';
import { sovereign_api_service } from '../../services/sovereign_api_service';

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it('promotes through the shared service and returns the acknowledged skill identity', async () => {
    const promote = vi.spyOn(sovereign_api_service, 'promote').mockResolvedValue('skill-id');
    const onPromoted = vi.fn();
    render(<ArtifactPromotionCard name="Generated skill" content="test content" agentId="agent" missionId="mission" onPromoted={onPromoted} />);
    fireEvent.click(screen.getByRole('button'));
    await waitFor(() => expect(onPromoted).toHaveBeenCalledWith('skill-id'));
    expect(promote).toHaveBeenCalledWith(expect.objectContaining({
        name: 'Generated skill', cap_type: 'skill', content: 'test content', agent_id: 'agent', mission_id: 'mission',
    }));
    expect(screen.getByRole('button')).toBeDisabled();
});

it('displays an authentication failure without reporting promotion success', async () => {
    vi.spyOn(sovereign_api_service, 'promote').mockRejectedValue(new Error('Unauthorized'));
    const onPromoted = vi.fn();
    render(<ArtifactPromotionCard name="Generated skill" content="test content" agentId="agent" onPromoted={onPromoted} />);
    fireEvent.click(screen.getByRole('button'));
    expect(await screen.findByText(/Unauthorized/)).toBeInTheDocument();
    expect(onPromoted).not.toHaveBeenCalled();
    expect(screen.getByRole('button')).not.toBeDisabled();
});

// Metadata: [ArtifactPromotionCard_test]
