/**
 * @docs ARCHITECTURE:Interface
 *
 * ### AI Assist Note
 * **Hook**: Calculates the optimal viewport-constrained position for contextual UI overlays
 * (tooltips, dropdowns). Handles smart flipping when near viewport edges and sub-pixel
 * clamping via a two-pass layout effect.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Tooltip renders off-screen when `content_ref` dimensions are 0
 *   (element not yet painted). Ensure `is_visible` is only `true` after mount.
 * - **Telemetry Link**: Search `[use_viewport_position]` in UI interaction traces.
 */

import { useState, useCallback, useLayoutEffect, useRef } from 'react';
import type { RefObject } from 'react';

export type Position = 'top' | 'bottom' | 'left' | 'right';

interface UseViewportPositionProps {
    trigger_ref: RefObject<HTMLElement | null>;
    content_ref: RefObject<HTMLElement | null>;
    position?: Position;
    padding?: number;
    offset?: number;
    is_visible?: boolean;
}

/**
 * use_viewport_position
 * Standardized hook for calculating the position of a contextual UI element (tooltip, dropdown)
 * relative to a trigger element, with smart flipping and viewport boundary constraints.
 */
export const useViewportPosition = ({
    trigger_ref,
    content_ref,
    position = 'top',
    padding = 8,
    offset = 8,
    is_visible = false,
}: UseViewportPositionProps) => {
    const [coords, set_coords] = useState({ x: 0, y: 0 });
    const [actual_position, set_actual_position] = useState<Position>(position);

    // Track coordinates and position in refs to eliminate effect re-trigger cycles
    const coords_ref = useRef({ x: 0, y: 0 });
    const actual_pos_ref = useRef<Position>(position);

    const calculate_and_apply = useCallback(() => {
        if (!trigger_ref.current) return;
        const trigger_rect = trigger_ref.current.getBoundingClientRect();
        const content_rect = content_ref.current?.getBoundingClientRect();

        const content_width = content_rect ? content_rect.width : 0;
        const content_height = content_rect ? content_rect.height : 0;

        const view_width = typeof window !== 'undefined' ? window.innerWidth : 1920;
        const view_height = typeof window !== 'undefined' ? window.innerHeight : 1080;

        let new_pos = position;

        // Base flipping logic
        if (position === 'top' && (trigger_rect.top - offset - content_height < padding)) {
            if (view_height - trigger_rect.bottom > trigger_rect.top) new_pos = 'bottom';
        } else if (position === 'bottom' && (trigger_rect.bottom + offset + content_height > view_height - padding)) {
            if (trigger_rect.top > view_height - trigger_rect.bottom) new_pos = 'top';
        } else if (position === 'left' && (trigger_rect.left - offset - content_width < padding)) {
            if (view_width - trigger_rect.right > trigger_rect.left) new_pos = 'right';
        } else if (position === 'right' && (trigger_rect.right + offset + content_width > view_width - padding)) {
            if (trigger_rect.left > view_width - trigger_rect.right) new_pos = 'left';
        }

        let x = 0;
        let y = 0;

        switch (new_pos) {
            case 'top':
                x = trigger_rect.left + trigger_rect.width / 2;
                y = trigger_rect.top - offset;
                break;
            case 'bottom':
                x = trigger_rect.left + trigger_rect.width / 2;
                y = trigger_rect.bottom + offset;
                break;
            case 'left':
                x = trigger_rect.left - offset;
                y = trigger_rect.top + trigger_rect.height / 2;
                break;
            case 'right':
                x = trigger_rect.right + offset;
                y = trigger_rect.top + trigger_rect.height / 2;
                break;
        }

        // Viewport boundary constraints (clamping)
        if (content_width > 0 && content_height > 0) {
            if (new_pos === 'top' || new_pos === 'bottom') {
                const half_w = content_width / 2;
                if (x - half_w < padding) {
                    x = padding + half_w;
                } else if (x + half_w > view_width - padding) {
                    x = view_width - padding - half_w;
                }

                if (new_pos === 'top') {
                    if (y - content_height < padding) {
                        y = padding + content_height;
                    }
                } else {
                    if (y + content_height > view_height - padding) {
                        y = view_height - padding - content_height;
                    }
                }
            } else {
                if (new_pos === 'left') {
                    if (x - content_width < padding) {
                        x = padding + content_width;
                    }
                } else {
                    if (x + content_width > view_width - padding) {
                        x = view_width - padding - content_width;
                    }
                }

                const half_h = content_height / 2;
                if (y - half_h < padding) {
                    y = padding + half_h;
                } else if (y + half_h > view_height - padding) {
                    y = view_height - padding - half_h;
                }
            }
        }

        const rounded_x = Math.round(x);
        const rounded_y = Math.round(y);

        const dx = Math.abs(rounded_x - coords_ref.current.x);
        const dy = Math.abs(rounded_y - coords_ref.current.y);
        const pos_changed = new_pos !== actual_pos_ref.current;

        // Invariant: ONLY update state if change exceeds 1px or orientation changed
        if (dx >= 1 || dy >= 1 || pos_changed) {
            coords_ref.current = { x: rounded_x, y: rounded_y };
            actual_pos_ref.current = new_pos;
            set_coords({ x: rounded_x, y: rounded_y });
            if (pos_changed) {
                set_actual_position(new_pos);
            }
        }
    }, [position, offset, padding, trigger_ref, content_ref]);

    // Synchronous layout calculation without feedback loop
    useLayoutEffect(() => {
        if (is_visible && trigger_ref.current) {
            calculate_and_apply();
        }
    }, [is_visible, calculate_and_apply]);

    return { coords, actual_position, update_position: calculate_and_apply };
};

/**
 * snake_case alias for backward compatibility.
 * New code should import `useViewportPosition` directly.
 */
export const use_viewport_position = useViewportPosition;



// Metadata: [use_viewport_position]
