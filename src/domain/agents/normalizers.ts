/**
 * @docs ARCHITECTURE:Domain
 * 
 * ### AI Assist Note
 * **Agent Normalizers**: Pure functions for transforming backend DTOs into Domain models.
 * Essential for absorbing breaking changes in the Rust API without impacting UI logic.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Failed parsing of JSON-stringified arrays (skills/workflows) or mapping drift between camelCase (wire) and snake_case (domain).
 * - **Telemetry Link**: Search `[Normalizer]` in UI traces.
 */

import type { 
    Agent, 
    AgentDto, 
    Department, 
    Agent_Status,
    Agent_Memory_Entry,
    Raw_Agent_Memory_Entry,
    Agent_Voice_Engine,
    Agent_Stt_Engine
} from '../../contracts/agent';
import { resolve_friendly_model_name } from '../../utils/model_utils';
import { get_settings } from '../../stores/settings_store';
import { use_trace_store } from '../../stores/trace_store';
import { v4 as uuidv4 } from 'uuid';


/**
 * RobustAgentDto
 * Internal type for the normalizer to handle both backend camelCase (DTO)
 * and legacy/mock snake_case properties without 'any' casting.
 */
interface RobustAgentDto extends Partial<AgentDto> {
    department?: string;
    tokens_used?: number;
    model_config?: Record<string, unknown>;
    workspace_path?: string;
    current_task?: string | null;
    mcp_tools?: string | string[];
    theme_color?: string;
    budget_usd?: number;
    cost_usd?: number;
    requires_oversight?: boolean;
    model_2?: string;
    model_3?: string;
    model_config2?: Record<string, unknown>;
    model_config3?: Record<string, unknown>;
    active_model_slot?: number;
    failure_count?: number;
    last_failure_at?: string;
    created_at?: string;
    last_pulse?: string | null;
    connector_configs?: Record<string, unknown>[];
    voice_id?: string;
    voice_engine?: Agent_Voice_Engine;
    stt_engine?: Agent_Stt_Engine;
    input_tokens?: number;
    output_tokens?: number;
    current_reasoning_turn?: number;
    reasoning_depth?: number;
}

/**
 * normalize_agent_dto
 * Transforms the raw backend representation (camelCase Wire DTO) 
 * into the authoritative frontend domain model (snake_case).
 * 
 * ### Robustness Pattern
 * 1. **Partial Merges**: Prioritizes `dto` fields, but falls back to `existing_agent` to prevent identity loss.
 * 2. **Dual-Case Support**: Checks both camelCase (API) and snake_case (Internal/Mock) properties.
 */
