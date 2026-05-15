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
export const serialize_agent_update = (patch: AgentPatch): AgentUpdateDto => {
    const dto: AgentUpdateDto = {};

    if (patch.role !== undefined) dto.role = patch.role;
    if (patch.name !== undefined) dto.name = patch.name;
    if (patch.department !== undefined) dto.department = patch.department;
    
    // 🧠 Model Slot 1 (Primary)
    // We prioritize the technical ID for the backend persistence layer.
    if (patch.model !== undefined) {
        dto.modelId = resolve_technical_model_id(patch.model);
        
        // Auto-resolve provider if missing but model is present
        if (!patch.model_config?.provider) {
            const m = (dto.modelId || '').toLowerCase();
            if (m.includes('gpt')) dto.provider = 'openai';
            else if (m.includes('claude')) dto.provider = 'anthropic';
            else if (m.includes('gemini')) dto.provider = 'google';
            else if (m.includes('llama') || m.includes('mixtral')) dto.provider = 'groq';
            else if (m.includes('mistral')) dto.provider = 'mistral';
            else if (m.includes('deepseek')) dto.provider = 'deepseek';
        }
    }

    if (patch.model_config !== undefined) {
        const cfg = patch.model_config;
        dto.modelConfig = {
            modelId: cfg.modelId || dto.modelId || resolve_technical_model_id(patch.model),
            provider: (cfg.provider || dto.provider || 'ollama').toLowerCase(),
            temperature: typeof cfg.temperature === 'number' && !isNaN(cfg.temperature) ? cfg.temperature : 0.7,
            systemPrompt: cfg.systemPrompt || '',
            reasoningDepth: typeof cfg.reasoningDepth === 'number' && !isNaN(cfg.reasoningDepth) ? Math.floor(cfg.reasoningDepth) : 1,
            actThreshold: typeof cfg.actThreshold === 'number' && !isNaN(cfg.actThreshold) ? cfg.actThreshold : 0.9,
            skills: Array.isArray(cfg.skills) ? cfg.skills : [],
            workflows: Array.isArray(cfg.workflows) ? cfg.workflows : [],
            ...(cfg.baseUrl ? { baseUrl: cfg.baseUrl } : {})
        };
        // Top-level parity for simple fields
        dto.modelId = dto.modelConfig.modelId;
        dto.provider = dto.modelConfig.provider;
        dto.reasoningDepth = dto.modelConfig.reasoningDepth;
    }

    // 🧠 Model Slot 2 (Secondary)
    if (patch.model_2 !== undefined) {
        dto.model2 = resolve_technical_model_id(patch.model_2);
    }
    if (patch.model_config2 !== undefined) {
        const cfg = patch.model_config2;
        dto.modelConfig2 = {
            modelId: cfg.modelId || dto.model2 || resolve_technical_model_id(patch.model_2),
            provider: (cfg.provider || 'ollama').toLowerCase(),
            temperature: typeof cfg.temperature === 'number' && !isNaN(cfg.temperature) ? cfg.temperature : 0.5,
            systemPrompt: cfg.systemPrompt || '',
            reasoningDepth: typeof cfg.reasoningDepth === 'number' && !isNaN(cfg.reasoningDepth) ? Math.floor(cfg.reasoningDepth) : 1,
            actThreshold: typeof cfg.actThreshold === 'number' && !isNaN(cfg.actThreshold) ? cfg.actThreshold : 0.9,
            skills: Array.isArray(cfg.skills) ? cfg.skills : [],
            workflows: Array.isArray(cfg.workflows) ? cfg.workflows : [],
            ...(cfg.baseUrl ? { baseUrl: cfg.baseUrl } : {})
        };
        dto.model2 = dto.modelConfig2.modelId;
    }

    // 🧠 Model Slot 3 (Tertiary)
    if (patch.model_3 !== undefined) {
        dto.model3 = resolve_technical_model_id(patch.model_3);
    }
    if (patch.model_config3 !== undefined) {
        const cfg = patch.model_config3;
        dto.modelConfig3 = {
            modelId: cfg.modelId || dto.model3 || resolve_technical_model_id(patch.model_3),
            provider: (cfg.provider || 'ollama').toLowerCase(),
            temperature: typeof cfg.temperature === 'number' && !isNaN(cfg.temperature) ? cfg.temperature : 0.9,
            systemPrompt: cfg.systemPrompt || '',
            reasoningDepth: typeof cfg.reasoningDepth === 'number' && !isNaN(cfg.reasoningDepth) ? Math.floor(cfg.reasoningDepth) : 1,
            actThreshold: typeof cfg.actThreshold === 'number' && !isNaN(cfg.actThreshold) ? cfg.actThreshold : 0.9,
            skills: Array.isArray(cfg.skills) ? cfg.skills : [],
            workflows: Array.isArray(cfg.workflows) ? cfg.workflows : [],
            ...(cfg.baseUrl ? { baseUrl: cfg.baseUrl } : {})
        };
        dto.model3 = dto.modelConfig3.modelId;
    }

    if (patch.theme_color !== undefined) dto.themeColor = patch.theme_color;
    if (patch.active_model_slot !== undefined) dto.activeModelSlot = patch.active_model_slot;
    if (patch.budget_usd !== undefined) dto.budgetUsd = patch.budget_usd;
    
    // Capabilities
    if (patch.skills !== undefined) dto.skills = Array.isArray(patch.skills) ? patch.skills : [patch.skills];
    if (patch.workflows !== undefined) dto.workflows = Array.isArray(patch.workflows) ? patch.workflows : [patch.workflows];
    if (patch.mcp_tools !== undefined) dto.mcpTools = Array.isArray(patch.mcp_tools) ? patch.mcp_tools : [patch.mcp_tools];
    
    if (patch.voice_id !== undefined) dto.voiceId = patch.voice_id;
    if (patch.voice_engine !== undefined) dto.voiceEngine = patch.voice_engine;
    if (patch.stt_engine !== undefined) dto.sttEngine = patch.stt_engine;
    if (patch.category !== undefined) dto.category = patch.category;
    if (patch.created_at !== undefined) dto.createdAt = patch.created_at;
    
    // Token Management (Flat fields for AgentConfigUpdate parity)
    if (patch.input_tokens !== undefined) dto.inputTokens = Math.floor(patch.input_tokens);
    if (patch.output_tokens !== undefined) dto.outputTokens = Math.floor(patch.output_tokens);
    if (patch.total_tokens !== undefined) dto.totalTokens = Math.floor(patch.total_tokens);
    
    if (
        patch.input_tokens !== undefined ||
        patch.output_tokens !== undefined ||
        patch.total_tokens !== undefined
    ) {
        dto.tokenUsage = {
            inputTokens: typeof patch.input_tokens === 'number' ? Math.floor(patch.input_tokens) : 0,
            outputTokens: typeof patch.output_tokens === 'number' ? Math.floor(patch.output_tokens) : 0,
            totalTokens: typeof patch.total_tokens === 'number' ? Math.floor(patch.total_tokens) : 0,
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

