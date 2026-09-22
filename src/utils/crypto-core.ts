/**
 * @docs ARCHITECTURE:Security
 * 
 * ### AI Assist Note
 * **Crypto Core**: Shared PBKDF2/AES-GCM logic used by both the main thread and workers. 
 * Standardizes the derivation of 256-bit keys from user passwords.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Decryption failure (incorrect password), PBKDF2 iteration mismatch, or missing `SubtleCrypto` in non-secure contexts.
 * - **Telemetry Link**: Search `[crypto_core]` in console logs.
 */
/**
 * @module utils/crypto-core
 * @description Core cryptographic logic shared between the main thread and workers.
 */

/**
 * Canonical default iterations for PBKDF2-HMAC-SHA256 key derivation.
 * Conforms to OWASP Password Storage Guidelines (floor >= 600,000).
 */
export const DEFAULT_PBKDF2_ITERATIONS = 600000;
export const DEFAULT_ITERATIONS = DEFAULT_PBKDF2_ITERATIONS;

/**
 * derive_key
 * Derives a cryptographic key from a password.
 */
export async function derive_key(password: string, salt: Uint8Array, iterations = DEFAULT_ITERATIONS): Promise<CryptoKey> {
    const encoder = new TextEncoder();
    const password_data = encoder.encode(password);

    if (typeof crypto === 'undefined' || !crypto.subtle) {
        throw new Error('Neural Secure Context (HTTPS/Localhost) Required for PBKDF2/AES operations.');
    }

    const base_key = await crypto.subtle.importKey(
        'raw',
        password_data,
        'PBKDF2',
        false,
        ['deriveBits', 'deriveKey']
    );

    return crypto.subtle.deriveKey(
        {
            name: 'PBKDF2',
            // SubtleCrypto requires ArrayBuffer-backed views; ensure we pass the narrow type.
            salt: new Uint8Array(salt) as Uint8Array<ArrayBuffer>,
            iterations,
            hash: 'SHA-256'
        },
        base_key,
        { name: 'AES-GCM', length: 256 },
        false,
        ['encrypt', 'decrypt']
    );
}

/**
 * encrypt_raw
 * Encrypts a string with PBKDF2 key derivation and AES-GCM.
 */
export async function encrypt_raw(text: string, password: string): Promise<string> {
    const salt = (typeof crypto !== 'undefined' && crypto.getRandomValues) ? crypto.getRandomValues(new Uint8Array(16)) : new Uint8Array(16);
    const iv = (typeof crypto !== 'undefined' && crypto.getRandomValues) ? crypto.getRandomValues(new Uint8Array(12)) : new Uint8Array(12);

    if (typeof crypto === 'undefined' || !crypto.subtle) {
        throw new Error('Neural Secure Context (HTTPS/Localhost) Required for encryption.');
    }

    const iterations = DEFAULT_ITERATIONS;
    const key = await derive_key(password, salt, iterations);

    const encoder = new TextEncoder();
    const encrypted = await crypto.subtle.encrypt(
        { name: 'AES-GCM', iv },
        key,
        encoder.encode(text)
    );

    const result = {
        salt: Array.from(salt).map(b => b.toString(16).padStart(2, '0')).join(''),
        iv: Array.from(iv).map(b => b.toString(16).padStart(2, '0')).join(''),
        data: Array.from(new Uint8Array(encrypted)).map(b => b.toString(16).padStart(2, '0')).join(''),
        kdf: 'PBKDF2-SHA256',
        iterations
    };

    return JSON.stringify(result);
}

/**
 * decrypt_raw
 * Decrypts a string. Supports versioned PBKDF2 envelopes and falls back
 * to legacy 100,000 iterations if iterations field is absent.
 */
export async function decrypt_raw(encrypted_json: string, password: string): Promise<string> {
    const parsed = JSON.parse(encrypted_json);
    const { salt, iv, data } = parsed;
    const iterations = typeof parsed.iterations === 'number' ? parsed.iterations : 100000;

    const salt_array = new Uint8Array(salt.match(/.{1,2}/g).map((byte: string) => parseInt(byte, 16)));
    const iv_array = new Uint8Array(iv.match(/.{1,2}/g).map((byte: string) => parseInt(byte, 16)));
    const data_array = new Uint8Array(data.match(/.{1,2}/g).map((byte: string) => parseInt(byte, 16)));

    const key = await derive_key(password, salt_array, iterations);

    try {
        const decrypted = await crypto.subtle.decrypt(
            { name: 'AES-GCM', iv: iv_array },
            key,
            data_array
        );

        const decoder = new TextDecoder();
        return decoder.decode(decrypted);
    } catch {
        throw new Error('Decryption failed');
    }
}




// Metadata: [crypto_core]
