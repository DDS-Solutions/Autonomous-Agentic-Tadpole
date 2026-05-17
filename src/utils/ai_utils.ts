/*
### AI Assist Note
**🛡️ Tadpole OS: Ai Utils**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

/**
 * @docs ARCHITECTURE:Utilities
 * 
 * ### AI Assist Note
 * **AI Utils**: Core utilities for AI context preparation and response normalization.
 * Implements **SEC-801 (Sovereign Context Shield)** to prevent prompt injection.
 */

/**
 * sanitize_ui_context
 * Redacts sensitive data and strips non-essential tags from DOM summaries
 * to prevent prompt injection and reduce token usage.
 */
export const sanitize_ui_context = (html: string): string => {
    if (!html) return '';

    let sanitized = html;

    // 1. Strip critical threat vectors
    sanitized = sanitized.replace(/<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi, '[SCRIPT_REDACTED]');
    sanitized = sanitized.replace(/<style\b[^<]*(?:(?!<\/style>)<[^<]*)*<\/style>/gi, '[STYLE_REDACTED]');
    sanitized = sanitized.replace(/<iframe\b[^<]*(?:(?!<\/iframe>)<[^<]*)*<\/iframe>/gi, '[IFRAME_REDACTED]');

    // 2. Redact sensitive attributes (passwords, tokens, values)
    // Focused on common input patterns to avoid accidental data leakage
    sanitized = sanitized.replace(/value="[^"]*"/gi, 'value="[VALUE_REDACTED]"');
    sanitized = sanitized.replace(/placeholder="[^"]*"/gi, 'placeholder="[PLACEHOLDER_REDACTED]"');

    // 3. Remove event handlers (security mitigation for model-induced execution)
    sanitized = sanitized.replace(/\son\w+="[^"]*"/gi, '');

    // 4. Clean up whitespace to save tokens
    sanitized = sanitized.replace(/\s+/g, ' ').trim();

    // 5. Token Limit Guard (approximate)
    // Ensures context doesn't overwhelm smaller local models
    const MAX_CHARS = 16000; // ~4k tokens
    if (sanitized.length > MAX_CHARS) {
        sanitized = sanitized.substring(0, MAX_CHARS) + '... [TRUNCATED]';
    }

    return sanitized;
};

/**
 * extract_neural_output
 * Robustly extracts the assistant's turn from a raw LLM completion.
 * Handles varied formatting from different local models (Gemma, Llama, etc).
 */
export const extract_neural_output = (raw: string, fallback: string = 'Analysis complete.'): string => {
    if (!raw) return fallback;

    // 1. Try to find content after formal turn markers
    const assistant_match = raw.match(/(?:ASSISTANT|Assistant):\s*([\s\S]*)/i);
    if (assistant_match && assistant_match[1].trim()) {
        return assistant_match[1].trim();
    }

    // 2. Support for DeepSeek/Gemma style thinking blocks
    const thought_end_match = raw.match(/<\/thought>\s*([\s\S]*)/i);
    if (thought_end_match && thought_end_match[1].trim()) {
        return thought_end_match[1].trim();
    }

    // 3. Fallback: If no system prompt is present, assume the whole text is assistant output
    if (!raw.includes('SYSTEM:') && !raw.includes('<DOM_STATE>')) {
        return raw.trim();
    }

    // 4. Brute force: Try to find the last paragraph if markers are missing
    const paragraphs = raw.split('\n\n').filter(p => p.trim());
    if (paragraphs.length > 0) {
        const last = paragraphs[paragraphs.length - 1].trim();
        if (!last.includes('USER:') && !last.includes('SYSTEM:')) {
            return last;
        }
    }

    return fallback;
};

// Metadata: [ai_utils]

// Metadata: [ai_utils]
