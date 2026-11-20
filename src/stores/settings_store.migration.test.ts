/**
 * @docs ARCHITECTURE:TestSuites
 *
 * ### AI Assist Note
 * **Settings Store v2 Migration Tests**: Verifies pre-v2 persisted users keep `expert` mode
 * and `onboarding_completed: true`, while fresh installs default to `focus` mode and
 * `onboarding_completed: false`.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Existing operators surprised by hidden tabs or fresh users skipping onboarding.
 * - **Telemetry Link**: Search `[settings_store_migration_test]` in test logs.
 */

import { describe, it, expect } from 'vitest';
import { use_settings_store } from './settings_store';

describe('settings_store v2 cockpit_mode migration', () => {
    it('migrates pre-v2 persisted profiles to expert mode + onboarding_completed=true', () => {
        const opts = use_settings_store.persist.getOptions();
        expect(opts.migrate).toBeDefined();

        const preV2State = {
            settings: {
                tadpole_os_url: 'http://localhost:8000',
                theme: 'zinc',
            },
        };
        const migrated = opts.migrate!(preV2State, 1) as { settings: Record<string, unknown> };
        expect(migrated.settings.cockpit_mode).toBe('expert');
        expect(migrated.settings.onboarding_completed).toBe(true);
        expect(migrated.settings.active_tier_preset).toBe('hybrid_overlord');
    });

    it('preserves explicit v2 user settings without overwriting them', () => {
        const opts = use_settings_store.persist.getOptions();
        const v2State = {
            settings: {
                cockpit_mode: 'focus',
                onboarding_completed: false,
                active_tier_preset: 'local_sovereign',
            },
        };
        const migrated = opts.migrate!(v2State, 2) as { settings: Record<string, unknown> };
        expect(migrated.settings.cockpit_mode).toBe('focus');
        expect(migrated.settings.onboarding_completed).toBe(false);
        expect(migrated.settings.active_tier_preset).toBe('local_sovereign');
    });
});

// Metadata: [settings_store_migration_test]
