/**
 * @docs ARCHITECTURE:Security
 * 
 * ### AI Assist Note
 * **Crypto Bridge**: Interfaces with `crypto.worker.ts` to offload intensive decryption/encryption 
 * to a background thread, preventing UI jank during large workspace operations.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Worker initialization failure, message timeout, or invalid JSON payload.
 * - **Telemetry Link**: Search `[crypto]` in console logs.
 */
// Worker instance
let crypto_worker: Worker | null = null;
const pending_requests = new Map<string, { resolve: (val: string) => void, reject: (err: Error) => void }>();

const WORKER_TIMEOUT_MS = 15000;

/**
 * get_worker
 * Initializes or retrieves the cryptographic WebWorker singleton.
 */
function get_worker(): Worker {
    if (!crypto_worker) {
        // Use standard Worker constructor with Vite/Web-friendly URL
        crypto_worker = new Worker(new URL('../workers/crypto.worker.ts', import.meta.url), { type: 'module' });
        crypto_worker.onmessage = (event) => {
            const { id, success, payload, error } = event.data;
            const req = pending_requests.get(id);
            if (req) {
                if (success) req.resolve(payload);
                else req.reject(new Error(error));
                pending_requests.delete(id);
            }
        };
        crypto_worker.onerror = (err) => {
            console.error('[CryptoWorker] Fatal Error:', err);
            const current_error = new Error('[CryptoWorker] Cryptographic worker encountered a fatal error');
            for (const [, req] of pending_requests.entries()) {
                req.reject(current_error);
            }
            pending_requests.clear();
        };
    }
    return crypto_worker;
}

/**
 * call_worker
 * Dispatches a cryptographic request to the background worker with a bounded timeout.
 */
function call_worker(type: 'encrypt' | 'decrypt', payload: { text?: string, password?: string, encrypted_json?: string }): Promise<string> {
    const id = (typeof crypto !== 'undefined' && crypto.randomUUID) ? crypto.randomUUID() : `msg-${Date.now()}-${Math.random().toString(36).slice(2, 11)}`;
    const worker = get_worker();

    return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
            if (pending_requests.has(id)) {
                pending_requests.delete(id);
                reject(new Error(`[CryptoWorker] Request timed out after ${WORKER_TIMEOUT_MS}ms`));
            }
        }, WORKER_TIMEOUT_MS);

        pending_requests.set(id, {
            resolve: (val: string) => {
                clearTimeout(timer);
                resolve(val);
            },
            reject: (err: Error) => {
                clearTimeout(timer);
                reject(err);
            }
        });
        worker.postMessage({ id, type, payload });
    });
}

/**
 * encrypt_text
 * Encrypts a string using a password (delegated to worker).
 */
export async function encrypt_text(text: string, password: string): Promise<string> {
    return call_worker('encrypt', { text, password });
}

/**
 * decrypt_text
 * Decrypts a JSON-formatted encrypted string (delegated to worker).
 */
export async function decrypt_text(encrypted_json: string, password: string): Promise<string> {
    return await call_worker('decrypt', { encrypted_json, password });
}




// Metadata: [crypto]
