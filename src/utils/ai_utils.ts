/*
### AI Assist Note
**🛡️ Tadpole OS: Ai Utils**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Search `[ai_utils]` in console logs.
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

    // 1.5. Strip ChatML control tokens and DOM fence breakouts (SEC-801: ChatML injection shield)
    sanitized = sanitized
        .replace(/<\|im_start\|>/gi, '')
        .replace(/<\|im_end\|>/gi, '')
        .replace(/<\|endoftext\|>/gi, '')
        .replace(/<\/?DOM_STATE>/gi, '')
        .replace(/\bESCALATE_TO_ARCHITECT\b/gi, '[REDACTED_ESCALATION_TOKEN]')
        .replace(/\bSENTINEL_SCAN\b/gi, 'SECURITY_SCAN');

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
 * Prunes repetitive degenerative loops and ragged truncation fragments
 * from model output.
 */
export const prune_repetition_and_truncation = (text: string): string => {
    if (!text || text.trim().length === 0) return text;

    let cleaned = text.trim();

    // 1. Detect and collapse phrase repetitions (e.g., repeated phrases of 12+ chars)
    cleaned = cleaned.replace(/(.{12,}?)\s*(?:\1\s*)+/gs, '$1');

    // 2. Sentence-level cycle detection
    const sentence_regex = /[^.!?\n]+(?:[.!?]+|$)/g;
    const matches = cleaned.match(sentence_regex);

    if (matches && matches.length > 1) {
        const unique_sentences: string[] = [];
        const seen_normalized = new Set<string>();
        let has_loop = false;

        for (let i = 0; i < matches.length; i++) {
            const raw_sentence = matches[i].trim();
            if (!raw_sentence) continue;

            const normalized = raw_sentence.toLowerCase().replace(/[^a-z0-9]/g, '');
            // Only enforce cycle detection on meaningful sentences (length > 10 chars)
            if (normalized.length > 10) {
                if (seen_normalized.has(normalized)) {
                    has_loop = true;
                    break; // Terminate generation at the start of the first repeat loop
                }
                seen_normalized.add(normalized);
            }
            unique_sentences.push(raw_sentence);
        }

        if (has_loop && unique_sentences.length > 0) {
            cleaned = unique_sentences.join(' ').trim();
        }
    }

    // 3. Trim ragged trailing truncation fragments if we have at least one complete sentence
    const terminal_punct_regex = /[.!?]["']?$/;
    if (!terminal_punct_regex.test(cleaned)) {
        const last_punct_index = Math.max(
            cleaned.lastIndexOf('.'),
            cleaned.lastIndexOf('!'),
            cleaned.lastIndexOf('?')
        );
        if (last_punct_index > 15) {
            cleaned = cleaned.substring(0, last_punct_index + 1).trim();
        }
    }

    return cleaned;
};

/**
 * extract_neural_output
 * Robustly extracts the assistant's turn from a raw LLM completion.
 * Handles varied formatting from different local models (Gemma, Llama, etc),
 * breaks degenerative repetition loops, and trims ragged token-limit truncations.
 */
export const extract_neural_output = (raw: string, fallback: string = 'Analysis complete.'): string => {
    if (!raw) return fallback;

    let result = fallback;

    // 1. Try to find content after formal turn markers
    const assistant_match = raw.match(/(?:ASSISTANT|Assistant):\s*([\s\S]*)/i);
    if (assistant_match && assistant_match[1].trim()) {
        result = assistant_match[1].trim();
    } else {
        // 2. Support for DeepSeek/Gemma style thinking blocks
        const thought_end_match = raw.match(/<\/thought>\s*([\s\S]*)/i);
        if (thought_end_match && thought_end_match[1].trim()) {
            result = thought_end_match[1].trim();
        } else if (!raw.includes('SYSTEM:') && !raw.includes('<DOM_STATE>')) {
            // 3. Fallback: If no system prompt is present, assume the whole text is assistant output
            result = raw.trim();
        } else {
            // 4. Brute force: Try to find the last paragraph if markers are missing
            const paragraphs = raw.split('\n\n').filter(p => p.trim());
            if (paragraphs.length > 0) {
                const last = paragraphs[paragraphs.length - 1].trim();
                if (!last.includes('USER:') && !last.includes('SYSTEM:')) {
                    result = last;
                }
            }
        }
    }

    if (result === fallback) return fallback;
    const sanitized = prune_repetition_and_truncation(result);
    return sanitized || fallback;
};





// Metadata: [ai_utils]
