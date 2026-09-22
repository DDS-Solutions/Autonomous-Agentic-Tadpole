/**
 * @docs ARCHITECTURE:Security
 *
 * ### AI Assist Note
 * **INV-006: Content Security Policy (CSP) Grammar & Browser Inference Invariant**
 * Statically parses tauri.conf.json and vite.config.ts to verify that:
 * 1. CSP is non-null and actively enforced.
 * 2. WebAssembly ('wasm-unsafe-eval') and model hub connections are permitted for browser AI inference.
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Null CSP, blocked WebAssembly inference, or vulnerable wildcards.
 * - **Telemetry Link**: Search `[inv_006_csp_grammar]` in test logs.
 *
 * // Metadata: [inv_006_csp_grammar]
 */

import { describe, it, expect } from 'vitest';
import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

describe('INV-006: CSP Grammar & Browser Inference Invariant', () => {
    const tauriConfPath = resolve('src-tauri/tauri.conf.json');
    const viteConfPath = resolve('vite.config.ts');

    it('requires tauri.conf.json to exist with active non-null security.csp', () => {
        expect(existsSync(tauriConfPath)).toBe(true);
        const conf = JSON.parse(readFileSync(tauriConfPath, 'utf-8'));
        const csp = conf?.app?.security?.csp;

        expect(csp).toBeTruthy();
        expect(typeof csp).toBe('string');
        expect(csp.trim().length).toBeGreaterThan(20);
    });

    it('enforces wasm-unsafe-eval in script-src for local browser AI inference', () => {
        const conf = JSON.parse(readFileSync(tauriConfPath, 'utf-8'));
        const csp = conf?.app?.security?.csp as string;

        // WebAssembly execution requires 'wasm-unsafe-eval' in script-src
        expect(csp).toMatch(/script-src[^;]*'wasm-unsafe-eval'/);
    });

    it('allows model downloads from HuggingFace hub in connect-src', () => {
        const conf = JSON.parse(readFileSync(tauriConfPath, 'utf-8'));
        const csp = conf?.app?.security?.csp as string;

        expect(csp).toMatch(/connect-src[^;]*https:\/\/huggingface\.co/);
    });

    it('allows web workers via blob: in worker-src', () => {
        const conf = JSON.parse(readFileSync(tauriConfPath, 'utf-8'));
        const csp = conf?.app?.security?.csp as string;

        expect(csp).toMatch(/worker-src[^;]*blob:/);
    });

    it('prohibits dangerous RFC 1918 private IP wildcards', () => {
        const conf = JSON.parse(readFileSync(tauriConfPath, 'utf-8'));
        const csp = conf?.app?.security?.csp as string;

        const rfc1918Wildcard = /https?:\/\/(?:192\.168|10\.|172\.(?:1[6-9]|2[0-9]|3[01]))\.?\*/i;
        expect(csp).not.toMatch(rfc1918Wildcard);
    });

    it('prohibits unquoted CSP keywords across both tauri and vite configurations', () => {
        const conf = JSON.parse(readFileSync(tauriConfPath, 'utf-8'));
        const tauriCsp = conf?.app?.security?.csp as string;
        const viteContent = readFileSync(viteConfPath, 'utf-8');

        // Check for unquoted keywords (e.g. `script-src self` instead of `'self'`)
        const unquotedKeywordPattern = /(?:^|\s)(?:default-src|script-src|style-src|connect-src|worker-src)\s+[^;]*\b(self|unsafe-inline|unsafe-eval|none)\b(?!\s*')/;

        expect(tauriCsp).not.toMatch(unquotedKeywordPattern);
        expect(viteContent).not.toMatch(unquotedKeywordPattern);
    });
});