export const normalize_agent_dto = (dto: AgentDto, workspace_path?: string, existing_agent?: Agent): Agent => {
    use_trace_store.getState().add_span({
        id: uuidv4().substring(0, 16),
        trace_id: use_trace_store.getState().active_trace_id || uuidv4().replace(/-/g, ''),
        name: '[Normalizer] normalize_agent_dto',
        agent_id: dto.id || 'system',
        mission_id: 'system_sync',
        start_time: Date.now(),
        end_time: Date.now(),
        status: 'success',
        attributes: { agent_id: dto.id }
    });
    const d = dto as RobustAgentDto;
    
    const get_val = <T>(key_wire: keyof RobustAgentDto, key_domain: keyof RobustAgentDto, fallback: T): T => {
        const wire_val = d[key_wire] as unknown as T;
        if (wire_val !== undefined && wire_val !== null) return wire_val;
        
        const domain_val = d[key_domain] as unknown as T;
        if (domain_val !== undefined && domain_val !== null) return domain_val;
        
        if (existing_agent && (existing_agent as unknown as Record<string, unknown>)[key_domain] !== undefined) {
            return (existing_agent as unknown as Record<string, unknown>)[key_domain] as T;
        }
        return fallback;
    };

    const metadata = get_val<Record<string, unknown>>('metadata', 'metadata', {});
    const get_metadata_string = (key: string): string | undefined => {
        const value = metadata[key];
        return typeof value === 'string' && value.trim() ? value : undefined;
    };
    const get_non_empty_string = (
        key_wire: keyof RobustAgentDto,
        key_domain: keyof RobustAgentDto,
        fallback: string,
        metadata_key?: string,
    ): string => {
        const value = get_val<string | undefined>(key_wire, key_domain, undefined);
        if (typeof value === 'string' && value.trim()) return value;
        return (metadata_key ? get_metadata_string(metadata_key) : undefined) || fallback;
    };

    // 1. Department Normalization (Handles legacy mapping)
    const raw_dept = get_non_empty_string('department', 'department', 'Operations', 'department');
    const dept = (raw_dept === 'QA' ? 'Quality Assurance' : raw_dept) as Department;

    // 2. Status Mapping
    const raw_status = get_val<string>('status', 'status', 'idle');
    const status = (raw_status === 'working' ? 'active' : raw_status) as Agent_Status;
    
    const has_current_task = Object.prototype.hasOwnProperty.call(dto, 'currentTask') || Object.prototype.hasOwnProperty.call(dto, 'current_task');
    const current_task = (status === 'active')
        ? (has_current_task ? (d.currentTask ?? d.current_task ?? undefined) : existing_agent?.current_task)
        : (has_current_task ? (d.currentTask ?? d.current_task ?? undefined) : undefined);

    const parse_json_array = (wire_key: keyof RobustAgentDto, domain_key: keyof RobustAgentDto): string[] => {
        const val = get_val<string | string[]>(wire_key, domain_key, [] as string[]);
        if (Array.isArray(val)) return val;
        if (typeof val === 'string' && val.startsWith('[')) {
            try { return JSON.parse(val); } catch { return []; }
        }
        return [];
    };

    const settings = get_settings();
    const default_model = settings.default_model || 'Gemma 4 (Local)';

    // Model Resolution logic: 
    // Priority: 1. `model` wire field (live, derived from model_config.model_id in backend)
    //           2. `modelConfig.modelId` (authoritative config)
    //           3. `modelId` wire field (legacy identity field, can be stale)
    const model_name_wire = get_val<string | undefined>('model', 'model', undefined);
    const model_config_id = (get_val('modelConfig', 'model_config', undefined) as any)?.modelId as string | undefined;
    const model_id_wire = get_val<string | undefined>('modelId', 'modelId', undefined);
    
    // Use the live model name first (most accurate), then resolve from config/identity IDs
    const model = model_name_wire 
        || (model_config_id ? resolve_friendly_model_name(model_config_id) : undefined)
        || (model_id_wire ? resolve_friendly_model_name(model_id_wire) : undefined)
        || default_model;

    const input_tokens = (dto.tokenUsage?.inputTokens ?? d.input_tokens ?? existing_agent?.input_tokens ?? 0);
    const output_tokens = (dto.tokenUsage?.outputTokens ?? d.output_tokens ?? existing_agent?.output_tokens ?? 0);
    const tokens_used = (dto.tokenUsage?.totalTokens ?? d.tokensUsed ?? d.tokens_used ?? (input_tokens + output_tokens));

    return {
        id: dto.id || existing_agent?.id || 'unknown',
        name: get_non_empty_string('name', 'name', 'Unnamed Agent'),
        role: get_non_empty_string('role', 'role', 'AI Agent', 'role'),
        department: dept,
        description: get_val('description', 'description', ''),
        status: status,
        tokens_used,
        model: model,
        model_config: get_val('modelConfig', 'model_config', undefined),
        workspace_path: workspace_path || get_val('workspace', 'workspace_path', undefined),
        current_task: current_task || undefined,
        skills: parse_json_array('skills', 'skills'),
        workflows: parse_json_array('workflows', 'workflows'),
        mcp_tools: parse_json_array('mcpTools', 'mcp_tools'),
        theme_color: get_val('themeColor', 'theme_color', undefined),
        budget_usd: get_val('budgetUsd', 'budget_usd', 0),
        cost_usd: get_val('costUsd', 'cost_usd', 0),
        requires_oversight: get_val('requiresOversight', 'requires_oversight', false),
        model_2: (() => {
            const m2_name = get_val<string | undefined>('model2', 'model_2', undefined);
            const m2_config_id = (get_val('modelConfig2', 'model_config2', undefined) as any)?.modelId as string | undefined;
            return m2_name || (m2_config_id ? resolve_friendly_model_name(m2_config_id) : undefined) || undefined;
        })(),
        model_3: (() => {
            const m3_name = get_val<string | undefined>('model3', 'model_3', undefined);
            const m3_config_id = (get_val('modelConfig3', 'model_config3', undefined) as any)?.modelId as string | undefined;
            return m3_name || (m3_config_id ? resolve_friendly_model_name(m3_config_id) : undefined) || undefined;
        })(),
        model_config2: get_val('modelConfig2', 'model_config2', undefined),
        model_config3: get_val('modelConfig3', 'model_config3', undefined),
        active_model_slot: (get_val('activeModelSlot', 'active_model_slot', 1) as 1 | 2 | 3),
        failure_count: get_val('failureCount', 'failure_count', 0),
        last_failure_at: get_val('lastFailureAt', 'last_failure_at', undefined),
        created_at: get_val('createdAt', 'created_at', undefined),
        last_pulse: get_val('lastPulse', 'last_pulse', null),
        connector_configs: get_val('connectorConfigs', 'connector_configs', []),
        metadata,
        voice_id: get_val('voiceId', 'voice_id', undefined),
        voice_engine: get_val<Agent_Voice_Engine | undefined>('voiceEngine', 'voice_engine', undefined),
        stt_engine: get_val<Agent_Stt_Engine | undefined>('sttEngine', 'stt_engine', get_metadata_string('stt_engine') as Agent_Stt_Engine | undefined),
        input_tokens,
        output_tokens,
        category: get_val('category', 'category', 'user'),
        current_reasoning_turn: get_val('currentReasoningTurn', 'current_reasoning_turn', undefined),
        reasoning_depth: get_val('reasoningDepth', 'reasoning_depth', undefined),
    };
};

/**
 * normalize_agent_memory_entry
 * Transforms a raw memory entry into a structured domain model.
 */
export const normalize_agent_memory_entry = (raw: Raw_Agent_Memory_Entry): Agent_Memory_Entry => {
    use_trace_store.getState().add_span({
        id: uuidv4().substring(0, 16),
        trace_id: use_trace_store.getState().active_trace_id || uuidv4().replace(/-/g, ''),
        name: '[Normalizer] normalize_memory_entry',
        agent_id: 'system',
        mission_id: raw.mission_id || 'system_sync',
        start_time: Date.now(),
        end_time: Date.now(),
        status: 'success',
        attributes: { memory_id: raw.id }
    });
    return {
        id: raw.id,
        text: raw.text || raw.content || '',
        mission_id: raw.mission_id || '',
        timestamp: typeof raw.timestamp === 'string' ? new Date(raw.timestamp).getTime() : (raw.timestamp || Date.now()),
        metadata: raw.metadata || {}
    };
};

// Metadata: [normalizers]

// Metadata: [normalizers]
