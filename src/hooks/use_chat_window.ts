/**
 * @docs ARCHITECTURE:Logic
 * @docs OPERATIONS_MANUAL:Navigation
 *
 * ### AI Assist Note
 * **UI State Hook**: Manages positioning, responsive viewport clamping, minimization, and
 * detachment transforms for the `440×600` Sovereign Chat window using `framer-motion`.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Viewport overflow on window resize or small viewports mitigated by `clamp_open_bounds`/`clamp_min_bounds`.
 * - **Telemetry Link**: Search `[useChatWindow]` in UI traces.
 */

import { useState, useRef, useEffect, useCallback } from 'react';
import { useMotionValue } from 'framer-motion';
import { use_sovereign_store } from '../stores/sovereign_store';

// Centralized layout dimensions (matching SovereignChat.tsx w-[440px] h-[600px] styling)
const CHAT_PANEL_WIDTH = 440;
const CHAT_PANEL_HEIGHT = 600;
const BUTTON_WIDTH = 220;
const BUTTON_HEIGHT = 48;
const PADDING = 48; // Combined margins/padding (24px each side)

const Y_TRANSLATION = -(CHAT_PANEL_HEIGHT - BUTTON_HEIGHT); // -552px
const X_TRANSLATION = -(CHAT_PANEL_WIDTH - BUTTON_WIDTH);   // -220px

export function useChatWindow() {
    const { is_detached, set_detached } = use_sovereign_store();
    const [is_minimized, set_is_minimized] = useState(false);
    const telemetry_source = '[useChatWindow]';

    const constraints_ref = useRef<HTMLDivElement>(null);

    const x_open = useMotionValue(0);
    const y_open = useMotionValue(0);
    const x_min = useMotionValue(0);
    const y_min = useMotionValue(0);

    const get_max_neg_bounds = useCallback((h: number, w: number) => {
        const effective_panel_h = Math.min(CHAT_PANEL_HEIGHT, Math.max(200, h - PADDING));
        const max_neg_y_open = -Math.max(0, h - effective_panel_h - PADDING);
        const max_neg_x_open = -Math.max(0, w - CHAT_PANEL_WIDTH - PADDING);
        const max_neg_y_min = -Math.max(0, h - BUTTON_HEIGHT - PADDING);
        const max_neg_x_min = -Math.max(0, w - BUTTON_WIDTH - PADDING);
        return { max_neg_y_open, max_neg_x_open, max_neg_y_min, max_neg_x_min };
    }, []);

    const clamp_open_bounds = useCallback(() => {
        const h = window.innerHeight;
        const w = window.innerWidth;
        const { max_neg_y_open, max_neg_x_open } = get_max_neg_bounds(h, w);

        if (y_open.get() < max_neg_y_open) y_open.set(max_neg_y_open);
        if (x_open.get() < max_neg_x_open) x_open.set(max_neg_x_open);
        if (y_open.get() > 0) y_open.set(0);
        if (x_open.get() > 0) x_open.set(0);
    }, [x_open, y_open, get_max_neg_bounds]);

    const clamp_min_bounds = useCallback(() => {
        const h = window.innerHeight;
        const w = window.innerWidth;
        const { max_neg_y_min, max_neg_x_min } = get_max_neg_bounds(h, w);

        if (y_min.get() < max_neg_y_min) y_min.set(max_neg_y_min);
        if (x_min.get() < max_neg_x_min) x_min.set(max_neg_x_min);
        if (y_min.get() > 0) y_min.set(0);
        if (x_min.get() > 0) x_min.set(0);
    }, [x_min, y_min, get_max_neg_bounds]);

    useEffect(() => {
        if (!is_minimized) {
            clamp_open_bounds();
        }
    }, [is_minimized, clamp_open_bounds]);

    useEffect(() => {
        let resize_timeout: ReturnType<typeof setTimeout>;

        const handle_resize = () => {
            if (is_minimized) {
                clamp_min_bounds();
            } else {
                clamp_open_bounds();
            }
        };

        const debounced_resize = () => {
            clearTimeout(resize_timeout);
            resize_timeout = setTimeout(handle_resize, 150);
        };

        window.addEventListener('resize', debounced_resize);
        return () => {
            clearTimeout(resize_timeout);
            window.removeEventListener('resize', debounced_resize);
        };
    }, [is_minimized, clamp_open_bounds, clamp_min_bounds]);

    const toggle_detach = useCallback(() => {
        console.debug(`${telemetry_source} Toggling detachment: ${!is_detached}`);
        set_detached(!is_detached);
        if (!is_detached) set_is_minimized(false);
    }, [is_detached, set_detached]);

    const perform_minimize_transform = useCallback(() => {
        console.debug(`${telemetry_source} Initiating minimize transform`);
        const h = window.innerHeight;
        const w = window.innerWidth;
        const { max_neg_y_open, max_neg_x_open } = get_max_neg_bounds(h, w);

        const safe_neg_y_open = Math.min(-1, max_neg_y_open);
        const safe_neg_x_open = Math.min(-1, max_neg_x_open);

        const ratio_y = Math.min(1, Math.max(0, y_open.get() / safe_neg_y_open));
        const ratio_x = Math.min(1, Math.max(0, x_open.get() / safe_neg_x_open));

        const y_shift = Y_TRANSLATION * ratio_y;
        const x_shift = X_TRANSLATION * ratio_x;

        x_min.set(x_open.get() + x_shift);
        y_min.set(y_open.get() + y_shift);
        clamp_min_bounds();
        set_is_minimized(true);
    }, [x_open, y_open, x_min, y_min, get_max_neg_bounds, clamp_min_bounds]);

    const perform_maximize_transform = useCallback(() => {
        console.debug(`${telemetry_source} Initiating maximize transform`);
        const h = window.innerHeight;
        const w = window.innerWidth;
        const { max_neg_y_min, max_neg_x_min } = get_max_neg_bounds(h, w);

        const safe_neg_y_min = Math.min(-1, max_neg_y_min);
        const safe_neg_x_min = Math.min(-1, max_neg_x_min);

        const ratio_y = Math.min(1, Math.max(0, y_min.get() / safe_neg_y_min));
        const ratio_x = Math.min(1, Math.max(0, x_min.get() / safe_neg_x_min));

        const y_shift = -Y_TRANSLATION * ratio_y;
        const x_shift = -X_TRANSLATION * ratio_x;

        y_open.set(y_min.get() + y_shift);
        x_open.set(x_min.get() + x_shift);
        clamp_open_bounds();

        set_is_minimized(false);
    }, [x_open, y_open, x_min, y_min, get_max_neg_bounds, clamp_open_bounds]);

    return {
        is_detached,
        is_minimized,
        constraints_ref,
        x_open,
        y_open,
        x_min,
        y_min,
        clamp_open_bounds,
        clamp_min_bounds,
        set_detached,
        toggle_detach,
        perform_minimize_transform,
        perform_maximize_transform
    };
}

// Metadata: [use_chat_window]
