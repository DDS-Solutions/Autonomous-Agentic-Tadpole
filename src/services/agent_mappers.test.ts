/**
 * @docs ARCHITECTURE:Contracts
 * 
 * ### AI Assist Note
 * **Test Suite**: Verifies the bidirectional mapping between frontend Domain models 
 * and backend Wire DTOs. Ensures that normalization logic handles edge cases 
 * like stringified JSON arrays and legacy department names.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Regression in snake_case to camelCase conversion or failed parsing of SQLx JSON fields.
 * - **Telemetry Link**: Search `[agent_mappers_test]` in console logs.
 */

import { describe, it, expect } from 'vitest';
import { normalize_agent_dto as from_backend_agent } from '../domain/agents/normalizers';
import { serialize_agent_update as to_agent_update_payload } from '../domain/agents/serializers';
import type { Agent } from '../contracts/agent/domain';

describe('agent_mappers', () => {
    describe('from_backend_agent', () => {
        it('should correctly normalize a raw backend agent', () => {
            const raw = {
                id: 'agent-1',
                name: 'Test Agent',
                role: 'Analyst',
                department: 'QA',
                budgetUsd: 100,
                costUsd: 10,
                tokensUsed: 1000,
                tokenUsage: {
                    inputTokens: 400,
                    outputTokens: 600,
                    totalTokens: 1000
                },
                skills: JSON.stringify(['skill1', 'skill2']),
                workflows: JSON.stringify(['flow1']),
                metadata: { custom: 'data' },
                model: 'gpt-4o',
                modelConfig: {
                    provider: 'openai',
                    temperature: 0.7
                }
            };

            const normalized = from_backend_agent(raw as any, './workspaces/test');
            
            expect(normalized.id).toBe('agent-1');
            expect(normalized.budget_usd).toBe(100);
            expect(normalized.skills).toContain('skill1');
            expect(normalized.model_config?.provider).toBe('openai');
            expect(normalized.workspace_path).toBe('./workspaces/test');
        });
    });

    describe('from_backend_agent (Robustness)', () => {
        it('should handle hybrid DTOs (mixed snake and camel case)', () => {
            const hybrid = {
                id: 'agent-hybrid',
                name: 'Hybrid Agent',
                role: 'Hybrid',
                department: 'Operations',
                budget_usd: 500, // snake
                costUsd: 50,      // camel
                tokens_used: 1000,
                modelId: 'gpt-4o' // camel
            };

            const normalized = from_backend_agent(hybrid as any);
            
            expect(normalized.budget_usd).toBe(500);
            expect(normalized.model).toBe('GPT-4o');
        });

        it('should preserve identity from existing_agent when DTO has missing fields', () => {
            const existing: Agent = {
                id: 'agent-1',
                name: 'Original Name',
                role: 'Original Role',
                department: 'Engineering',
                status: 'idle',
                tokens_used: 0,
                model: 'claude-3',
                skills: ['old-skill'],
                workflows: [],
                budget_usd: 100,
                category: 'general'
            };

            const partial_dto = {
                id: 'agent-1',
                name: 'Updated Name',
                // missing role, department, etc.
            };

            const normalized = from_backend_agent(partial_dto as any, undefined, existing);
            
            expect(normalized.name).toBe('Updated Name');
            expect(normalized.role).toBe('Original Role'); // Preserved
            expect(normalized.budget_usd).toBe(100);       // Preserved
        });

        it('should map legacy department names', () => {
            const legacy = { id: 'a', department: 'QA' };
            const normalized = from_backend_agent(legacy as any);
            expect(normalized.department).toBe('Quality Assurance');
        });

        it('should handle stringified skills from SQLx', () => {
            const raw = { id: 'a', skills: '["rust", "ts"]' };
            const normalized = from_backend_agent(raw as any);
            expect(normalized.skills).toEqual(['rust', 'ts']);
        });
        it('should prioritize modelConfig, modelConfig2, and modelConfig3 modelId and resolve friendly names', () => {
            const raw = {
                id: 'agent-tri-slot',
                name: 'Tri Slot Agent',
                model: 'claude-3-5-sonnet',
                model2: 'gpt-4o-mini',
                model3: 'unknown',
                modelConfig: {
                    modelId: 'gemini-2.5-flash',
                    provider: 'google',
                    temperature: 0.7
                },
                modelConfig2: {
                    modelId: 'gpt-4o',
                    provider: 'openai',
                    temperature: 0.5
                },
                modelConfig3: {
                    modelId: 'claude-3-5-sonnet',
                    provider: 'anthropic',
                    temperature: 0.9
                }
            };

            const normalized = from_backend_agent(raw as any);

            expect(normalized.model).toBe('Gemini 2.5 Flash');
            expect(normalized.model_2).toBe('GPT-4o');
            expect(normalized.model_3).toBe('Claude 3.5 Sonnet');
            expect(normalized.model_config?.modelId).toBe('gemini-2.5-flash');
            expect(normalized.model_config2?.modelId).toBe('gpt-4o');
            expect(normalized.model_config3?.modelId).toBe('claude-3-5-sonnet');
        });
    });

    describe('to_agent_update_payload', () => {
        it('should map frontend partials to backend DTOs with camelCase', () => {
            const updates: Partial<Agent> = {
                name: 'New Name',
                budget_usd: 500,
                skills: ['new-skill']
            };

            const payload = to_agent_update_payload(updates as any);
            
            expect(payload.name).toBe('New Name');
            expect(payload.budgetUsd).toBe(500);
            expect(payload.skills).toEqual(['new-skill']);
            // Ensure snake_case from Agent is mapped to camelCase in payload
            expect(payload).toHaveProperty('budgetUsd');
            expect(payload).not.toHaveProperty('budget_usd');
        });

        it('should serialize model_config, model_config2, and model_config3 to backend DTO', () => {
            const updates: Partial<Agent> = {
                model: 'Gemini 2.5 Flash',
                model_config: {
                    modelId: 'gemini-2.5-flash',
                    provider: 'google',
                    temperature: 0.7
                },
                model_2: 'GPT-4o',
                model_config2: {
                    modelId: 'gpt-4o',
                    provider: 'openai',
                    temperature: 0.5
                },
                model_3: 'Claude 3.5 Sonnet',
                model_config3: {
                    modelId: 'claude-3-5-sonnet',
                    provider: 'anthropic',
                    temperature: 0.9
                }
            };

            const payload = to_agent_update_payload(updates as any);

            expect(payload.modelId).toBe('gemini-2.5-flash');
            expect(payload.modelConfig?.modelId).toBe('gemini-2.5-flash');
            expect(payload.model2).toBe('gpt-4o');
            expect(payload.modelConfig2?.modelId).toBe('gpt-4o');
            expect(payload.model3).toBe('claude-3-5-sonnet');
            expect(payload.modelConfig3?.modelId).toBe('claude-3-5-sonnet');
        });
    });

    describe('AgentFormState Lifecycle', () => {
        it('buildAgentFormState extracts slot models accurately and resolves friendly names', async () => {
            const { buildAgentFormState } = await import('../domain/agents/form_state');
            const agent: Agent = {
                id: 'agent-1',
                name: 'Test Agent',
                role: 'Analyst',
                department: 'Operations',
                status: 'idle',
                tokens_used: 0,
                model: 'gemma4:e4b',
                model_config: { modelId: 'gemma4:e4b', provider: 'ollama', temperature: 0.7 },
                model_2: 'claude-3-5-sonnet',
                model_config2: { modelId: 'claude-3-5-sonnet', provider: 'anthropic', temperature: 0.5 },
                active_model_slot: 2,
                skills: [],
                workflows: []
            };

            const form_state = buildAgentFormState(agent);
            expect(form_state.slots.primary.model).toBe('Gemma 4 (Local)');
            expect(form_state.slots.secondary.model).toBe('Claude 3.5 Sonnet');
            expect(form_state.slots.tertiary.model).toBe('');
            expect(form_state.active_model_slot).toBe(2);
            expect(form_state.active_tab).toBe('secondary');
        });

        it('serializeFormState preserves active_model_slot and does NOT create "unknown" configs for empty slots', async () => {
            const { buildAgentFormState, serializeFormState } = await import('../domain/agents/form_state');
            const agent: Agent = {
                id: 'agent-1',
                name: 'Test Agent',
                role: 'Analyst',
                department: 'Operations',
                status: 'idle',
                tokens_used: 0,
                model: 'Gemini 1.5 Flash',
                model_config: { modelId: 'gemini-1.5-flash', provider: 'google', temperature: 0.7 },
                active_model_slot: 1,
                skills: [],
                workflows: []
            };

            const form_state = buildAgentFormState(agent);
            // Simulate user switching to secondary tab to view settings
            form_state.active_tab = 'secondary';
            // Slot 2 remains empty
            form_state.slots.secondary.model = '';

            const serialized = serializeFormState(form_state);
            // active_model_slot must NOT be hijacked by active_tab!
            expect(serialized.active_model_slot).toBe(1);
            // Slot 2 must be undefined, NOT { modelId: 'unknown' }!
            expect(serialized.model_2).toBeUndefined();
            expect(serialized.model_config2).toBeUndefined();
        });
    });
});



// Metadata: [agent_mappers_test]
