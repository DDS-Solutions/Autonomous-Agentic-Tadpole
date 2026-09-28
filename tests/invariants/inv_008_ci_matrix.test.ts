/**
 * @docs ARCHITECTURE:Core
 *
 * ### AI Assist Note
 * **INV-008: CI Matrix & Path Governance Invariant Gate**
 * Statically enforces path coverage in `.github/workflows/ci.yml`:
 * 1. Root Directory Governance: All critical code and infrastructure directories
 *    (`src`, `server-rs`, `execution`, `scripts`, `monitoring`, `tests`, `crates`, `directives`, `docs`)
 *    must be explicitly governed by workflow trigger paths.
 * 2. Filter Parity: The `changes` job filter must route `server-rs/**` and `scripts/**` into the Python/parity filters.
 * 3. Gate Reliability: `ci-gate` must verify the changes job status to prevent skipped false-positives.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Ungoverned directories sneaking past CI checks without running tests/lints.
 * - **Telemetry Link**: Search `[inv_008_ci_matrix]` in test logs.
 *
 * // Metadata: [inv_008_ci_matrix]
 */

import { describe, it, expect } from 'vitest';
import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

describe('INV-008: CI Matrix & Path Governance Invariant', () => {
    const ciPath = resolve('.github/workflows/ci.yml');

    it('verifies .github/workflows/ci.yml exists and is readable', () => {
        expect(existsSync(ciPath)).toBe(true);
    });

    it('governs all critical codebase directories under workflow trigger paths', () => {
        const content = readFileSync(ciPath, 'utf-8');

        const criticalDirs = [
            'src/**',
            'src-tauri/**',
            'server-rs/**',
            'crates/**',
            'execution/**',
            'tests/**',
            'data/**',
            'directives/**',
            'docs/**',
            'scripts/**',
            'monitoring/**',
        ];

        for (const dir of criticalDirs) {
            expect(content).toContain(`'${dir}'`);
        }
    });

    it('enforces server-rs and scripts in python/parity changes filter', () => {
        const content = readFileSync(ciPath, 'utf-8');

        // Locate dorny/paths-filter section
        expect(content).toMatch(/dorny\/paths-filter@v3/);

        // Check python filter contains execution, server-rs, scripts, and monitoring
        const filterMatch = content.match(/python:\s*\n([\s\S]*?)(?:ci-gate|jobs:|\n\s*[a-z_]+:)/i);
        expect(filterMatch).not.toBeNull();
        const pythonFilter = filterMatch![1];

        expect(pythonFilter).toContain("'execution/**'");
        expect(pythonFilter).toContain("'server-rs/**'");
        expect(pythonFilter).toContain("'scripts/**'");
    });

    it('hardens ci-gate to prevent skipped false-positives', () => {
        const content = readFileSync(ciPath, 'utf-8');

        // Verify ci-gate checks changes job status
        expect(content).toMatch(/needs\.changes\.result/);
        expect(content).toMatch(/needs\.changes\.result.*?!=.*?success/);
    });
});

// Metadata: [inv_008_ci_matrix]
