/**
 * @docs ARCHITECTURE:Infrastructure
 * 
 * ### AI Assist Note
 * **Root/Core Constants Hub**: Consolidated registry for global enums, provider IDs,
 * default model configurations, route definitions, and design system themes.
 * Part of the Tadpole-OS core layer.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Incorrect provider string mapping or missing model IDs when a new service is integrated.
 * - **Telemetry Link**: Search `[constants]` in source audits.
 */

export const PROVIDERS = {
    GOOGLE: 'google',
    OPENAI: 'openai',
    ANTHROPIC: 'anthropic',
    GROQ: 'groq',
    OLLAMA: 'ollama',
    INCEPTION: 'inception',
    LOCAL: 'local',
} as const;

export type ProviderType = typeof PROVIDERS[keyof typeof PROVIDERS];

export const DEFAULT_PROVIDER = PROVIDERS.OLLAMA;

export const MODEL_IDS = {
    GEMINI_PRO: 'gemini-pro',
    GEMINI_FLASH: 'gemini-2.0-flash',
    CLAUDE_OPUS: 'claude-3-opus-20240229',
    GPT4_O: 'gpt-4o',
    GEMMA4: 'gemma4:e4b',
} as const;

export type ModelIdType = typeof MODEL_IDS[keyof typeof MODEL_IDS];

// Re-export domain-specific constants for consolidated import access
export * from './routes';
export * from './theme';

// Metadata: [constants]
