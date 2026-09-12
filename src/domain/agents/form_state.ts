/**
 * @docs ARCHITECTURE:Domain
 * 
 * ### AI Assist Note
 * **Agent Form State Logic**: Dedicated helpers for converting between 
 * flat Domain models and grouped UI Form state.
 * Prevents logic duplication in the configuration component.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Status mismatch between domain and form, or incorrect slot mapping (primary/secondary/tertiary).
 * - **Telemetry Link**: Search `[form_state]` in UI traces.
 */

console.debug("[FormState] Domain logic loaded");

import { resolve_technical_model_id, resolve_friendly_model_name, get_agent_slot_model, resolve_provider } from '../../utils/model_utils';
import type { Agent, AgentFormState, AgentPatch, Department } from '../../contracts/agent';
import { slugify_role } from '../../utils/agent_uiutils';
import { use_settings_store } from '../../stores/settings_store';

/** Shape of legacy/mixed model_config objects that may have either camelCase or snake_case keys. */
type LegacyModelConfig = {
    systemPrompt?: string; system_prompt?: string;
    reasoningDepth?: number; reasoning_depth?: number;
    actThreshold?: number; act_threshold?: number;
    baseUrl?: string; base_url?: string;
    skills?: string[];
    workflows?: string[];
};

/**
 * buildAgentFormState
 * Converts a Domain Agent into a ready-to-use Form State for the configuration UI.
 */
export const buildAgentFormState = (agent: Agent): AgentFormState => {
    // Retrieve system level default model
    const settings = use_settings_store.getState().settings;
    const system_default_model = settings?.default_model || 'Gemini 3 Pro';

    const raw_active = get_agent_slot_model(agent, 1) || agent.model || system_default_model;
    const raw_secondary = get_agent_slot_model(agent, 2) || '';
    const raw_tertiary = get_agent_slot_model(agent, 3) || '';

    const active_model = resolve_friendly_model_name(raw_active) || raw_active;
    const secondary_model = raw_secondary ? (resolve_friendly_model_name(raw_secondary) || raw_secondary) : '';
    const tertiary_model = raw_tertiary ? (resolve_friendly_model_name(raw_tertiary) || raw_tertiary) : '';

    // Phase 4: Ensure provider/model consistency on hydration
    const primary_provider = agent.model_config?.provider || (agent as unknown as { modelConfig?: { provider?: string } }).modelConfig?.provider || resolve_provider(active_model);
    const secondary_provider = agent.model_config2?.provider || (agent as unknown as { modelConfig2?: { provider?: string } }).modelConfig2?.provider || resolve_provider(secondary_model || 'claude');
    const tertiary_provider = agent.model_config3?.provider || (agent as unknown as { modelConfig3?: { provider?: string } }).modelConfig3?.provider || resolve_provider(tertiary_model || 'llama');

    return {
        main_tab: 'cognition',
        active_tab: agent.active_model_slot === 2 ? 'secondary' : agent.active_model_slot === 3 ? 'tertiary' : 'primary',
        active_model_slot: (agent.active_model_slot as 1 | 2 | 3) || 1,
        identity: {
            name: agent.name,
            role: slugify_role(agent.role || ''),
            department: agent.department
        },
        voice: {
            voice_id: agent.voice_id || 'alloy',
            voice_engine: agent.voice_engine || 'browser',
            stt_engine: agent.stt_engine || 'groq'
        },
        slots: {
            primary: {
                provider: primary_provider,
                model: active_model,
                temperature: agent.model_config?.temperature ?? 0.7,
                system_prompt: (agent.model_config as LegacyModelConfig)?.systemPrompt ?? (agent.model_config as LegacyModelConfig)?.system_prompt ?? '',
                reasoning_depth: (agent.model_config as LegacyModelConfig)?.reasoningDepth ?? (agent.model_config as LegacyModelConfig)?.reasoning_depth ?? 1,
                act_threshold: (agent.model_config as LegacyModelConfig)?.actThreshold ?? (agent.model_config as LegacyModelConfig)?.act_threshold ?? 0.9,
                skills: agent.model_config?.skills ?? agent.skills ?? [],
                workflows: agent.model_config?.workflows ?? agent.workflows ?? [],
                base_url: agent.model_config?.baseUrl ?? (agent.model_config as LegacyModelConfig)?.base_url ?? ''
            },
            secondary: {
                provider: secondary_provider,
                model: secondary_model,
                temperature: agent.model_config2?.temperature ?? 0.5,
                system_prompt: (agent.model_config2 as LegacyModelConfig)?.systemPrompt ?? (agent.model_config2 as LegacyModelConfig)?.system_prompt ?? '',
                reasoning_depth: (agent.model_config2 as LegacyModelConfig)?.reasoningDepth ?? (agent.model_config2 as LegacyModelConfig)?.reasoning_depth ?? 1,
                act_threshold: (agent.model_config2 as LegacyModelConfig)?.actThreshold ?? (agent.model_config2 as LegacyModelConfig)?.act_threshold ?? 0.9,
                skills: agent.model_config2?.skills ?? [],
                workflows: agent.model_config2?.workflows ?? [],
                base_url: agent.model_config2?.baseUrl ?? (agent.model_config2 as LegacyModelConfig)?.base_url ?? ''
            },
            tertiary: {
                provider: tertiary_provider,
                model: tertiary_model,
                temperature: agent.model_config3?.temperature ?? 0.9,
                system_prompt: (agent.model_config3 as LegacyModelConfig)?.systemPrompt ?? (agent.model_config3 as LegacyModelConfig)?.system_prompt ?? '',
                reasoning_depth: (agent.model_config3 as LegacyModelConfig)?.reasoningDepth ?? (agent.model_config3 as LegacyModelConfig)?.reasoning_depth ?? 1,
                act_threshold: (agent.model_config3 as LegacyModelConfig)?.actThreshold ?? (agent.model_config3 as LegacyModelConfig)?.act_threshold ?? 0.9,
                skills: agent.model_config3?.skills ?? [],
                workflows: agent.model_config3?.workflows ?? [],
                base_url: agent.model_config3?.baseUrl ?? (agent.model_config3 as LegacyModelConfig)?.base_url ?? ''
            }
        },
        mcp_tools: agent.mcp_tools || [],
        governance: {
            budget_usd: agent.budget_usd || 0,
            requires_oversight: agent.requires_oversight || false
        },
        ui: {
            direct_message: '',
            saving: false,
            theme_color: agent.theme_color || '#10b981',
            new_role_name: '',
            show_promote: false
        },
        connector_configs: agent.connector_configs || []
    };
};

