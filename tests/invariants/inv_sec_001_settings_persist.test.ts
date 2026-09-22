/**
 * @docs ARCHITECTURE:Security
 *
 * ### AI Assist Note
 * **INV-SEC-001: Settings Store Storage Partialize & Token Isolation Invariant**
 * Enforces that `settings_store` uses Zustand's `partialize` middleware to ensure
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: API token leakage to localStorage or unencrypted state dump.
 * - **Telemetry Link**: Search `[inv_sec_001_settings_persist]` in test logs.
 *
 * // Metadata: [inv_sec_001_settings_persist]
 */

import { describe, it, expect } from 'vitest';
import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { use_settings_store, type Tadpole_Settings } from '../../src/stores/settings_store';

describe('INV-SEC-001: Settings Store Storage Partialize Invariant', () => {
    const settingsStorePath = resolve('src/stores/settings_store.ts');

    it('statically enforces partialize configuration in settings_store.ts', () => {
        expect(existsSync(settingsStorePath)).toBe(true);
        const content = readFileSync(settingsStorePath, 'utf-8');

        // Verify partialize is declared in persist options
        expect(content).toMatch(/partialize:\s*\(/);

        // Verify tadpole_os_api_key is explicitly cleared in partialize
        expect(content).toMatch(/tadpole_os_api_key:\s*['"]['"]/);
    });

    it('dynamically blanks tadpole_os_api_key in persisted state snapshot', () => {
        const persistOptions = use_settings_store.persist.getOptions();
        expect(persistOptions.partialize).toBeDefined();

        const dummySettings: Tadpole_Settings = {
            tadpole_os_url: 'http://localhost:8000',
            tadpole_os_api_key: 'sk-super-secret-production-token-never-persist',
            theme: 'zinc',
            density: 'compact',
            backdrop_theme: 'cyan',
            default_model: 'llama3',
            default_temperature: 0.7,
            auto_approve_safe_skills: true,
            max_agents: 10,
            max_clusters: 2,
            max_swarm_depth: 3,
            max_task_length: 500,
            default_budget_usd: 5.0,
            is_safe_mode: true,
            privacy_mode: true,
            browser_specialist_model_id: 'browser-use',
            computer_architect_url: 'http://localhost:9000',
            enable_neural_handoff: true,
            sentinel_mode: true,
        };

        const stateToPersist = {
            settings: dummySettings,
            save_settings: () => null,
            update_setting: () => {},
            reset_to_defaults: () => {},
        };

        const partialized = persistOptions.partialize!(stateToPersist as any) as any;

        // Verify persistent payload has settings object
        expect(partialized.settings).toBeDefined();
        // tadpole_os_api_key must be an empty string at rest
        expect(partialized.settings.tadpole_os_api_key).toBe('');
        // Non-secret settings should be preserved
        expect(partialized.settings.tadpole_os_url).toBe('http://localhost:8000');
        expect(partialized.settings.theme).toBe('zinc');
    });
});
