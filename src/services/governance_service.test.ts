/**
 * @docs ARCHITECTURE:Services
 * 
 * ### AI Assist Note
 * Unit test suite for Governance_Service: Verifies quota mapping, socket event
 * ingestion for GOVERNANCE_PULSE, reactive subscriber notification, and validation.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Pulse desync, malformed quota payload, or listener leak.
 * - **Telemetry Link**: Search `[governance_service_test]` in test logs.
 */

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest';
import { governance_service } from './governance_service';
import { system_api_service } from './system_api_service';
import { event_bus } from './event_bus';

vi.mock('./system_api_service', () => ({
    system_api_service: {
        get_security_quotas: vi.fn(),
        update_security_quota: vi.fn(),
        get_sovereign_manifest: vi.fn(),
    }
}));

vi.mock('./socket', () => {
    let custom_listener: ((data: Record<string, unknown>) => void) | null = null;
    return {
        tadpole_os_socket: {
            subscribe_custom_event: vi.fn((cb) => {
                custom_listener = cb;
            }),
            __trigger_custom_event: (data: Record<string, unknown>) => {
                if (custom_listener) custom_listener(data);
            }
        }
    };
});

vi.mock('./event_bus', () => ({
    event_bus: {
        emit_log: vi.fn(),
    }
}));

describe('governance_service - Mission Budget Pulse & Quota Orchestration', () => {
    const raw_backend_quotas = {
        total_budget: 200,
        total_spent: 50,
        efficiency: 25, // 25% from server-rs
        remaining: 150,
        agent_quotas: [
            {
                entity_id: 'agent-alpha',
                budget_usd: 100,
                used_usd: 25,
                reset_period: 'daily' as const,
                last_reset_at: '2026-09-07T00:00:00Z',
                next_reset_at: '2026-09-08T00:00:00Z',
            }
        ],
        system_defense: {
            merkle_integrity: 1.0,
            aletheia_status: 'verified',
            drift_status: 'stable',
            memory_pressure: 0.18,
            cpu_load: 0.22,
            sandbox_status: 'ACTIVE',
            sandbox_type: 'Docker Sandbox'
        },
        last_sync: '2026-09-07T12:00:00Z'
    };

    beforeEach(() => {
        vi.clearAllMocks();
        (system_api_service.get_security_quotas as Mock).mockResolvedValue(raw_backend_quotas);
        (system_api_service.get_sovereign_manifest as Mock).mockResolvedValue('# Sovereign Manifest');
        (system_api_service.update_security_quota as Mock).mockResolvedValue({ status: 'ok' });
    });

    afterEach(() => {
        vi.restoreAllMocks();
    });

    it('synchronizes quotas via system_api_service and notifies event bus', async () => {
        const quotas = await governance_service.sync();

        expect(system_api_service.get_security_quotas).toHaveBeenCalledTimes(1);
        expect(quotas.total_budget).toBe(200);
        expect(quotas.total_spent).toBe(50);
        expect(quotas.remaining).toBe(150);
        expect(quotas.efficiency).toBe(25);
        expect(governance_service.get_current_quotas()).toEqual(quotas);

        expect(event_bus.emit_log).toHaveBeenCalledWith(
            expect.objectContaining({
                text: expect.stringContaining('Budget utilization at 25.0%'),
                severity: 'info',
                source: 'System'
            })
        );
    });

    it('notifies registered on_pulse listeners when new quotas are synchronized', async () => {
        const pulse_spy = vi.fn();
        const unsubscribe = governance_service.on_pulse(pulse_spy);

        await governance_service.sync();

        expect(pulse_spy).toHaveBeenCalledTimes(1);
        expect(pulse_spy).toHaveBeenCalledWith(expect.objectContaining({
            total_budget: 200,
            total_spent: 50,
            remaining: 150
        }));

        unsubscribe();
        await governance_service.sync();
        expect(pulse_spy).toHaveBeenCalledTimes(1); // Not called again after unsubscribe
    });

    it('updates security quota after validating non-negative values', async () => {
        await expect(governance_service.update_quota('agent-alpha', -10)).rejects.toThrow('Budget cannot be negative.');

        await governance_service.update_quota('agent-alpha', 150);

        expect(system_api_service.update_security_quota).toHaveBeenCalledWith('agent-alpha', 150);
        expect(system_api_service.get_security_quotas).toHaveBeenCalled(); // Triggered sync()
    });

    it('delegates get_manifest to system_api_service.get_sovereign_manifest', async () => {
        const manifest = await governance_service.get_manifest();
        expect(manifest).toBe('# Sovereign Manifest');
        expect(system_api_service.get_sovereign_manifest).toHaveBeenCalledTimes(1);
    });
});