/**
 * serializeFormState
 * Aggregates hierarchical form state back into a flat Partial<Agent> (Domain Patch) for persistence.
 */
export const serializeFormState = (state: AgentFormState): AgentPatch => {
    const { identity, slots, voice, ui, governance, mcp_tools, connector_configs } = state;
    
    const primary_model_clean = slots.primary.model && slots.primary.model.toLowerCase() !== 'unknown' ? slots.primary.model : undefined;
    const secondary_model_clean = slots.secondary.model && slots.secondary.model.toLowerCase() !== 'unknown' ? slots.secondary.model : undefined;
    const tertiary_model_clean = slots.tertiary.model && slots.tertiary.model.toLowerCase() !== 'unknown' ? slots.tertiary.model : undefined;

    return {
        name: identity.name,
        role: identity.role,
        department: identity.department as Department,
        budget_usd: governance.budget_usd,
        requires_oversight: governance.requires_oversight,
        voice_id: voice.voice_id,
        voice_engine: voice.voice_engine,
        stt_engine: voice.stt_engine,
        theme_color: ui.theme_color,
        mcp_tools,
        connector_configs,
        model: primary_model_clean ? (resolve_friendly_model_name(primary_model_clean) || primary_model_clean) : undefined,
        model_2: secondary_model_clean ? (resolve_friendly_model_name(secondary_model_clean) || secondary_model_clean) : undefined,
        model_3: tertiary_model_clean ? (resolve_friendly_model_name(tertiary_model_clean) || tertiary_model_clean) : undefined,
        model_config: primary_model_clean ? {
            modelId: resolve_technical_model_id(primary_model_clean),
            provider: slots.primary.provider,
            temperature: slots.primary.temperature,
            systemPrompt: slots.primary.system_prompt,
            reasoningDepth: slots.primary.reasoning_depth,
            actThreshold: slots.primary.act_threshold,
            skills: slots.primary.skills,
            workflows: slots.primary.workflows,
            ...(slots.primary.base_url ? { baseUrl: slots.primary.base_url } : {})
        } : undefined,
        model_config2: secondary_model_clean ? {
            modelId: resolve_technical_model_id(secondary_model_clean),
            provider: slots.secondary.provider,
            temperature: slots.secondary.temperature,
            systemPrompt: slots.secondary.system_prompt,
            reasoningDepth: slots.secondary.reasoning_depth,
            actThreshold: slots.secondary.act_threshold,
            skills: slots.secondary.skills,
            workflows: slots.secondary.workflows,
            ...(slots.secondary.base_url ? { baseUrl: slots.secondary.base_url } : {})
        } : undefined,
        model_config3: tertiary_model_clean ? {
            modelId: resolve_technical_model_id(tertiary_model_clean),
            provider: slots.tertiary.provider,
            temperature: slots.tertiary.temperature,
            systemPrompt: slots.tertiary.system_prompt,
            reasoningDepth: slots.tertiary.reasoning_depth,
            actThreshold: slots.tertiary.act_threshold,
            skills: slots.tertiary.skills,
            workflows: slots.tertiary.workflows,
            ...(slots.tertiary.base_url ? { baseUrl: slots.tertiary.base_url } : {})
        } : undefined,
        active_model_slot: state.active_model_slot ?? 1,
        skills: Array.from(new Set([
            ...slots.primary.skills,
            ...slots.secondary.skills,
            ...slots.tertiary.skills
        ])),
        workflows: Array.from(new Set([
            ...slots.primary.workflows,
            ...slots.secondary.workflows,
            ...slots.tertiary.workflows
        ]))
    };
};



// Metadata: [form_state]
