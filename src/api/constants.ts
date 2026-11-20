/**
 * @docs ARCHITECTURE:Core
 * 
 * ### AI Assist Note
 * **Core technical resource for the Tadpole OS Sovereign infrastructure.**
 * Handles reactive state and high-fidelity user interactions.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI regression, hook desync, or API timeout.
 * - **Telemetry Link**: Search `[constants]` in observability traces.
 */

export const DEFAULT_TIMEOUT = 110000; // 110s aligned with server handler envelope
export const DEPLOY_TIMEOUT = 110000; // 110s server hard cap
export const MAX_RETRIES = 3;
export const INITIAL_RETRY_DELAY = 1000;

// Metadata: [constants]
