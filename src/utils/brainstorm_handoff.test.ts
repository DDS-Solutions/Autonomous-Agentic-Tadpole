/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Brainstorm Handoff Utility Tests**: Verifies happy, failure, and boundary paths for
 * `build_brainstorm_context`.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Empty context leaks or unbounded string overflow.
 * - **Trace Scope**: `src/utils/brainstorm_handoff.test`
 */

import { describe, it, expect } from 'vitest';
import { build_brainstorm_context } from './brainstorm_handoff';

describe('brainstorm_handoff', () => {
    it('formats recent user and agent turns while skipping system noise (Happy Path)', () => {
        const result = build_brainstorm_context([
            { speaker: 'System', text: 'Connected', is_system: true },
            { speaker: 'User', text: 'Let us redesign the voice pipeline' },
            { speaker: 'Architect', text: 'We should split TTS, recorder, and mic lease.' },
        ]);

        expect(result).toBeDefined();
        expect(result).toContain('User: Let us redesign the voice pipeline');
        expect(result).toContain('Architect: We should split TTS, recorder, and mic lease.');
        expect(result).not.toContain('Connected');
    });

    it('returns undefined when no conversational turns exist (Failure / Empty Path)', () => {
        expect(build_brainstorm_context([])).toBeUndefined();
        expect(
            build_brainstorm_context([
                { speaker: 'System', text: 'Engine ready', is_system: true },
                { speaker: 'User', text: '   ' },
            ])
        ).toBeUndefined();
    });

    it('bounds long transcripts to max_chars cleanly (Edge Case Path)', () => {
        const long_turns = Array.from({ length: 20 }, (_, i) => ({
            speaker: i % 2 === 0 ? 'User' : 'Alpha',
            text: `Turn ${i}: ${'x'.repeat(300)}`,
        }));
        const bounded = build_brainstorm_context(long_turns, 12, 800);
        expect(bounded).toBeDefined();
        expect((bounded ?? '').length).toBeLessThanOrEqual(800);
    });
});
