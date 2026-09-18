/**
 * @docs ARCHITECTURE:Types
 * 
 * ### AI Assist Note
 * **Root Type Registry**: Authoritative TypeScript interfaces for the Tadpole OS frontend. 
 * Orchestrates the shape of `Agent`, `Task`, and `Department` models, ensuring 1:1 structural parity with the Rust backend.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: TypeScript compilation error (due to union type mismatch), or runtime object property absence when backend DTOs evolve.
 * - **Telemetry Link**: Search `[index]` in console logs.
 */


import type { Mission } from './mission';
export type { Mission };
export type { Mission_Cluster } from '../stores/workspace_store';

// Consolidated Agent Contracts (Phase 2 Migration)
import type {
  Agent,
  AgentPatch,
  AgentFormState,
  Agent_Connector_Config,
  Agent_Metadata,
  Agent_Model_Slot_Key,
  Agent_Model_Slot_State,
  Agent_Status,
  Agent_Stt_Engine,
  Agent_Voice_Engine,
  Department
} from '../contracts/agent';

export type {
  Agent,
  AgentPatch,
  AgentFormState,
  Agent_Connector_Config,
  Agent_Metadata,
  Agent_Model_Slot_Key,
  Agent_Model_Slot_State,
  Agent_Status,
  Agent_Stt_Engine,
  Agent_Voice_Engine,
  Department
};

export type Message_Part = 
    | { type: 'text', content: string, status?: 'complete' | 'streaming' }
    | { type: 'thought', content: string, status: 'thinking' | 'done' }
    | { type: 'tool', name: string, input: unknown, output?: unknown, status?: 'pending' | 'success' | 'error' }
    | { 
        type: 'question', 
        question: string, 
        options: string[], 
        context?: string, 
        question_id?: string, 
        selected_option?: string, 
        status?: 'pending' | 'answered' 
      }
    | {
        type: 'openui',
        /** The DSL payload describing the UI to render */
        dsl: OpenUI_DSL,
        status?: 'rendering' | 'complete'
      };

// ── OpenUI DSL Types ─────────────────────────────────────────

export type OpenUI_DSL = OpenUI_KPI_Card | OpenUI_Bar_Chart | OpenUI_Table | OpenUI_Layout;

export interface OpenUI_KPI_Card {
    kind: 'kpi_card';
    title: string;
    value: string | number;
    unit?: string;
    delta?: number;
    delta_label?: string;
    icon?: string;
}

export interface OpenUI_Bar_Chart {
    kind: 'bar_chart';
    title: string;
    labels: string[];
    datasets: {
        label: string;
        data: number[];
        color?: string;
    }[];
}

export interface OpenUI_Table {
    kind: 'table';
    title?: string;
    columns: { key: string; label: string; align?: 'left' | 'center' | 'right' }[];
    rows: Record<string, string | number | boolean>[];
    sortable?: boolean;
}

export interface OpenUI_Layout {
    kind: 'layout';
    direction: 'row' | 'column';
    children: OpenUI_DSL[];
}







/**
 * Swarm_Node
 * Represents a Bunker node in the Swarm network.
 */
export interface Swarm_Node {
  id: string;
  name: string;
  address: string;
  status: 'online' | 'offline' | 'deploying';
  last_seen: string;
  metadata: Record<string, string>;
  /** running_agents - IDs of agents currently running on this node */
  running_agents?: string[];
}
/**
 * Swarm_Pulse
 * High-speed binary telemetry for real-time swarm visualization.
 * Mirrored from server-rs/src/telemetry/pulse_types.rs for 1:1 parity.
 */
export interface Pulse_Node {
  id: string;
  x: number;
  y: number;
  status: number; // 0: idle, 1: busy, 2: error, 3: degraded
  battery: number;
  signal: number;
  progress: number;
}

export interface Pulse_Connection {
  source: string;
  target: string;
}

export interface Swarm_Pulse {
  timestamp: number;
  nodes: Pulse_Node[];
  edges: Pulse_Connection[];
}




// Metadata: [index]
