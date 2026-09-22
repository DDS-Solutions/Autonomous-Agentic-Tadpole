/**
 * @docs ARCHITECTURE:State
 * 
 * ### AI Assist Note
 * **Zustand State**: Swarm-wide alert and notification bus. 
 * Orchestrates the queueing, display, and auto-dismissal of system alerts, mission milestones, and security warnings.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Notification queue starvation (alerts not showing), or persistent "zombie" toasts after manual dismissal.
 * - **Telemetry Link**: Search for `[notification_store]` or `ALERT_PULSE` in UI logs.
 */

import { create } from 'zustand';

export type Notification_Severity = 'info' | 'success' | 'warning' | 'error';

export interface Notification {
    id: string;
    severity: Notification_Severity;
    title: string;
    message: string;
    type_id?: string;
    persistent: boolean;
    duration?: number;
    timestamp: Date;
}

interface Notification_State {
    notifications: Notification[];
    add_notification: (notification: Omit<Notification, 'id' | 'timestamp'>) => void;
    remove_notification: (id: string) => void;
    clear_all: () => void;
}

/** 
 * generate_id
 * Utility to generate unique notification IDs.
 */
const generate_id = () => (typeof crypto !== 'undefined' && crypto.randomUUID) 
    ? crypto.randomUUID() 
    : Math.random().toString(36).substring(2, 9);

// Module-level registry for active auto-dismiss timers
const active_timers = new Map<string, ReturnType<typeof setTimeout>>();

/**
 * use_notification_store
 * Manages global application notifications and toast alerts.
 * Features automatic dismissal for non-persistent alerts and severity-based grouping.
 */
export const use_notification_store = create<Notification_State>((set) => ({
    notifications: [],

    add_notification: (notification) => {
        // Deduplication: prevent stacking identical alerts within 3 seconds
        const current_state = use_notification_store.getState();
        const is_duplicate = current_state.notifications.some(
            n => n.title === notification.title && 
                 n.message === notification.message && 
                 (Date.now() - new Date(n.timestamp).getTime() < 3000)
        );
        if (is_duplicate) {
            return;
        }

        const id = generate_id();
        const is_system_alert = notification.title.includes('System Alert') || notification.title.includes('Agent Alert');
        // System alerts auto-dismiss after 10s; standard toasts after 6s; explicit duration takes precedence
        const default_duration = is_system_alert ? 10000 : 6000;
        const duration = notification.duration ?? (notification.persistent && !is_system_alert ? undefined : default_duration);

        const new_notification: Notification = {
            ...notification,
            id,
            duration,
            timestamp: new Date(),
        };

        set((state) => ({
            notifications: [new_notification, ...state.notifications],
        }));

        // Auto-dismiss logic for timed notifications
        if (duration && duration > 0) {
            const timer = setTimeout(() => {
                set((state) => ({
                    notifications: state.notifications.filter((n) => n.id !== id),
                }));
                active_timers.delete(id);
            }, duration);
            active_timers.set(id, timer);
        }
    },

    remove_notification: (id) => {
        const timer = active_timers.get(id);
        if (timer) {
            clearTimeout(timer);
            active_timers.delete(id);
        }
        set((state) => ({
            notifications: state.notifications.filter((n) => n.id !== id),
        }));
    },

    clear_all: () => {
        active_timers.forEach(clearTimeout);
        active_timers.clear();
        set({ notifications: [] });
    },
}));




// Metadata: [notification_store]
