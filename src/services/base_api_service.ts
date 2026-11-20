/**
 * @docs ARCHITECTURE:Services
 *
 * ### AI Assist Note
 * **Unified HTTP Client Facade**: Single-source-of-truth transport facade delegating directly
 * to `src/api` (`BaseApiService`), enforcing loopback/desktop origin allowlisting (`is_allowed_origin`),
 * W3C TraceContext propagation, telemetry secret scrubbing (`scrub_secrets`), 10MB payload caps,
 * RFC 9457 typed errors, and bounded exponential backoff retries.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Origin policy refusal, 401/403 Auth Failure, 408/504 Timeout, or invalid JSON.
 * - **Telemetry Link**: Search `[base_api_service]` in console logs.
 *
 * @aiContext
 * - **Dependencies**: `src/api/service`, `src/api/legacy`, `src/api/errors`, `src/api/utils/signal`.
 * - **Side Effects**: Delegates all HTTP/retry/tracing execution to `base_api_service_instance`.
 * - **Mocking**: Mock `api_request` or global `fetch` for unit tests.
 */

import { base_api_service_instance } from '../api/legacy';
import type { RequestOptions } from '../api/types';

export {
    DEFAULT_TIMEOUT,
    DEPLOY_TIMEOUT,
    MAX_RETRIES,
    INITIAL_RETRY_DELAY,
} from '../api/constants';

export { ApiError, AuthError } from '../api/errors';
export { with_timeout } from '../api/utils/signal';
export { is_allowed_origin } from '../api/service/origin-policy';
export { base_api_service_instance } from '../api/legacy';

/**
 * Generates standardized headers and W3C TraceContext metadata via the unified `src/api` client.
 */
export function get_headers(
    custom_request_id?: string,
    skip_auth: boolean = false
): {
    headers: Record<string, string>;
    context: { span_id: string; trace_id: string; traceparent: string; request_id: string };
} {
    return base_api_service_instance.get_headers(custom_request_id, undefined, skip_auth);
}

/**
 * Executes an authenticated (or explicitly public `skip_auth: true`) HTTP request
 * through the consolidated `BaseApiService` security & observability pipeline.
 */
export async function api_request<T = unknown>(
    path: string,
    options: RequestOptions = {}
): Promise<T> {
    return base_api_service_instance.request<T>(path, options);
}

// Metadata: [base_api_service]
