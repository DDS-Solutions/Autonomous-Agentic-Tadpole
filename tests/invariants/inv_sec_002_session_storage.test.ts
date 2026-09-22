/**
 * @docs ARCHITECTURE:Security
 *
 * ### AI Assist Note
 * **INV-SEC-002: Session Storage Credential Isolation Invariant**
 * Statically scans the entire `src/` tree to guarantee that:
 * 1. Zero auth tokens, master keys, or credentials are written to `sessionStorage`.
 * 2. Zero reads (`sessionStorage.getItem`) occur across application source files.
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Credential leakage into browser sessionStorage.
 * - **Telemetry Link**: Search `[inv_sec_002_session_storage]` in test logs.
 *
 * // Metadata: [inv_sec_002_session_storage]
 */

import { describe, it, expect } from 'vitest';
import { readdirSync, statSync, readFileSync } from 'node:fs';
import { resolve, join, extname } from 'node:path';

function getSourceFiles(dir: string): string[] {
    const results: string[] = [];
    const entries = readdirSync(dir);

    for (const entry of entries) {
        const fullPath = join(dir, entry);
        const stat = statSync(fullPath);

        if (stat.isDirectory()) {
            results.push(...getSourceFiles(fullPath));
        } else {
            const ext = extname(entry);
            // Scan TypeScript and JavaScript production source files
            if (['.ts', '.tsx', '.js', '.jsx'].includes(ext) && !entry.includes('.test.') && !entry.includes('.spec.')) {
                results.push(fullPath);
            }
        }
    }

    return results;
}

describe('INV-SEC-002: Session Storage Credential Isolation Invariant', () => {
    const srcDir = resolve('src');
    const sourceFiles = getSourceFiles(srcDir);

    it('finds source files in src/', () => {
        expect(sourceFiles.length).toBeGreaterThan(10);
    });

    it('prohibits sessionStorage.setItem across all production source files', () => {
        const violations: { file: string; line: number; text: string }[] = [];

        for (const file of sourceFiles) {
            const content = readFileSync(file, 'utf-8');
            const lines = content.split('\n');

            lines.forEach((line, index) => {
                // Strip comments before evaluating
                const trimmed = line.trim();
                if (trimmed.startsWith('//') || trimmed.startsWith('*') || trimmed.startsWith('/*')) {
                    return;
                }
                if (/sessionStorage\s*\.\s*setItem/.test(line)) {
                    violations.push({ file, line: index + 1, text: line.trim() });
                }
            });
        }

        expect(violations).toEqual([]);
    });

    it('prohibits sessionStorage.getItem across all production source files', () => {
        const violations: { file: string; line: number; text: string }[] = [];

        for (const file of sourceFiles) {
            const content = readFileSync(file, 'utf-8');
            const lines = content.split('\n');

            lines.forEach((line, index) => {
                const trimmed = line.trim();
                if (trimmed.startsWith('//') || trimmed.startsWith('*') || trimmed.startsWith('/*')) {
                    return;
                }
                if (/sessionStorage\s*\.\s*getItem/.test(line)) {
                    violations.push({ file, line: index + 1, text: line.trim() });
                }
            });
        }

        expect(violations).toEqual([]);
    });

    it('ensures any sessionStorage usage is strictly restricted to defensive purge calls', () => {
        const nonPurgeUsages: { file: string; line: number; text: string }[] = [];

        for (const file of sourceFiles) {
            const content = readFileSync(file, 'utf-8');
            const lines = content.split('\n');

            lines.forEach((line, index) => {
                const trimmed = line.trim();
                if (trimmed.startsWith('//') || trimmed.startsWith('*') || trimmed.startsWith('/*')) {
                    return;
                }
                if (/sessionStorage\b/.test(line)) {
                    // Only removeItem and clear are permitted
                    const isAllowedPurge = /sessionStorage\s*\.\s*(?:removeItem|clear)\s*\(/.test(line);
                    if (!isAllowedPurge) {
                        nonPurgeUsages.push({ file, line: index + 1, text: line.trim() });
                    }
                }
            });
        }

        expect(nonPurgeUsages).toEqual([]);
    });
});
