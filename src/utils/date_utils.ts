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
export const get_safe_date = (input: any, fallback: Date | null = null): Date | null => {
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
    const raw = input.timestamp || 
                input.created_at || 
                input.decided_at || 
                input.started_at || 
                input.completed_at || 
                input.next_run_at ||
                input.tool_call?.timestamp || 
                input.tool_call?.created_at;
    
    if (!raw) return fallback;
    const d = new Date(raw);
    return isNaN(d.getTime()) ? fallback : d;
};

// Metadata: [date_utils]

// Metadata: [date_utils]
