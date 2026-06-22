/*
@docs ARCHITECTURE:UI

### AI Assist Note
**🛡️ Tadpole OS: Date Utils**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

/**
 * @file date_utils.ts
 * @description Centralized date handling utilities for Tadpole OS.
 */

/**
 * Robustly parses a date from various potential fields in an object or a raw value.
 * Returns a Date object or the provided fallback (defaults to null) if parsing fails.
 */
export const get_safe_date = (input: unknown, fallback: Date | null = null): Date | null => {
    if (!input) return fallback;

    // 1. If it's already a Date object
    if (input instanceof Date) {
        return isNaN(input.getTime()) ? fallback : input;
    }

    // 2. If it's a string or number, try parsing it directly first
    if (typeof input === 'string' || typeof input === 'number') {
        const d = new Date(input);
        if (!isNaN(d.getTime())) return d;
    }

    // 3. Otherwise, check for common date fields in an object
    if (typeof input === 'object' && input !== null) {
        const obj = input as Record<string, unknown>;
        const tool_call = obj.tool_call as Record<string, unknown> | undefined;
        const raw = (obj.timestamp || 
                    obj.created_at || 
                    obj.decided_at || 
                    obj.started_at || 
                    obj.completed_at || 
                    obj.next_run_at ||
                    tool_call?.timestamp || 
                    tool_call?.created_at) as string | number | Date | undefined;
        
        if (!raw) return fallback;
        const d = new Date(raw as string | number | Date);
        return isNaN(d.getTime()) ? fallback : d;
    }

    return fallback;
};

// Metadata: [date_utils]

// Metadata: [date_utils]
