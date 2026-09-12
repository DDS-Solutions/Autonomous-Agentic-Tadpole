/**
 * @docs ARCHITECTURE:TestSuites
 * 
 * ### AI Assist Note
 * **Verification of the Model Resolver and Provider Registry.** 
 * Validates the normalization of friendly model names (e.g., 'Gemini 3.1 Pro') into backend technical IDs and the heuristic resolution of AI providers based on naming patterns. 
 * Ensures that agent model configuration correctly respects multi-slot overrides and global intelligence synchronization.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Incorrect provider mapping leading to invalid API key usage or failure to apply global default model to legacy agents.
 * - **Telemetry Link**: Search `[model_utils_test]` in console logs.
 */

import { describe, it, expect } from 'vitest';
import { resolve_technical_model_id, resolve_provider, resolve_agent_model_config, get_model_color, get_active_model_name, get_agent_slot_model } from './model_utils';
import type { Agent } from '../types';

describe('model_utils', () => {
    describe('get_agent_slot_model', () => {
        it('resolves slot 1 across model_config.modelId, model_id, and model', () => {
            expect(get_agent_slot_model({ model_config: { modelId: 'gemini-1.5-pro' } } as unknown as Agent, 1)).toBe('gemini-1.5-pro');
            expect(get_agent_slot_model({ model_config: { model_id: 'gpt-4o' } } as unknown as Agent, 1)).toBe('gpt-4o');
            expect(get_agent_slot_model({ model: 'claude-3-5-sonnet' } as unknown as Agent, 1)).toBe('claude-3-5-sonnet');
        });

        it('resolves slot 2 across snake_case, camelCase wire DTOs, and model_2', () => {
            expect(get_agent_slot_model({ model_config2: { modelId: 'claude-3-5-sonnet' } } as unknown as Agent, 2)).toBe('claude-3-5-sonnet');
            expect(get_agent_slot_model({ modelConfig2: { modelId: 'gemma4:e4b' } } as unknown as Agent, 2)).toBe('gemma4:e4b');
            expect(get_agent_slot_model({ model_2: 'GPT-4o' } as unknown as Agent, 2)).toBe('GPT-4o');
            expect(get_agent_slot_model({ model2: 'deepseek-r1' } as unknown as Agent, 2)).toBe('deepseek-r1');
        });

        it('returns undefined for empty, whitespace, or "unknown" slot models', () => {
            expect(get_agent_slot_model({ model_config2: { modelId: 'unknown' } } as unknown as Agent, 2)).toBeUndefined();
            expect(get_agent_slot_model({ model_2: '' } as unknown as Agent, 2)).toBeUndefined();
            expect(get_agent_slot_model({ model_config3: { modelId: '   ' } } as unknown as Agent, 3)).toBeUndefined();
            expect(get_agent_slot_model(undefined, 1)).toBeUndefined();
        });
    });

    describe('resolve_technical_model_id', () => {
        it('resolves mapped names correctly', () => {
            expect(resolve_technical_model_id('Gemini 1.5 Pro')).toBe('gemini-1.5-pro');
            expect(resolve_technical_model_id('GPT-5.2')).toBe('gpt-5.2-preview');
        });

        it('returns original name if no mapping found', () => {
            expect(resolve_technical_model_id('Custom Model')).toBe('Custom Model');
        });

        it('returns unknown for empty/null input', () => {
            expect(resolve_technical_model_id(undefined)).toBe('unknown');
            expect(resolve_technical_model_id('')).toBe('unknown');
        });
    });

    describe('resolve_provider', () => {
        it('identifies core providers', () => {
            expect(resolve_provider('gpt-4')).toBe('openai');
            expect(resolve_provider('claude-3')).toBe('anthropic');
            expect(resolve_provider('gemini-pro')).toBe('google');
            expect(resolve_provider('ollama:llama3')).toBe('ollama');
        });

        it('identifies secondary vendors', () => {
            expect(resolve_provider('mistral-large')).toBe('mistral');
            expect(resolve_provider('deepseek-v3')).toBe('deepseek');
            expect(resolve_provider('grok-2')).toBe('xai');
        });

        it('handles groq/llama ambiguity', () => {
            expect(resolve_provider('llama-3-groq')).toBe('groq');
            expect(resolve_provider('llama-3-vanilla')).toBe('meta');
        });

        it('fallbacks to google by default', () => {
            expect(resolve_provider('mystery-ai')).toBe('google');
        });
    });

    describe('resolve_agent_model_config', () => {
        const base_agent: Partial<Agent> = {
            name: 'Test Agent',
            model: 'gemini-1.5-flash',
            active_model_slot: 1
        };

        it('resolves basic agent config', () => {
            const config = resolve_agent_model_config(base_agent as Agent);
            expect(config.model_id).toBe('gemini-1.5-flash');
            expect(config.provider).toBe('google');
        });

        it('respects global overrides for default agents', () => {
            const config = resolve_agent_model_config(base_agent as Agent, 'ollama:phi3');
            expect(config.model_id).toBe('ollama:phi3');
            expect(config.provider).toBe('ollama');
        });

        it('handles multi-slot overrides (Slot 2)', () => {
            const agent: Partial<Agent> = {
                ...base_agent,
                active_model_slot: 2,
                model_2: 'claude-3-sonnet',
                model_config2: { modelId: 'claude-3-sonnet', provider: 'anthropic', apiKey: 'key' }
            };
            const config = resolve_agent_model_config(agent as Agent);
            expect(config.model_id).toBe('claude-3-sonnet');
            expect(config.provider).toBe('anthropic');
        });

        it('handles multi-slot overrides (Slot 3)', () => {
            const agent: Partial<Agent> = {
                ...base_agent,
                active_model_slot: 3,
                model_3: 'gpt-4',
                model_config3: { modelId: 'gpt-4', provider: 'openai', apiKey: 'key' }
            };
            const config = resolve_agent_model_config(agent as Agent);
            expect(config.model_id).toBe('gpt-4');
            expect(config.provider).toBe('openai');
        });
    });

    describe('get_active_model_name', () => {
        it('prioritizes slot 1 model_config over wire fallback and resolves friendly name', () => {
            const agent: Partial<Agent> = {
                model: 'claude-3-5-sonnet',
                model_config: { modelId: 'gemini-2.5-flash', provider: 'google' },
                active_model_slot: 1
            };
            expect(get_active_model_name(agent as Agent)).toBe('Gemini 2.5 Flash');
        });

        it('prioritizes slot 2 model_config when slot 2 is active', () => {
            const agent: Partial<Agent> = {
                model: 'gemini-1.5-flash',
                model_2: 'claude-3-opus',
                model_config2: { modelId: 'gpt-4o', provider: 'openai' },
                active_model_slot: 2
            };
            expect(get_active_model_name(agent as Agent)).toBe('GPT-4o');
        });

        it('prioritizes slot 3 model_config when slot 3 is active', () => {
            const agent: Partial<Agent> = {
                model: 'gemini-1.5-flash',
                model_3: 'old-model',
                model_config3: { modelId: 'claude-3-5-sonnet', provider: 'anthropic' },
                active_model_slot: 3
            };
            expect(get_active_model_name(agent as Agent)).toBe('Claude 3.5 Sonnet');
        });

        it('falls back to model_config.model if modelId is not present', () => {
            const agent: Partial<Agent> = {
                model: 'fallback-model',
                model_config: { provider: 'google', modelId: '', model: 'gemini-2.5-flash' },
                active_model_slot: 1
            };
            expect(get_active_model_name(agent as Agent)).toBe('Gemini 2.5 Flash');
        });

        it('returns Unknown when all model slots and configs are missing', () => {
            const agent: Partial<Agent> = {
                model: '',
                active_model_slot: 1
            };
            expect(get_active_model_name(agent as Agent)).toBe('Unknown');
        });
    });

    describe('get_model_color', () => {
        it('returns correct Tailwind classes for known providers', () => {
            expect(get_model_color('gpt-4')).toContain('emerald');
            expect(get_model_color('claude')).toContain('zinc');
            expect(get_model_color('gemini')).toContain('green');
            expect(get_model_color('llama')).toContain('amber');
            expect(get_model_color('deepseek')).toContain('cyan');
        });

        it('returns fallback for unknown models', () => {
            expect(get_model_color('mystery')).toBe('text-zinc-400 border-zinc-800 bg-zinc-900');
        });
    });
});



// Metadata: [model_utils_test]
