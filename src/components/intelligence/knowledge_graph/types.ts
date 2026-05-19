/**
 * @docs ARCHITECTURE:UI-Components
 * 
 * ### AI Assist Note
 * **Core technical resource for the Tadpole OS Sovereign infrastructure.**
 * Handles reactive state and high-fidelity user interactions.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI regression, hook desync, or API timeout.
 * - **Telemetry Link**: Search `[types]` in observability traces.
 */

import type { SymbolNode } from '../../../types/schemas';

export interface ExtendedGraphNode extends SymbolNode {
    id: string;
    is_affected: boolean;
    x?: number;
    y?: number;
    vx?: number;
    vy?: number;
    fx?: number;
    fy?: number;
}

export interface ForceGraphLink {
    source: string | ExtendedGraphNode;
    target: string | ExtendedGraphNode;
}

// Metadata: [types]
