/**
 * @docs ARCHITECTURE:Security
 *
 * ### AI Assist Note
 * **Origin & URL Guard**: Validates WebSocket and HTTP engine URLs against loopback,
 * same-origin, private-network opt-in, and explicit allow-lists to prevent credential exfiltration.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Rejected non-HTTPS external URL or disallowed WebSocket origin.
 * - **Telemetry Link**: Search `[origin_guard]` in security logs.
 */

const origin_cache = new Map<string, boolean>();

function is_rfc1918_or_local(hostname: string): boolean {
    const clean = hostname.toLowerCase().trim();
    if (clean.endsWith('.local') || clean.endsWith('.lan') || clean.endsWith('.internal')) {
        return true;
    }
    const ipv4_match = clean.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/);
    if (ipv4_match) {
        const octets = ipv4_match.slice(1, 5).map(Number);
        if (octets.some(o => o < 0 || o > 255)) return false;
        const [o1, o2] = octets;
        if (o1 === 10) return true;
        if (o1 === 172 && o2 >= 16 && o2 <= 31) return true;
        if (o1 === 192 && o2 === 168) return true;
    }
    return false;
}

export function is_private_network_origin(hostname: string): boolean {
    const clean = hostname.toLowerCase().trim();
    if (clean === 'localhost' || clean === '127.0.0.1' || clean === '[::1]' || clean === '::1') {
        return true;
    }
    if (is_rfc1918_or_local(clean)) return true;
    const ipv4_match = clean.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/);
    if (ipv4_match && Number(ipv4_match[1]) === 127) return true;
    return false;
}

export function validate_and_sanitize_url(url_str: string, allow_private_network = false): string {
    const trimmed = url_str.trim();
    if (!trimmed) {
        throw new Error('URL is empty');
    }

    let parsed: URL;
    try {
        parsed = new URL(trimmed);
    } catch {
        throw new Error(`Invalid URL format: ${trimmed}`);
    }

    parsed.username = '';
    parsed.password = '';

    const protocol = parsed.protocol.toLowerCase();
    const hostname = parsed.hostname.toLowerCase();
    const clean_hostname = hostname.replace(/^\[|\]$/g, '');
    const is_loopback =
        clean_hostname === 'localhost' ||
        clean_hostname === '127.0.0.1' ||
        clean_hostname === '::1' ||
        clean_hostname.endsWith('.localhost');

    const is_private = allow_private_network && is_rfc1918_or_local(clean_hostname);

    if (protocol !== 'https:' && !is_loopback && !is_private) {
        throw new Error(`Insecure transmission blocked: external connection to ${hostname} must use HTTPS.`);
    }

    return parsed.toString().replace(/\/$/, '');
}

function run_origin_check(url_string: string, allowed_origins?: string[], allow_private_network = false): boolean {
    try {
        const url = new URL(url_string);
        const hostname = url.hostname.toLowerCase();
        const port = url.port || (url.protocol === 'https:' || url.protocol === 'wss:' ? '443' : '80');

        if (hostname === 'localhost' || hostname === '127.0.0.1' || hostname === '[::1]' || hostname === '::1') {
            return true;
        }

        if (typeof window !== 'undefined' && window.location) {
            const loc = window.location;
            const loc_port = loc.port || (loc.protocol === 'https:' ? '443' : '80');
            const url_scheme = url.protocol.replace(/^ws/, 'http');
            const loc_scheme = loc.protocol.replace(/^ws/, 'http');
            if (
                loc.hostname.toLowerCase() === hostname &&
                loc_scheme === url_scheme &&
                loc_port === port
            ) {
                return true;
            }
        }

        if (allow_private_network && is_private_network_origin(hostname)) {
            return true;
        }

        const env_allowed = (typeof import.meta !== 'undefined' && import.meta.env?.VITE_ALLOWED_ORIGINS) || '';
        const env_origins = env_allowed
            ? env_allowed.split(',').map((x: string) => x.trim().toLowerCase())
            : [];

        const runtime_allowed = allowed_origins
            ? allowed_origins.map(x => x.trim().toLowerCase())
            : [];

        const all_allowed = [...env_origins, ...runtime_allowed];

        return all_allowed.some(allowed => {
            if (!allowed) return false;
            if (allowed.includes('://')) {
                try {
                    const allowed_url = new URL(allowed);
                    const allowed_port = allowed_url.port || (allowed_url.protocol === 'https:' || allowed_url.protocol === 'wss:' ? '443' : '80');
                    return allowed_url.hostname.toLowerCase() === hostname
                        && allowed_url.protocol.replace(/^ws/, 'http') === url.protocol.replace(/^ws/, 'http')
                        && allowed_port === port;
                } catch {
                    return false;
                }
            }
            if (allowed.includes(':')) {
                const [allowed_host, allowed_p] = allowed.split(':');
                return allowed_host === hostname && allowed_p === port;
            }
            if (allowed.startsWith('*.')) {
                const root = allowed.slice(2);
                return hostname === root || hostname.endsWith(`.${root}`);
            }
            return allowed === hostname;
        });
    } catch {
        return false;
    }
}

export function is_allowed_origin(url_string: string, allowed_origins?: string[], allow_private_network = false): boolean {
    const env_allowed = (typeof import.meta !== 'undefined' && import.meta.env?.VITE_ALLOWED_ORIGINS) || '';
    const cache_key = `${url_string}::${(allowed_origins || []).join(',')}::${env_allowed}::${allow_private_network}`;

    if (origin_cache.has(cache_key)) {
        return origin_cache.get(cache_key)!;
    }

    const result = run_origin_check(url_string, allowed_origins, allow_private_network);
    if (origin_cache.size > 500) origin_cache.clear();
    origin_cache.set(cache_key, result);
    return result;
}

// Metadata: [origin_guard]
