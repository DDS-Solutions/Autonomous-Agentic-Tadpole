import { describe, it, expect } from 'vitest';
import { sanitize_ui_context, extract_neural_output } from './ai_utils';

describe('ai_utils', () => {
    describe('sanitize_ui_context', () => {
        it('should strip script tags', () => {
            const html = '<div>Hello <script>alert("hack")</script> world</div>';
            expect(sanitize_ui_context(html)).toContain('[SCRIPT_REDACTED]');
            expect(sanitize_ui_context(html)).not.toContain('alert("hack")');
        });

        it('should redact input values', () => {
            const html = '<input value="secret123" /><input type="text" value="public" />';
            const sanitized = sanitize_ui_context(html);
            expect(sanitized).toContain('value="[VALUE_REDACTED]"');
            expect(sanitized).not.toContain('secret123');
        });

        it('should remove event handlers', () => {
            const html = '<button onclick="doEvil()">Click me</button>';
            expect(sanitize_ui_context(html)).toBe('<button>Click me</button>');
        });

        it('should truncate long strings', () => {
            const longStr = 'a'.repeat(20000);
            expect(sanitize_ui_context(longStr).length).toBeLessThan(17000);
            expect(sanitize_ui_context(longStr)).toContain('[TRUNCATED]');
        });
    });

    describe('extract_neural_output', () => {
        it('should extract content after ASSISTANT:', () => {
            const raw = 'SYSTEM: Prompt\nUSER: Query\nASSISTANT: This is the answer.';
            expect(extract_neural_output(raw)).toBe('This is the answer.');
        });

        it('should handle variations of Assistant tag', () => {
            const raw = 'Assistant: Mixed case works.';
            expect(extract_neural_output(raw)).toBe('Mixed case works.');
        });

        it('should extract content after thought tags', () => {
            const raw = '<thought>Internal monologue</thought> Final answer.';
            expect(extract_neural_output(raw)).toBe('Final answer.');
        });

        it('should fall back to raw text if no tags and no system prompt', () => {
            const raw = 'Simple response.';
            expect(extract_neural_output(raw)).toBe('Simple response.');
        });

        it('should return fallback if system prompt is present but no assistant tag', () => {
            const raw = 'SYSTEM: Rules\nUSER: Query';
            expect(extract_neural_output(raw, 'Default')).toBe('Default');
        });
    });
});
