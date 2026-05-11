/**
 * @docs ARCHITECTURE:Domain
 * 
 * ### AI Assist Note
 * **Agent Serializers**: Pure functions for transforming Domain patches back to backend DTOs.
 * Centralizes the bidirectional mapping between snake_case (frontend) and camelCase (backend).
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Improper mapping of optional fields causing 400 Bad Request from Axum backend, or field-name drift during contract updates.
 * - **Telemetry Link**: Search `[Serializer]` in UI traces.
 */

import type { AgentPatch, AgentUpdateDto } from '../../contracts/agent';
import { resolve_technical_model_id } from '../../utils/model_utils';

/**
 * serialize_agent_update
 * Maps frontend Domain updates back to the backend AgentUpdateDto shape.
 * Implements the "One Merge Policy" for metadata vs first-class fields.
 */
export const serialize_agent_update = (patch: any): AgentUpdateDto => {
    const dto: AgentUpdateDto = {};

    // Telemetry for debugging model drift
    if (patch.model || patch.model_config) {
        console.debug(`[Serializer] Serializing model update: model=${patch.model}, config_id=${patch.model_config?.modelId}`);
    }

    if (patch.role !== undefined) dto.role = patch.role;
    if (patch.name !== undefined) dto.name = patch.name;
    if (patch.department !== undefined) dto.department = patch.department;
    
    // Model resolution logic: ALWAYS ensure we send the technical ID
    if (patch.model !== undefined) {
        dto.modelId = resolve_technical_model_id(patch.model);
        if (patch.model_config?.provider) {
            dto.provider = patch.model_config.provider;
        } else {
            const m = dto.modelId.toLowerCase();
            if (m.includes('gpt')) dto.provider = 'openai';
            else if (m.includes('claude')) dto.provider = 'anthropic';
            else if (m.includes('gemini')) dto.provider = 'google';
            else if (m.includes('llama') || m.includes('mixtral')) dto.provider = 'groq';
        }
    }

    if (patch.model_config !== undefined) {
        dto.modelId = patch.model_config.modelId || dto.modelId;
        dto.provider = patch.model_config.provider;
        dto.modelConfig = patch.model_config;
    }

    if (patch.theme_color !== undefined) dto.themeColor = patch.theme_color;
    if (patch.active_model_slot !== undefined) dto.activeModelSlot = patch.active_model_slot;
    
    // Slot 2 & 3 parity
    if (patch.model_2 !== undefined) {
        dto.model2 = resolve_technical_model_id(patch.model_2);
    }
    if (patch.model_3 !== undefined) {
        dto.model3 = resolve_technical_model_id(patch.model_3);
    }
    
    if (patch.model_config2 !== undefined) {
        dto.modelConfig2 = patch.model_config2;
        dto.model2 = patch.model_config2.modelId || dto.model2;
    }
    if (patch.model_config3 !== undefined) {
        dto.modelConfig3 = patch.model_config3;
        dto.model3 = patch.model_config3.modelId || dto.model3;
    }

    if (patch.budget_usd !== undefined) dto.budgetUsd = patch.budget_usd;
    
    if (patch.skills !== undefined) dto.skills = patch.skills;
    if (patch.workflows !== undefined) dto.workflows = patch.workflows;
    if (patch.mcp_tools !== undefined) dto.mcpTools = patch.mcp_tools;
    
    if (patch.voice_id !== undefined) dto.voiceId = patch.voice_id;
    if (patch.voice_engine !== undefined) dto.voiceEngine = patch.voice_engine;
    if (patch.category !== undefined) dto.category = patch.category;
    if (patch.created_at !== undefined) dto.createdAt = patch.created_at;
    
    if (patch.input_tokens !== undefined) dto.inputTokens = Math.floor(patch.input_tokens);
    if (patch.output_tokens !== undefined) dto.outputTokens = Math.floor(patch.output_tokens);
    if (patch.total_tokens !== undefined) dto.totalTokens = Math.floor(patch.total_tokens);
    if (
        patch.input_tokens !== undefined ||
        patch.output_tokens !== undefined ||
        patch.total_tokens !== undefined
    ) {
        dto.tokenUsage = {
            ...(patch.input_tokens !== undefined ? { inputTokens: Math.floor(patch.input_tokens) } : {}),
            ...(patch.output_tokens !== undefined ? { outputTokens: Math.floor(patch.output_tokens) } : {}),
            ...(patch.total_tokens !== undefined ? { totalTokens: Math.floor(patch.total_tokens) } : {}),
        };
    }
    if (patch.tokens_used !== undefined) dto.tokensUsed = Math.floor(patch.tokens_used);
    
    if (patch.current_task !== undefined) dto.currentTask = patch.current_task;
    if (patch.failure_count !== undefined) dto.failureCount = Math.floor(patch.failure_count);
    if (patch.connector_configs !== undefined) dto.connectorConfigs = patch.connector_configs;
    if (patch.metadata !== undefined) dto.metadata = patch.metadata;
    if (patch.requires_oversight !== undefined) dto.requiresOversight = patch.requires_oversight;

    return dto;
};
