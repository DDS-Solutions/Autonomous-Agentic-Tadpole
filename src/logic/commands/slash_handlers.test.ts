/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Slash Handlers & DeployConfirmationGate Unit Tests**: Verifies encapsulated deploy
 * TTL expiration, slot switching validation, pause/resume RPC handling, and swarm subcommands.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Confirmation TTL leak, invalid slot accepted, or unhandled slash command exception.
 * - **Telemetry Link**: Search `[slash_handlers_test]` in test logs.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
    DeployConfirmationGate,
    handle_slash_command,
    reset_deploy_confirmation_for_testing,
} from './slash_handlers';
import { event_bus } from '../../services/event_bus';
import { agent_api_service } from '../../services/agent_api_service';
import type { Agent } from '../../types';

vi.mock('../../services/event_bus', () => ({
    event_bus: {
        emit_log: vi.fn(),
        clear_history: vi.fn(),
    },
}));

vi.mock('../../services/agent_api_service', () => ({
    agent_api_service: {
        pause_agent: vi.fn(),
        resume_agent: vi.fn(),
        update_agent: vi.fn(),
        send_command: vi.fn(),
    },
}));

vi.mock('../../services/system_api_service', () => ({
    system_api_service: {
        deploy_engine: vi.fn().mockResolvedValue({ output: 'deployed' }),
        pre_pr_engine: vi.fn().mockResolvedValue({ status: 'success', output: 'ok' }),
    },
}));

vi.mock('../../stores/workspace_store', () => ({
    use_workspace_store: {
        getState: vi.fn().mockReturnValue({
            clusters: [{ id: 'c1', name: 'Core', theme: 'emerald', alpha_id: '1', objective: 'Audit', collaborators: ['2'] }],
            active_proposals: {},
            generate_proposal: vi.fn(),
        }),
    },
}));

describe('DeployConfirmationGate', () => {
    beforeEach(() => {
        vi.useFakeTimers();
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    it('consumes pending state once on valid confirmation (Happy Path)', () => {
        const gate = new DeployConfirmationGate(30_000);
        expect(gate.is_pending()).toBe(false);
        gate.request();
        expect(gate.is_pending()).toBe(true);
        expect(gate.consume()).toBe(true);
        expect(gate.is_pending()).toBe(false);
        expect(gate.consume()).toBe(false);
    });

    it('rejects consume without request or after TTL expiry (Failure / Edge Path)', () => {
        const gate = new DeployConfirmationGate(5_000);
        expect(gate.consume()).toBe(false);

        gate.request();
        vi.advanceTimersByTime(5_001);
        expect(gate.is_pending()).toBe(false);
        expect(gate.consume()).toBe(false);
    });

    it('refreshes TTL when requested again before expiry', () => {
        const gate = new DeployConfirmationGate(5_000);
        gate.request();
        vi.advanceTimersByTime(3_000);
        gate.request();
        vi.advanceTimersByTime(3_000);
        expect(gate.is_pending()).toBe(true);
        expect(gate.consume()).toBe(true);
    });
});

describe('handle_slash_command registry', () => {
    const agents: Agent[] = [
        {
            id: '1',
            name: 'CEO',
            status: 'active',
            theme_color: '#10b981',
            voice_id: 'v1',
            tokens_used: 2500,
            model: 'gpt-5',
            model_config: { provider: 'openai', modelId: 'gpt-5' },
            role: 'CEO',
            department: 'Executive',
            category: 'general',
        },
    ];
    const find_agent = (q: string | undefined) => agents.find(a => a.name.toLowerCase() === q?.toLowerCase()) || null;

    beforeEach(() => {
        vi.clearAllMocks();
        reset_deploy_confirmation_for_testing();
    });

    it('handles /clear, /help, /status, and unknown commands cleanly', async () => {
        const clear_res = await handle_slash_command('/clear', [], agents, find_agent);
        expect(clear_res.should_clear_logs).toBe(true);
        expect(event_bus.clear_history).toHaveBeenCalledOnce();

        await handle_slash_command('/help', [], agents, find_agent);
        expect(event_bus.emit_log).toHaveBeenCalledWith(expect.objectContaining({ severity: 'info' }));

        await handle_slash_command('/status', [], agents, find_agent);
        expect(event_bus.emit_log).toHaveBeenCalledWith(
            expect.objectContaining({ text: expect.stringContaining('1 active'), severity: 'success' })
        );

        await handle_slash_command('/nope', [], agents, find_agent);
        expect(event_bus.emit_log).toHaveBeenCalledWith(
            expect.objectContaining({ text: expect.stringContaining('Unknown command: /nope'), severity: 'error' })
        );
    });

    it('validates /switch slot bounds and invokes update_agent', async () => {
        await handle_slash_command('/switch', ['CEO', '4'], agents, find_agent);
        expect(agent_api_service.update_agent).not.toHaveBeenCalled();

        vi.mocked(agent_api_service.update_agent).mockResolvedValueOnce(undefined as never);
        await handle_slash_command('/switch', ['CEO', '2'], agents, find_agent);
        expect(agent_api_service.update_agent).toHaveBeenCalledWith('1', { active_model_slot: 2 });
    });

    it('routes /pause and /resume with offline fallback and error handling', async () => {
        vi.mocked(agent_api_service.pause_agent).mockResolvedValueOnce(false);
        await handle_slash_command('/pause', ['CEO'], agents, find_agent);
        expect(event_bus.emit_log).toHaveBeenCalledWith(
            expect.objectContaining({ text: expect.stringContaining('locally'), severity: 'warning' })
        );

        vi.mocked(agent_api_service.resume_agent).mockRejectedValueOnce(new Error('timeout'));
        await handle_slash_command('/resume', ['CEO'], agents, find_agent);
        expect(event_bus.emit_log).toHaveBeenCalledWith(
            expect.objectContaining({ text: expect.stringContaining('Failed to resume'), severity: 'error' })
        );
    });
});

// Metadata: [slash_handlers_test]
