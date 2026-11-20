/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * Verifies every registered application route is reachable from the primary
 * navigation shell. This catches route/menu drift before a page becomes
 * effectively hidden.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI regression, hook desync, or API timeout.
 * - **Telemetry Link**: Search `[routes_navigation_test]` in console logs.
 */

import { describe, expect, it, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { APP_ROUTES } from './routes';
import { Sidebar } from '../components/layout/Sidebar';
import { use_settings_store } from '../stores/settings_store';

describe('Route navigation connectivity', () => {
    beforeEach(() => {
        use_settings_store.getState().update_setting('cockpit_mode', 'expert');
    });

    it('exposes every APP_ROUTES path in the sidebar navigation in Expert Mode', () => {
        render(
            <MemoryRouter>
                <Sidebar nav_item_class={() => 'nav-item'} />
            </MemoryRouter>
        );

        const sidebar_paths = new Set(
            screen.getAllByRole('link')
                .map(link => link.getAttribute('href'))
                .filter((href): href is string => Boolean(href))
        );

        const missing_paths = APP_ROUTES
            .map(route => route.path)
            .filter(path => !sidebar_paths.has(path));

        expect(missing_paths).toEqual([]);
    });

    it('exposes the 6 core routes in Focus Mode', () => {
        use_settings_store.getState().update_setting('cockpit_mode', 'focus');
        render(
            <MemoryRouter>
                <Sidebar nav_item_class={() => 'nav-item'} />
            </MemoryRouter>
        );

        const sidebar_paths = new Set(
            screen.getAllByRole('link')
                .map(link => link.getAttribute('href'))
                .filter((href): href is string => Boolean(href))
        );

        expect(sidebar_paths).toEqual(new Set([
            '/dashboard',
            '/missions',
            '/oversight',
            '/models',
            '/agents',
            '/settings',
        ]));
    });
});


// Metadata: [routes_navigation_test]
