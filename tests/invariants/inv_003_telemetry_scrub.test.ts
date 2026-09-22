/**
 * @docs ARCHITECTURE:Security
 *
 * ### AI Assist Note
 * **INV-003: Multi-Provider Telemetry Secret Redaction Invariant**
 * Asserts that scrub_string and scrub_secrets redact all known AI provider
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Plaintext API key leak in logs or telemetry buffer.
 * - **Telemetry Link**: Search `[inv_003_telemetry_scrub]` in test logs.
 *
 * // Metadata: [inv_003_telemetry_scrub]
 */

import { describe, it, expect } from 'vitest';
import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { scrub_string, scrub_secrets } from '../../src/api/utils/scrub';

describe('INV-003: Telemetry Secret Redaction Invariant', () => {
    describe('scrub_string provider key patterns', () => {
        it('redacts Google Gemini API keys (AIza...)', () => {
            const raw = 'api_key=AIzaSyBdefghijklmnopqrstuvwxyz123456789; status=ok';
            const scrubbed = scrub_string(raw);
            expect(scrubbed).not.toContain('AIzaSyB');
            expect(scrubbed).toContain('[REDACTED]');
        });

        it('redacts Groq API keys (gsk_...)', () => {
            const raw = 'Authorization: gsk_1234567890abcdefghijklmnopqrstuvwxyz';
            const scrubbed = scrub_string(raw);
            expect(scrubbed).not.toContain('gsk_');
            expect(scrubbed).toContain('[REDACTED]');
        });

        it('redacts HuggingFace tokens (hf_...)', () => {
            const raw = 'model download token: hf_abcdefghijklmnopqrstuvwxyz123456';
            const scrubbed = scrub_string(raw);
            expect(scrubbed).not.toContain('hf_');
            expect(scrubbed).toContain('[REDACTED]');
        });

        it('redacts standard GitHub PATs (ghp_...)', () => {
            const raw = 'git clone https://ghp_0123456789abcdefghijklmnopqrstuvwxyz@github.com';
            const scrubbed = scrub_string(raw);
            expect(scrubbed).not.toContain('ghp_');
            expect(scrubbed).toContain('[REDACTED]');
        });

        it('redacts fine-grained GitHub PATs (github_pat_...)', () => {
            const raw = 'token: github_pat_11ABCD0123456789abcdef_ghijklmnopqrstuvwxyz0123456789';
            const scrubbed = scrub_string(raw);
            expect(scrubbed).not.toContain('github_pat_');
            expect(scrubbed).toContain('[REDACTED]');
        });

        it('redacts OpenAI & Anthropic secret keys (sk-... and sk-ant-...)', () => {
            const openai = 'Bearer sk-1234567890abcdef1234567890abcdef';
            const anthropic = 'x-api-key: sk-ant-api03-abcdef1234567890abcdef';

            expect(scrub_string(openai)).not.toContain('sk-1234567890');
            expect(scrub_string(anthropic)).not.toContain('sk-ant-');
        });

        it('redacts standard HTTP Bearer tokens', () => {
            const raw = 'Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.e30.signature';
            const scrubbed = scrub_string(raw);
            expect(scrubbed).not.toContain('eyJhbGci');
            expect(scrubbed).toContain('Bearer [REDACTED]');
        });
    });

    describe('scrub_secrets payload sanitization', () => {
        it('recursively scrubs sensitive object keys in JSON objects', () => {
            const payload = {
                user: 'operator',
                api_key: 'AIzaSyB123456789012345678901234567890123',
                nested: {
                    token: 'secret_token_val',
                    safe_field: 'public_data'
                }
            };

            const scrubbed = scrub_secrets(payload) as Record<string, unknown>;
            expect(scrubbed.api_key).toBe('[REDACTED]');
            expect((scrubbed.nested as Record<string, unknown>).token).toBe('[REDACTED]');
            expect((scrubbed.nested as Record<string, unknown>).safe_field).toBe('public_data');
        });
    });

    describe('.gitleaks.toml synchronization (Single Source of Truth)', () => {
        it('parses .gitleaks.toml and enforces that scrub.ts redacts all declared secret patterns', () => {
            const gitleaksPath = resolve('.gitleaks.toml');
            expect(existsSync(gitleaksPath)).toBe(true);
            const content = readFileSync(gitleaksPath, 'utf-8');

            // Parse [[rules]] from .gitleaks.toml
            const ruleMatches = [...content.matchAll(/\[\[rules\]\][\s\S]*?id\s*=\s*["']([^"']+)["'][\s\S]*?regex\s*=\s*'''([\s\S]*?)'''/g)];
            expect(ruleMatches.length).toBeGreaterThan(0);

            for (const match of ruleMatches) {
                const ruleId = match[1];
                const patternStr = match[2].trim();
                const ruleRegex = new RegExp(patternStr);

                // Deterministic synthetic samples for provider rules
                let sampleSecret = '';
                if (ruleId === 'openai-anthropic-key') {
                    sampleSecret = 'sk-1234567890abcdef123456';
                } else if (ruleId === 'google-gemini-key') {
                    sampleSecret = 'AIzaSyBdefghijklmnopqrstuvwxyz123456789';
                } else if (ruleId === 'groq-api-key') {
                    sampleSecret = 'gsk_1234567890abcdefghijklmnopqrstuvwxyz';
                } else if (ruleId === 'huggingface-api-token') {
                    sampleSecret = 'hf_abcdefghijklmnopqrstuvwxyz123456';
                } else if (ruleId === 'github-classic-pat') {
                    sampleSecret = 'ghp_0123456789abcdefghijklmnopqrstuvwxyz';
                } else if (ruleId === 'github-fine-grained-pat') {
                    sampleSecret = 'github_pat_11ABCD0123456789abcdef_ghijklmnopqrstuvwxyz0123456789';
                } else {
                    throw new Error(`Unmapped gitleaks rule '${ruleId}' in invariant suite: define synthetic validation sample in inv_003.`);
                }

                // Invariant 1: Sample matches the gitleaks regex
                expect(ruleRegex.test(sampleSecret)).toBe(true);

                // Invariant 2: scrub_string in scrub.ts redacts the sample secret completely
                const testPayload = `Authorization token: ${sampleSecret} (end of payload)`;
                const scrubbed = scrub_string(testPayload);
                expect(scrubbed).not.toContain(sampleSecret);
                expect(scrubbed).toContain('[REDACTED]');
            }
        });
    });
});
