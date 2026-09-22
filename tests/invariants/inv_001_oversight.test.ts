/**
 * @docs ARCHITECTURE:Governance
 *
 * ### AI Assist Note
 * **INV-001: Oversight Decision Boundary Invariant**
 * Enforces that user question decisions strictly map to 'approved' vs 'rejected'
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Non-conforming oversight decision payload or unhandled rejection.
 * - **Telemetry Link**: Search `[inv_001_oversight]` in test logs.
 *
 * // Metadata: [inv_001_oversight]
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { oversight_api_service } from '../../src/services/oversight_api_service';
import * as baseApi from '../../src/services/base_api_service';

describe('INV-001: Oversight Decision Boundary Invariant', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('requires valid approval decision mapping ("approved" | "rejected")', async () => {
        const apiSpy = vi.spyOn(baseApi, 'api_request').mockResolvedValue({});

        await oversight_api_service.decide_oversight('action-123', 'approved');
        expect(apiSpy).toHaveBeenCalledWith('/v1/oversight/action-123/decide', {
            method: 'POST',
            body: JSON.stringify({ decision: 'approved' }),
        });

        await oversight_api_service.decide_oversight('action-456', 'rejected', 'Violates security boundary');
        expect(apiSpy).toHaveBeenCalledWith('/v1/oversight/action-456/decide', {
            method: 'POST',
            body: JSON.stringify({ decision: 'rejected', answer: 'Violates security boundary' }),
        });
    });

    it('propagates network failure without corrupting decision state', async () => {
        vi.spyOn(baseApi, 'api_request').mockRejectedValue(new Error('Network timeout'));

        await expect(
            oversight_api_service.decide_oversight('action-timeout', 'approved')
        ).rejects.toThrow('Network timeout');
    });
});
