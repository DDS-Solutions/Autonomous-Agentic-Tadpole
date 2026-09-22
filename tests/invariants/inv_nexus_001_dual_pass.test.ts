/**
 * @docs ARCHITECTURE:Core
 *
 * ### AI Assist Note
 * **INV-NEXUS-001: Mandatory Dual-Pass Protocol & Invariant Gates**
 * Statically enforces the Dual-Pass Nexus Invariants:
 * 1. Gateway Route Security: Protected `/v1` routes in `server-rs/src/router.rs` must be wrapped in `validate_token`.
 * 2. Relational Invariants: Database connection initializers in `server-rs/src/db/init.rs` must enforce `foreign_keys = ON`.
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Missing route authentication, disabled foreign keys, or CI regression.
 * - **Telemetry Link**: Search `[inv_nexus_001_dual_pass]` in test logs.
 *
 * // Metadata: [inv_nexus_001_dual_pass]
 */

import { describe, it, expect } from 'vitest';
import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

describe('INV-NEXUS-001: Mandatory Dual-Pass Protocol & Invariant Gates', () => {
    const routerPath = resolve('server-rs/src/router.rs');
    const dbInitPath = resolve('server-rs/src/db/init.rs');
    const ciWorkflowPath = resolve('.github/workflows/ci.yml');

    it('enforces Bearer token auth middleware on protected /v1 routes in router.rs', () => {
        expect(existsSync(routerPath)).toBe(true);
        const routerContent = readFileSync(routerPath, 'utf-8');

        // Verify build_protected_v1_routes exists
        expect(routerContent).toMatch(/fn\s+build_protected_v1_routes\s*\(/);

        // Verify validate_token is applied as route layer
        expect(routerContent).toMatch(/middleware::auth::validate_token/);

        // Verify core sub-routers are nested within protected routes
        expect(routerContent).toMatch(/\.nest\(\s*["']\/agents["']/);
        expect(routerContent).toMatch(/\.nest\(\s*["']\/oversight["']/);
        expect(routerContent).toMatch(/\.nest\(\s*["']\/infra["']/);
        expect(routerContent).toMatch(/\.nest\(\s*["']\/model-manager["']/);
        expect(routerContent).toMatch(/\.nest\(\s*["']\/skills["']/);
        expect(routerContent).toMatch(/\.nest\(\s*["']\/governance["']/);
        expect(routerContent).toMatch(/\.nest\(\s*["']\/intelligence["']/);
    });

    it('enforces SQLite foreign_keys = ON pragma in server-rs/src/db/init.rs', () => {
        expect(existsSync(dbInitPath)).toBe(true);
        const dbContent = readFileSync(dbInitPath, 'utf-8');

        // Check for pragma("foreign_keys", "ON")
        expect(dbContent).toMatch(/\.pragma\(\s*["']foreign_keys["']\s*,\s*["']ON["']\s*\)/);
    });

    it('enforces parity_guard execution in GitHub Actions CI workflow', () => {
        expect(existsSync(ciWorkflowPath)).toBe(true);
        const ciContent = readFileSync(ciWorkflowPath, 'utf-8');

        // Verify execution/parity_guard.py is executed in CI
        expect(ciContent).toMatch(/parity_guard\.py/);

        // Verify verify_ai_context.py is executed in CI
        expect(ciContent).toMatch(/verify_ai_context\.py/);
    });
});
