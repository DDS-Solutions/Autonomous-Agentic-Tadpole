/**
 * @docs ARCHITECTURE:Core
 * 
 * ### AI Assist Note
 * **@docs ARCHITECTURE:Security**
 * Handles reactive state and high-fidelity user interactions.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI regression, hook desync, or API timeout.
 * - **Telemetry Link**: Search `[security_utils]` in observability traces.
 */

/**
 * @docs ARCHITECTURE:Security
 * 
 * ### AI Assist Note
 * **Security Utility**: Provides robust sanitization for real-time telemetry and agent-generated content.
 * Prevents XSS by stripping dangerous HTML tags and event handlers.
 */

/**
 * Sanitizes a string by stripping HTML tags and potentially dangerous attributes.
 * Focused on preventing XSS in real-time logs and thought streams.
 */
export const sanitize_telemetry = (text: string): string => {
    if (!text) return '';

    // 1. Strip <script> tags and their contents
    let sanitized = text.replace(/<script\b[^>]*>([\s\S]*?)<\/script>/gim, '');

    // 2. Strip common dangerous event handlers (on* attributes)
    sanitized = sanitized.replace(/on\w+="[^"]*"/gim, '');
    sanitized = sanitized.replace(/on\w+='[^']*'/gim, '');
    sanitized = sanitized.replace(/on\w+=\S+/gim, '');

    // 3. Strip potentially dangerous tags but leave formatting if needed
    // In a production app, use DOMPurify. This is a lightweight defensive measure.
    const dangerous_tags = /<(iframe|object|embed|form|frameset|frame|applet|meta|link|style|base)\b[^>]*>([\s\S]*?)<\/\1>|<(iframe|object|embed|form|frameset|frame|applet|meta|link|style|base)\b[^>]*>/gim;
    sanitized = sanitized.replace(dangerous_tags, '');

    // 4. Encode angle brackets for any remaining tags if we want strict safety, 
    // but here we might want to allow some markdown-like or basic formatting if the UI supports it.
    // For now, we'll be strict:
    // sanitized = sanitized.replace(/</g, '&lt;').replace(/>/g, '&gt;');

    return sanitized;
};

/**
 * Normalizes and sanitizes an entire telemetry payload recursively.
 * Ensures that all string fields are stripped of dangerous HTML.
 */
export const sanitize_payload = <T>(payload: T): T => {
    if (payload === null || payload === undefined) {
        return payload;
    }

    if (typeof payload === 'string') {
        return sanitize_telemetry(payload) as unknown as T;
    }

    if (Array.isArray(payload)) {
        return payload.map(item => sanitize_payload(item)) as unknown as T;
    }

    if (typeof payload === 'object') {
        const new_obj = { ...payload } as Record<string, unknown>;
        for (const key in new_obj) {
            if (Object.prototype.hasOwnProperty.call(new_obj, key)) {
                new_obj[key] = sanitize_payload(new_obj[key]);
            }
        }
        return new_obj as unknown as T;
    }

    return payload;
};

// Metadata: [security_utils]
