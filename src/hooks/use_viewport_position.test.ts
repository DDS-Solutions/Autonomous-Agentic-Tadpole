/**
 * @docs ARCHITECTURE:UI-Hooks
 * 
 * ### AI Assist Note
 * Test suite for useViewportPosition hook.
 * Asserts viewport boundary clamping, auto-flipping, and zero infinite-loop regressions.
 * 
 * ### 🔍 Debugging & Observability
 * - Telemetry: Search `[use_viewport_position_test]`
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useViewportPosition } from './use_viewport_position';
import type { RefObject } from 'react';

describe('useViewportPosition', () => {
    let mock_trigger: HTMLElement;
    let mock_content: HTMLElement;
    let trigger_ref: RefObject<HTMLElement | null>;
    let content_ref: RefObject<HTMLElement | null>;

    beforeEach(() => {
        // Set fixed inner window dimensions
        Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1000 });
        Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

        mock_trigger = document.createElement('div');
        mock_content = document.createElement('div');

        // Default: trigger in center
        mock_trigger.getBoundingClientRect = () => ({
            left: 450,
            top: 380,
            right: 550,
            bottom: 420,
            width: 100,
            height: 40,
            x: 450,
            y: 380,
            toJSON: () => {}
        });

        // Content 200x50
        mock_content.getBoundingClientRect = () => ({
            left: 0,
            top: 0,
            right: 200,
            bottom: 50,
            width: 200,
            height: 50,
            x: 0,
            y: 0,
            toJSON: () => {}
        });

        trigger_ref = { current: mock_trigger };
        content_ref = { current: mock_content };
    });

    it('calculates top position correctly when centered', () => {
        const { result } = renderHook(() => useViewportPosition({
            trigger_ref,
            content_ref,
            position: 'top',
            is_visible: true,
            offset: 8,
            padding: 8
        }));

        expect(result.current.actual_position).toBe('top');
        // x = 450 + 100/2 = 500
        // y = 380 - 8 = 372
        expect(result.current.coords.x).toBe(500);
        expect(result.current.coords.y).toBe(372);
    });

    it('flips from top to bottom when near top edge', () => {
        mock_trigger.getBoundingClientRect = () => ({
            left: 450,
            top: 20, // Not enough room for 50px content + 8px offset + 8px padding
            right: 550,
            bottom: 60,
            width: 100,
            height: 40,
            x: 450,
            y: 20,
            toJSON: () => {}
        });

        const { result } = renderHook(() => useViewportPosition({
            trigger_ref,
            content_ref,
            position: 'top',
            is_visible: true,
            offset: 8,
            padding: 8
        }));

        expect(result.current.actual_position).toBe('bottom');
        // y = bottom + offset = 60 + 8 = 68
        expect(result.current.coords.y).toBe(68);
    });

    it('flips from left to right when near left edge', () => {
        mock_trigger.getBoundingClientRect = () => ({
            left: 50, // Not enough room for 200px content + 8px offset + 8px padding
            top: 380,
            right: 150,
            bottom: 420,
            width: 100,
            height: 40,
            x: 50,
            y: 380,
            toJSON: () => {}
        });

        const { result } = renderHook(() => useViewportPosition({
            trigger_ref,
            content_ref,
            position: 'left',
            is_visible: true,
            offset: 8,
            padding: 8
        }));

        expect(result.current.actual_position).toBe('right');
        // x = right + offset = 150 + 8 = 158
        expect(result.current.coords.x).toBe(158);
    });

    it('clamps horizontal coordinates to prevent overflowing left screen boundary', () => {
        // Trigger on far left edge
        mock_trigger.getBoundingClientRect = () => ({
            left: 10,
            top: 380,
            right: 40,
            bottom: 420,
            width: 30,
            height: 40,
            x: 10,
            y: 380,
            toJSON: () => {}
        });

        const { result } = renderHook(() => useViewportPosition({
            trigger_ref,
            content_ref,
            position: 'bottom',
            is_visible: true,
            offset: 8,
            padding: 8
        }));

        // x would be 10 + 15 = 25
        // left edge = 25 - 200/2 = -75 < 8
        // clamped x = 8 + 100 = 108
        expect(result.current.coords.x).toBe(108);
    });

    it('does NOT trigger infinite re-renders or exceed maximum update depth', () => {
        let render_count = 0;

        expect(() => {
            renderHook(() => {
                render_count++;
                return useViewportPosition({
                    trigger_ref,
                    content_ref,
                    position: 'top',
                    is_visible: true,
                    offset: 8,
                    padding: 8
                });
            });
        }).not.toThrow();

        // Under 5 renders total (mount + initial layout pass), never exceeding maximum update depth
        expect(render_count).toBeLessThan(10);
    });
});
