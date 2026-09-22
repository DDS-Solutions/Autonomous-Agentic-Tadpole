/**
 * @docs ARCHITECTURE:Security
 *
 * ### AI Assist Note
 * **INV-002: PBKDF2 Iterations & Cryptographic Non-Extractability Invariant**
 * Asserts that cryptographic key derivation uses >= 600,000 PBKDF2 iterations,
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Weak iteration counts, extractable keys, or vault bricking.
 * - **Telemetry Link**: Search `[inv_002_vault_pbkdf2]` in test logs.
 *
 * // Metadata: [inv_002_vault_pbkdf2]
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { derive_key, encrypt_raw, decrypt_raw, DEFAULT_ITERATIONS } from '../../src/utils/crypto-core';

describe('INV-002: Vault PBKDF2 & Cryptographic Invariants', () => {
    const subtleMock = {
        importKey: vi.fn(),
        deriveKey: vi.fn(),
        encrypt: vi.fn(),
        decrypt: vi.fn(),
    };

    beforeEach(() => {
        vi.clearAllMocks();
        vi.stubGlobal('crypto', {
            subtle: subtleMock,
            getRandomValues: (arr: Uint8Array) => arr,
        });
    });

    it('enforces >= 600,000 PBKDF2 iterations by default and non-extractable keys', async () => {
        // Floor Invariant: DEFAULT_ITERATIONS must satisfy the OWASP security floor
        expect(DEFAULT_ITERATIONS).toBeGreaterThanOrEqual(600000);

        const mockBaseKey = { type: 'raw' };
        const mockDerivedKey = { type: 'secret', extractable: false };

        subtleMock.importKey.mockResolvedValue(mockBaseKey);
        subtleMock.deriveKey.mockResolvedValue(mockDerivedKey);

        const key = await derive_key('test-password', new Uint8Array(16));

        expect(key).toBe(mockDerivedKey);
        expect(subtleMock.deriveKey).toHaveBeenCalledWith(
            expect.objectContaining({
                name: 'PBKDF2',
                hash: 'SHA-256',
                iterations: DEFAULT_ITERATIONS,
            }),
            mockBaseKey,
            { name: 'AES-GCM', length: 256 },
            false, // Non-extractable key
            ['encrypt', 'decrypt']
        );
    });

    it('encrypt_raw stamps modern versioned envelope with default iterations', async () => {
        const mockKey = {};
        subtleMock.importKey.mockResolvedValue({});
        subtleMock.deriveKey.mockResolvedValue(mockKey);
        subtleMock.encrypt.mockResolvedValue(new Uint8Array([1, 2, 3, 4]).buffer);

        const envelopeJson = await encrypt_raw('vault-secret-content', 'master-pass');
        const envelope = JSON.parse(envelopeJson);

        expect(envelope).toHaveProperty('salt');
        expect(envelope).toHaveProperty('iv');
        expect(envelope).toHaveProperty('data');
        expect(envelope.kdf).toBe('PBKDF2-SHA256');
        expect(envelope.iterations).toBe(DEFAULT_ITERATIONS);
    });

    it('decrypt_raw supports legacy vaults lacking iterations field via 100,000 fallback', async () => {
        const mockKey = {};
        subtleMock.importKey.mockResolvedValue({});
        subtleMock.deriveKey.mockResolvedValue(mockKey);
        subtleMock.decrypt.mockResolvedValue(new TextEncoder().encode('legacy-recovered-plaintext').buffer);

        // Legacy envelope with NO iterations field
        const legacyEnvelope = JSON.stringify({
            salt: '00'.repeat(16),
            iv: '00'.repeat(12),
            data: '01020304',
        });

        const decrypted = await decrypt_raw(legacyEnvelope, 'master-pass');
        expect(decrypted).toBe('legacy-recovered-plaintext');

        // Verify fallback to 100,000
        expect(subtleMock.deriveKey).toHaveBeenCalledWith(
            expect.objectContaining({
                name: 'PBKDF2',
                iterations: 100000,
            }),
            expect.anything(),
            expect.anything(),
            false,
            ['encrypt', 'decrypt']
        );
    });
});
