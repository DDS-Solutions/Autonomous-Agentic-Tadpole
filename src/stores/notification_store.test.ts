/**
 * @docs ARCHITECTURE:TestSuites
 * 
 * ### AI Assist Note
 * **Tests the global notification (toast) state management.** 
 * Verifies addition, manual removal, and auto-dismissal timers for persistent and non-persistent notifications.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Memory leaks from uncleared timers or race conditions when multiple notifications are added/removed rapidly.
 * - **Telemetry Link**: Search `[notification_store_test]` in console logs.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { use_notification_store } from './notification_store';

describe('notification_store', () => {
    beforeEach(() => {
        vi.useFakeTimers();
        use_notification_store.getState().clear_all();
    });

    it('adds a notification', () => {
        const store = use_notification_store.getState();
        store.add_notification({
            severity: 'info',
            title: 'Test Title',
            message: 'Test Message',
            persistent: true
        });

        const state = use_notification_store.getState();
        expect(state.notifications).toHaveLength(1);
        expect(state.notifications[0]).toMatchObject({
            severity: 'info',
            title: 'Test Title',
            message: 'Test Message',
            persistent: true
        });
        expect(state.notifications[0].id).toBeDefined();
    });

    it('removes a notification manually', () => {
        const store = use_notification_store.getState();
        store.add_notification({
            severity: 'success',
            title: 'Title',
            message: 'Message',
            persistent: true
        });

        const id = use_notification_store.getState().notifications[0].id;
        use_notification_store.getState().remove_notification(id);

        expect(use_notification_store.getState().notifications).toHaveLength(0);
    });

    it('automatically dismisses non-persistent notifications after 6 seconds', () => {
        const store = use_notification_store.getState();
        store.add_notification({
            severity: 'warning',
            title: 'Toast',
            message: 'I disappear',
            persistent: false
        });

        expect(use_notification_store.getState().notifications).toHaveLength(1);

        // Advance time by 5.9 seconds - should still be there
        vi.advanceTimersByTime(5900);
        expect(use_notification_store.getState().notifications).toHaveLength(1);

        // Advance time by another 0.1 seconds - should be gone
        vi.advanceTimersByTime(100);
        expect(use_notification_store.getState().notifications).toHaveLength(0);
    });

    it('clears all notifications and timers', () => {
        const store = use_notification_store.getState();
        store.add_notification({ severity: 'error', title: '1', message: 'm', persistent: false });
        store.add_notification({ severity: 'info', title: '2', message: 'm', persistent: true });

        expect(use_notification_store.getState().notifications).toHaveLength(2);

        use_notification_store.getState().clear_all();
        expect(use_notification_store.getState().notifications).toHaveLength(0);

        // Ensure timers don't cause issues after clear
        vi.runAllTimers();
        expect(use_notification_store.getState().notifications).toHaveLength(0);
    });

    it('clears timers when removed manually', () => {
        const store = use_notification_store.getState();
        store.add_notification({
            severity: 'info',
            title: 'Manual Remove',
            message: 'test',
            persistent: false
        });

        const id = use_notification_store.getState().notifications[0].id;
        
        // This should clear the timer internal to the store
        use_notification_store.getState().remove_notification(id);
        
        // Mock a new notification with same ID (highly unlikely but for test logic)
        // or just verify no errors occur when timers fire
        vi.runAllTimers();
        expect(use_notification_store.getState().notifications).toHaveLength(0);
    });

    it('automatically dismisses System Alert notifications after 10 seconds', () => {
        const store = use_notification_store.getState();
        store.add_notification({
            severity: 'warning',
            title: 'System Alert',
            message: 'Governance Pulse: Budget utilization at 85.0%',
            persistent: false
        });

        expect(use_notification_store.getState().notifications).toHaveLength(1);

        // Advance by 9.9 seconds - should still be visible
        vi.advanceTimersByTime(9900);
        expect(use_notification_store.getState().notifications).toHaveLength(1);

        // Advance by remaining 100ms - should now be dismissed
        vi.advanceTimersByTime(100);
        expect(use_notification_store.getState().notifications).toHaveLength(0);
    });

    it('deduplicates identical notifications added in rapid succession within 3 seconds', () => {
        const store = use_notification_store.getState();
        store.add_notification({
            severity: 'info',
            title: 'System Alert',
            message: 'Budget utilization at 60.5%',
            persistent: false
        });

        // Attempting to add an identical notification immediately
        store.add_notification({
            severity: 'info',
            title: 'System Alert',
            message: 'Budget utilization at 60.5%',
            persistent: false
        });

        // Another one immediately
        store.add_notification({
            severity: 'info',
            title: 'System Alert',
            message: 'Budget utilization at 60.5%',
            persistent: false
        });

        expect(use_notification_store.getState().notifications).toHaveLength(1);
    });

    it('honors custom duration when specified', () => {
        const store = use_notification_store.getState();
        store.add_notification({
            severity: 'info',
            title: 'Custom Timer',
            message: 'Dismiss me in 10s',
            persistent: false,
            duration: 10000
        });

        expect(use_notification_store.getState().notifications).toHaveLength(1);
        vi.advanceTimersByTime(9900);
        expect(use_notification_store.getState().notifications).toHaveLength(1);
        vi.advanceTimersByTime(200);
        expect(use_notification_store.getState().notifications).toHaveLength(0);
    });
});



// Metadata: [notification_store_test]
