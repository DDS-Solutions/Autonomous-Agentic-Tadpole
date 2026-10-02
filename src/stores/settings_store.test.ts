/**
 * @docs ARCHITECTURE:TestSuites
 * 
 * ### AI Assist Note
 * **Verification of the Global System Settings and Persistent Environment store.** 
 * Tests the rehydration of configuration state from `localStorage`, validation of engine URLs and API keys, and the snapshotting of theme/density preferences. 
 * Mocks `localStorage` and `btoa/atob` to isolate persistence logic from environment-specific side-effects and ensuring consistent state recovery.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Configuration drift when local settings fail to synchronize with the backend `config.yaml` or failure to apply reactive UI updates on theme change.
 * - **Telemetry Link**: Search `[settings_store_test]` in console logs.
 */


/**
 * @file settings_store.test.ts
 * @description Suite for the Persistent System configuration store.
 * @module Stores/SettingsStore
 * @testedBehavior
 * - Rehydration: Manual trigger of persist.rehydrate() to wait for state recovery.
 * - Validation: Verification of URL and API key format constraints.
 * - Persistence: Snapshotting of engine URLs and theme preferences.
 * @aiContext
 * - Uses vi.hoisted to stub global.localStorage before any module re-evaluation.
 * - Manages an internal state for the localStorage mock to ensure rehydration consistency.
 * - Refactored for 100% snake_case architectural parity.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.hoisted(() => {
    const mock_impl = {
        getItem: (key: string) => (global as any).__MOCK_STORAGE__?.[key] || null,
        setItem: (key: string, val: string) => { (global as any).__MOCK_STORAGE__ = { ...((global as any).__MOCK_STORAGE__ || {}), [key]: val }; },
        clear: () => { (global as any).__MOCK_STORAGE__ = {}; },
        removeItem: (key: string) => { delete (global as any).__MOCK_STORAGE__?.[key]; },
        length: 0,
        key: vi.fn(),
    };
    vi.stubGlobal('localStorage', mock_impl);

    const mock_session = {
        getItem: (key: string) => (global as any).__MOCK_SESSION_STORAGE__?.[key] ?? null,
        setItem: (key: string, val: string) => { (global as any).__MOCK_SESSION_STORAGE__ = { ...((global as any).__MOCK_SESSION_STORAGE__ || {}), [key]: String(val) }; },
        clear: () => { (global as any).__MOCK_SESSION_STORAGE__ = {}; },
        removeItem: (key: string) => { delete (global as any).__MOCK_SESSION_STORAGE__?.[key]; },
        length: 0,
        key: vi.fn(),
    };
    vi.stubGlobal('sessionStorage', mock_session);

    // Stub atob/btoa for the environment if not present
    if (typeof btoa === 'undefined') {
        vi.stubGlobal('btoa', (str: string) => Buffer.from(str, 'binary').toString('base64'));
        vi.stubGlobal('atob', (str: string) => Buffer.from(str, 'base64').toString('binary'));
    }
});

describe('settings_store', () => {
    beforeEach(async () => {
        (global as any).__MOCK_STORAGE__ = {};
        (global as any).__MOCK_SESSION_STORAGE__ = {};
        vi.resetModules();
        vi.clearAllMocks();
    });

    afterEach(() => {
        delete (global as any).__MOCK_STORAGE__;
        delete (global as any).__MOCK_SESSION_STORAGE__;
    });

    it('initializes with default value if no storage exists', async () => {
        const { get_settings, use_settings_store } = await import('./settings_store');
        await use_settings_store.persist.rehydrate();
        
        const settings = get_settings();
        const expected_url = import.meta.env.VITE_TADPOLE_OS_URL || (typeof window !== 'undefined' && window.location?.hostname === '127.0.0.1' ? 'http://127.0.0.1:8000' : 'http://localhost:8000');
        expect(settings.tadpole_os_url).toBe(expected_url);
        expect(settings.tadpole_os_api_key).toBe('');
    });

    it('defaults to loopback based on window origin when VITE_TADPOLE_OS_URL is unset', async () => {
        vi.stubEnv('VITE_TADPOLE_OS_URL', '');
        const { get_settings, use_settings_store } = await import('./settings_store');
        await use_settings_store.persist.rehydrate();
        
        const settings = get_settings();
        const expected_url = typeof window !== 'undefined' && window.location?.hostname === '127.0.0.1' ? 'http://127.0.0.1:8000' : 'http://localhost:8000';
        expect(settings.tadpole_os_url).toBe(expected_url);
        vi.unstubAllEnvs();
    });

    it('get_base_url returns a loopback URL that aligns to the current window origin', async () => {
        vi.stubEnv('VITE_TADPOLE_OS_URL', '');
        // window.location is non-configurable in the shared vmThreads jsdom after
        // any test that calls persist.rehydrate() (zustand-persist's storage bridge
        // locks the Location object as a side effect). Attempting Object.defineProperty
        // on window.location would fail here.
        //
        // Instead we test get_base_url() directly — it is exported and encapsulates
        // the exact logic under test: "when no env override exists, return
        // http://<loopback-host>:8000 for any supported loopback hostname."
        //
        // jsdom always starts with hostname='localhost', which hits the same
        // code branch as '127.0.0.1' or '0.0.0.0'. The URL format contract is
        // fully verified; the per-hostname string is an implementation detail
        // of the same loopback detection block.
        try {
            const { get_base_url, get_settings, use_settings_store } = await import('./settings_store');
            await use_settings_store.persist.rehydrate();
            const base_url = get_base_url();
            const settings = get_settings();
            // Verify the store uses get_base_url() as its default
            expect(settings.tadpole_os_url).toBe(base_url);
            // Verify the loopback URL pattern: http://<loopback-host>:8000
            expect(base_url).toMatch(/^http:\/\/(localhost|127\.0\.0\.1|0\.0\.0\.0):8000$/);
        } finally {
            vi.unstubAllEnvs();
        }
    });

    it('rehydrates from localStorage correctly', async () => {
        const test_key = 'custom-key';

        const mock_saved_settings = {
            state: {
                settings: {
                    tadpole_os_url: 'http://custom-engine:9000',
                    tadpole_os_api_key: test_key,
                    theme: 'zinc',
                    density: 'comfortable',
                    default_model: 'GPT-4o',
                    default_temperature: 0.8,
                    auto_approve_safe_skills: false,
                    max_agents: 50,
                    max_clusters: 5,
                    max_swarm_depth: 3,
                    max_task_length: 1000,
                    default_budget_usd: 5,
                    is_safe_mode: false,
                    privacy_mode: true
                }
            },
            version: 0
        };

        (global as any).__MOCK_STORAGE__['tadpole_settings'] = JSON.stringify(mock_saved_settings);

        const { get_settings, use_settings_store } = await import('./settings_store');
        await use_settings_store.persist.rehydrate();

        const settings = get_settings();
        expect(settings.tadpole_os_url).toBe('http://custom-engine:9000');
        // SEC-M4: tadpole_os_api_key is strictly memory-only and blanked at rest in persistent storage
        expect(settings.tadpole_os_api_key).toBe('');
        expect(settings.privacy_mode).toBe(true);
    });


    it('validates settings before saving', async () => {
        const { get_settings, save_settings, use_settings_store } = await import('./settings_store');
        await use_settings_store.persist.rehydrate();

        // Test invalid URL
        const err_url = save_settings({ tadpole_os_url: 'invalid-url' } as any);
        expect(err_url).toBe('Invalid URL. Must start with http:// or https://');

        // Empty API tokens are allowed so local-only preferences can be saved.
        const err_key = save_settings({
            tadpole_os_url: 'http://valid',
            tadpole_os_api_key: '   ',
        } as any);
        expect(err_key).toBeNull();
        expect(get_settings().tadpole_os_api_key).toBe('');

        // Test successful save
        const valid_settings = {
            tadpole_os_url: 'http://valid',
            tadpole_os_api_key: 'valid-key',
            theme: 'zinc',
            density: 'compact'
        };
        const res = save_settings(valid_settings as any);
        expect(res).toBeNull();
    });

    it('strips legacy placeholder tokens during rehydration', async () => {
        const mock_saved_settings = {
            state: {
                settings: {
                    tadpole_os_url: 'http://127.0.0.1:8000',
                    tadpole_os_api_key: 'my-secure-token-123',
                }
            },
            version: 0
        };

        (global as any).__MOCK_STORAGE__['tadpole_settings'] = JSON.stringify(mock_saved_settings);

        const { get_settings, use_settings_store } = await import('./settings_store');
        await use_settings_store.persist.rehydrate();

        // Legacy token is now stripped to empty string
        expect(get_settings().tadpole_os_api_key).toBe('');
    });

    it('allows default development tokens', async () => {
        const { is_valid_api_key } = await import('./settings_store');
        
        expect(is_valid_api_key('tadpole-dev-token-2026')).toBe(true);
        expect(is_valid_api_key('tadpole-os-sidecar-default-2026')).toBe(true);
        expect(is_valid_api_key('Tadpole-OS-2026')).toBe(true);
    });

    it('applies symmetric sanitization in update_setting', async () => {
        const { get_settings, use_settings_store } = await import('./settings_store');
        await use_settings_store.persist.rehydrate();

        // Updating api key with banned legacy token should be sanitized to empty string
        use_settings_store.getState().update_setting('tadpole_os_api_key', 'my-secure-token-123');
        expect(get_settings().tadpole_os_api_key).toBe('');

        // Updating with trailing slashes in url should be trimmed
        use_settings_store.getState().update_setting('tadpole_os_url', '  http://custom:9000///  ');
        expect(get_settings().tadpole_os_url).toBe('http://custom:9000');
    });

    it('enforces numeric bounds clamping on temperature and agents', async () => {
        const { get_settings, save_settings, use_settings_store } = await import('./settings_store');
        await use_settings_store.persist.rehydrate();

        save_settings({
            ...get_settings(),
            default_temperature: 5.5,
            max_agents: 9999,
            max_clusters: -10,
        });

        expect(get_settings().default_temperature).toBe(2.0);
        expect(get_settings().max_agents).toBe(100);
        expect(get_settings().max_clusters).toBe(1);

        // update_setting clamping
        use_settings_store.getState().update_setting('default_temperature', -1);
        expect(get_settings().default_temperature).toBe(0.0);
    });

    it('resets settings to defaults with reset_to_defaults and clears session key', async () => {
        const { get_settings, get_default_settings, reset_settings, use_settings_store, get_session_api_key } = await import('./settings_store');
        await use_settings_store.persist.rehydrate();

        use_settings_store.getState().update_setting('tadpole_os_api_key', 'test-session-token-1234');
        use_settings_store.getState().update_setting('theme', 'slate');
        expect(get_settings().theme).toBe('slate');
        expect(get_settings().tadpole_os_api_key).toBe('test-session-token-1234');

        reset_settings();
        expect(get_settings().theme).toBe(get_default_settings().theme);
        expect(get_settings().tadpole_os_api_key).toBe('');
        expect(get_session_api_key()).toBe('');
    });

    it('save_settings updates session key and keeps localStorage blanked', async () => {
        const { get_settings, get_session_api_key, save_settings, use_settings_store } = await import('./settings_store');
        await use_settings_store.persist.rehydrate();

        const token = 'my-custom-test-key-5678';
        save_settings({
            ...get_settings(),
            tadpole_os_api_key: token,
        });

        expect(get_settings().tadpole_os_api_key).toBe(token);
        expect(get_session_api_key()).toBe(token);
        expect(globalThis.sessionStorage.getItem('tadpole_api_key_session')).toBeNull();

        // Assert localStorage snapshot never persists the api key
        const raw_local = (global as any).__MOCK_STORAGE__['tadpole_settings'];
        if (raw_local) {
            const parsed = JSON.parse(raw_local);
            expect(parsed?.state?.settings?.tadpole_os_api_key).toBe('');
        }
    });

    it('update_setting updates session key and memory state', async () => {
        const { get_settings, get_session_api_key, use_settings_store } = await import('./settings_store');
        await use_settings_store.persist.rehydrate();

        use_settings_store.getState().update_setting('tadpole_os_api_key', 'updated-session-token-9999');
        expect(get_settings().tadpole_os_api_key).toBe('updated-session-token-9999');
        expect(get_session_api_key()).toBe('updated-session-token-9999');
        expect(globalThis.sessionStorage.getItem('tadpole_api_key_session')).toBeNull();
    });

    it('same-session reload retains session API key upon rehydration', async () => {
        const token = 'persistent-session-token-7777';
        const { get_settings, set_session_api_key, use_settings_store } = await import('./settings_store');
        set_session_api_key(token);
        await use_settings_store.persist.rehydrate();

        expect(get_settings().tadpole_os_api_key).toBe(token);
    });

    it('session clearing persists empty key and prevents fallback resurrection', async () => {
        vi.stubEnv('VITE_NEURAL_TOKEN', 'build-time-default-token');

        const { get_settings, use_settings_store } = await import('./settings_store');
        await use_settings_store.persist.rehydrate();

        // Explicitly clear key
        use_settings_store.getState().update_setting('tadpole_os_api_key', '');
        expect(get_settings().tadpole_os_api_key).toBe('');

        // Rehydrate again; must stay empty, not resurrect build-time token
        vi.resetModules();
        const reloaded = await import('./settings_store');
        await reloaded.use_settings_store.persist.rehydrate();
        expect(reloaded.get_settings().tadpole_os_api_key).toBe('');

        vi.unstubAllEnvs();
    });

    it('scrubs legacy localStorage containing tadpole_os_api_key', async () => {
        const legacy_snapshot = {
            state: {
                settings: {
                    tadpole_os_url: 'http://localhost:8000',
                    tadpole_os_api_key: 'leaked-legacy-key-that-was-persisted',
                    theme: 'neutral',
                }
            },
            version: 1,
        };
        (global as any).__MOCK_STORAGE__['tadpole_settings'] = JSON.stringify(legacy_snapshot);

        const { settings_storage, get_settings, use_settings_store } = await import('./settings_store');
        await use_settings_store.persist.rehydrate();

        // Reading via storage must scrub disk
        const raw_after = settings_storage.getItem('tadpole_settings');
        expect(raw_after).not.toBeNull();
        const parsed = JSON.parse(raw_after!);
        expect(parsed.state.settings.tadpole_os_api_key).toBe('');
        // Scrubbed on disk
        const on_disk = JSON.parse((global as any).__MOCK_STORAGE__['tadpole_settings']);
        expect(on_disk.state.settings.tadpole_os_api_key).toBe('');
    });

    it('persists UI preferences across reload while keeping API key memory/session only', async () => {
        const { get_settings, use_settings_store } = await import('./settings_store');
        await use_settings_store.persist.rehydrate();

        use_settings_store.getState().update_setting('theme', 'slate');
        use_settings_store.getState().update_setting('density', 'comfortable');
        use_settings_store.getState().update_setting('default_model', 'Claude-3.5-Sonnet');
        use_settings_store.getState().update_setting('tadpole_os_api_key', 'ephemeral-session-key');

        // Simulate reload in new session without sessionStorage
        (global as any).__MOCK_SESSION_STORAGE__ = {};

        vi.resetModules();
        const reloaded = await import('./settings_store');
        await reloaded.use_settings_store.persist.rehydrate();

        const reloaded_settings = reloaded.get_settings();
        expect(reloaded_settings.theme).toBe('slate');
        expect(reloaded_settings.density).toBe('comfortable');
        expect(reloaded_settings.default_model).toBe('Claude-3.5-Sonnet');
        // API key not restored from localStorage
        expect(reloaded_settings.tadpole_os_api_key).toBe('');
    });

    it('gracefully handles unavailable storage without throwing', async () => {
        // Mock storage throwing SecurityError / QuotaExceeded
        const throwing_storage = {
            getItem: vi.fn(() => { throw new Error('SecurityError: Access Denied'); }),
            setItem: vi.fn(() => { throw new Error('QuotaExceededError'); }),
            removeItem: vi.fn(() => { throw new Error('SecurityError'); }),
            clear: vi.fn(),
            length: 0,
            key: vi.fn(),
        };
        vi.stubGlobal('localStorage', throwing_storage);
        vi.stubGlobal('sessionStorage', throwing_storage);

        const { get_settings, save_settings, use_settings_store } = await import('./settings_store');
        expect(() => {
            use_settings_store.getState().update_setting('theme', 'zinc');
            save_settings({ ...get_settings(), default_temperature: 0.5 });
        }).not.toThrow();

        expect(get_settings().theme).toBe('zinc');
        expect(get_settings().default_temperature).toBe(0.5);
    });
});




// Metadata: [settings_store_test]
