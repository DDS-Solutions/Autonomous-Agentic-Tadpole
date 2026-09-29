/**
 * @docs ARCHITECTURE:Core
 * 
 * ### AI Assist Note
 * **Origin Policy Enforcer**: Guards API communication endpoints by strictly permitting
 * only sovereign local loopback and trusted desktop origins to prevent credential exfiltration.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Connection refused if an unauthorized external URL is configured.
 * - **Telemetry Link**: Search `[origin_policy]` in UI tracing.
 */

/**
 * Verifies if the target API origin is in the allowed set.
 * Enforces strict default-deny: permits only local loopback (localhost, 127.0.0.1, ::1, 127.0.0.0/8)
 * and the official desktop origin (tauri://localhost, *.localhost), rejecting external network hosts
 * and the wildcard bind address (0.0.0.0) to prevent credential exfiltration.
 *
 * @param url The target URL to validate
 * @returns boolean indicating if the origin is permitted
 */
export function is_allowed_origin(url: string): boolean {
    try {
        const parsed = new URL(url);
        const protocol = parsed.protocol.toLowerCase();
        if (protocol !== 'http:' && protocol !== 'https:' && protocol !== 'tauri:') {
            return false;
        }
        if (protocol === 'tauri:' && parsed.hostname === 'localhost') {
            return true;
        }
        const host = parsed.hostname.toLowerCase().replace(/^\[|\]$/g, '');
        // Explicitly reject 0.0.0.0 bind destination
        if (host === '0.0.0.0') {
            return false;
        }
        // Allow loopback: localhost, *.localhost (e.g. tauri.localhost), ::1
        if (host === 'localhost' || host === '::1' || host.endsWith('.localhost')) {
            return true;
        }
        // Allow IPv4 loopback range 127.0.0.0/8
        const parts = host.split('.');
        if (parts.length === 4 && parts[0] === '127') {
            const allValid = parts.slice(1).every(part => {
                const n = Number(part);
                return /^\d+$/.test(part) && n >= 0 && n <= 255;
            });
            if (allValid) return true;
        }
        return false;
    } catch {
        return false;
    }
}

// Metadata: [origin_policy]
