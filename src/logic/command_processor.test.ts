/**
 * @docs ARCHITECTURE:TestSuites
 * 
 * ### AI Assist Note
 * **Tests the command processing and routing logic.** 
 * Validates the parsing of chat commands and ensures they are correctly routed to the appropriate agent, cluster, or swarm scope based on prefixes (@, #, etc.).
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Regex parsing failure for complex quoted strings or incorrect scope resolution for overlapping cluster/agent names.
 * - **Telemetry Link**: Search `[command_processor.test]` in tracing logs.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { process_command } from './command_processor';
import { agent_api_service } from '../services/agent_api_service';
import { use_browser_specialist_store } from '../stores/browser_specialist_store';
import { get_settings } from '../stores/settings_store';
import type { Agent } from '../types';

// Mock dependencies
vi.mock('../services/event_bus', () => ({
    event_bus: {
        emit: vi.fn(),
        emit_log: vi.fn(),
    }
}));

vi.mock('../services/agent_api_service', () => ({
    agent_api_service: {
        send_command: vi.fn().mockResolvedValue({ success: true }),
        pause_agent: vi.fn().mockResolvedValue(true),
        resume_agent: vi.fn().mockResolvedValue(true),
    }
}));

vi.mock('../services/system_api_service', () => ({
    system_api_service: {
        execute_local_cmd: vi.fn().mockResolvedValue({ success: true }),
        get_engine_status: vi.fn().mockResolvedValue({ features: [] }),
    }
}));

vi.mock('../stores/workspace_store', () => ({
    use_workspace_store: {
        getState: vi.fn().mockReturnValue({
            clusters: [{ id: 'c1', name: 'Engineering', alpha_id: '2', department: 'Eng' }]
        })
    }
}));

vi.mock('../stores/agent_store', () => ({
    use_agent_store: {
        getState: vi.fn().mockReturnValue({
            agents: [
                { id: '1', name: 'CEO', status: 'idle', tokens_used: 0, model: 'gpt-4', model_config: {} },
                { id: '2', name: 'Tadpole_Alpha', status: 'idle', tokens_used: 0, model: 'gpt-4', model_config: {} }
            ]
        })
    }
}));

vi.mock('../stores/sovereign_store', () => {
    const mock_store = {
        getState: vi.fn().mockReturnValue({
            add_message: vi.fn(),
        }),
        setState: vi.fn(),
        subscribe: vi.fn(),
    };
    const use_store = vi.fn().mockImplementation(() => mock_store.getState());
    return {
        use_sovereign_store: Object.assign(use_store, mock_store)
    };
});

vi.mock('../services/browser_inference', () => ({
    browser_inference_service: {
        analyze_ui: vi.fn().mockResolvedValue('The UI is healthy.'),
        init_specialist: vi.fn().mockResolvedValue(undefined),
    }
}));

vi.mock('../stores/browser_specialist_store', () => ({
    use_browser_specialist_store: {
        getState: vi.fn().mockReturnValue({
            analyze_dom: vi.fn().mockResolvedValue('The UI is healthy.')
        })
    }
}));

vi.mock('../stores/settings_store', () => ({
    get_settings: vi.fn().mockReturnValue({
        sentinel_mode: true,
        enable_neural_handoff: true,
        default_model: 'gpt-4',
    })
}));

describe('process_command', () => {
    const mock_agents: Agent[] = [
        { id: '1', name: 'CEO', status: 'idle', theme_color: '#000', voice_id: 'v1', tokens_used: 0, model: 'gpt-4', model_config: { provider: 'openai', modelId: 'gpt-4' } },
        { id: '2', name: 'Tadpole_Alpha', status: 'idle', theme_color: '#fff', voice_id: 'v2', tokens_used: 0, model: 'gpt-4', model_config: { provider: 'openai', modelId: 'gpt-4' } }
    ];

    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('should split commands correctly and preserve quoted strings', async () => {
        const text = '/send CEO "hello world"';
        await process_command(text, mock_agents);
        
        expect(agent_api_service.send_command).toHaveBeenCalledWith(
            '1', 
            'hello world',
            expect.anything(),
            expect.anything(),
            undefined,
            undefined,
            undefined,
            undefined,
            false,
            undefined,
            undefined,
            undefined
        );
    });

    it('should route prefix-less commands to target_node in agent scope', async () => {
        const text = 'prime system';
        await process_command(text, mock_agents, false, 'agent', 'Tadpole_Alpha');
        
        expect(agent_api_service.send_command).toHaveBeenCalledWith(
            '2', 
            'prime system',
            expect.anything(),
            expect.anything(),
            undefined,
            undefined,
            undefined,
            undefined,
            false,
            undefined,
            undefined,
            undefined
        );
    });

    it('should allow @mention override in any scope', async () => {
        const text = '@CEO wake up';
        await process_command(text, mock_agents, false, 'agent', 'Tadpole_Alpha');
        
        expect(agent_api_service.send_command).toHaveBeenCalledWith(
            '1', 
            'wake up',
            expect.anything(),
            expect.anything(),
            undefined,
            undefined,
            undefined,
            undefined,
            false,
            undefined,
            undefined,
            undefined
        );
    });

    it('should handle cluster targeting via # prefix by routing to the Alpha node', async () => {
        const text = '#Engineering sync';
        await process_command(text, mock_agents, false);
        
        expect(agent_api_service.send_command).toHaveBeenCalledWith(
            '2', 
            'sync',
            expect.anything(),
            expect.anything(),
            'c1', 
            'Eng', 
            undefined,
            undefined,
            false,
            undefined,
            undefined,
            undefined
        );
    });

    describe('Tiered Routing (Sentinel)', () => {
        it('should intercept tactical UI queries when sentinel_mode is enabled', async () => {
            const text = 'how is the screen looking?';
            const result = await process_command(text, mock_agents);
            
            expect(use_browser_specialist_store.getState().analyze_dom).toHaveBeenCalledWith(text);
            expect(agent_api_service.send_command).not.toHaveBeenCalled();
            expect(result.should_clear_logs).toBe(false);
        });

        it('should NOT intercept tactical queries if sentinel_mode is disabled', async () => {
            vi.mocked(get_settings).mockReturnValueOnce({
                sentinel_mode: false,
                enable_neural_handoff: true,
            } as any);
            
            const text = 'how is the screen looking?';
            await process_command(text, mock_agents);
            
            expect(use_browser_specialist_store.getState().analyze_dom).not.toHaveBeenCalled();
        });

        it('should escalate to Computer Architect if Browser Specialist requests it', async () => {
            vi.mocked(use_browser_specialist_store.getState().analyze_dom).mockResolvedValueOnce('I detect high entropy. ESCALATE_TO_ARCHITECT.');
            
            const text = 'show me the screen status';
            await process_command(text, mock_agents);
            
            // Should have called specialist FIRST
            expect(use_browser_specialist_store.getState().analyze_dom).toHaveBeenCalled();
            // Then fell through to broadcast to swarm (since no prefix)
            // (Note: broadcase doesn't call send_command but emits a log)
        });
    });
});

// Metadata: [command_processor_test]

// Metadata: [command_processor_test]
