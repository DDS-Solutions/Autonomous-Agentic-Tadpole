/**
 * @docs ARCHITECTURE:Interface
 *
 * ### AI Assist Note
 * **INV-005: OpenUI Recursion Bounded Depth Invariant**
 * Asserts that recursive OpenUI layout rendering enforces MAX_RENDER_DEPTH = 10
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Call-stack exhaustion or UI crash on deep layout ASTs.
 * - **Telemetry Link**: Search `[inv_005_openui_depth]` in test logs.
 *
 * // Metadata: [inv_005_openui_depth]
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import React from 'react';
import { OpenUI_Renderer, MAX_RENDER_DEPTH } from '../../src/components/chat/OpenUI_Renderer';
import type { OpenUI_DSL } from '../../src/types';

describe('INV-005: OpenUI Recursion Depth Boundary Invariant', () => {
    it('defines MAX_RENDER_DEPTH constant exactly equal to 10', () => {
        expect(MAX_RENDER_DEPTH).toBe(10);
    });

    it('successfully renders nested layouts within safe boundary (depth <= 10)', () => {
        let safeLayout: OpenUI_DSL = {
            kind: 'kpi_card',
            title: 'Inner Metric',
            value: 42,
        };
        for (let i = 0; i < 5; i++) {
            safeLayout = {
                kind: 'layout',
                direction: 'column',
                children: [safeLayout],
            };
        }

        render(React.createElement(OpenUI_Renderer, { dsl: safeLayout }));
        expect(screen.getByText('Inner Metric')).toBeTruthy();
        expect(screen.queryByText('[OpenUI: Max Render Depth Exceeded]')).toBeNull();
    });

    it('halts recursion and displays warning pill when nesting exceeds depth 10', () => {
        let deepLayout: OpenUI_DSL = {
            kind: 'kpi_card',
            title: 'Exhausted Leaf',
            value: 999,
        };
        for (let i = 0; i < 15; i++) {
            deepLayout = {
                kind: 'layout',
                direction: 'column',
                children: [deepLayout],
            };
        }

        render(React.createElement(OpenUI_Renderer, { dsl: deepLayout }));
        expect(screen.getByText('[OpenUI: Max Render Depth Exceeded]')).toBeTruthy();
        expect(screen.queryByText('Exhausted Leaf')).toBeNull();
    });
});
