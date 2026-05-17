/**
 * @docs ARCHITECTURE:Services
 * 
 * ### AI Assist Note
 * **Governance Service**: Centralized nexus for managing swarm-wide resource boundaries and security protocols.
 * Orchestrates budgeting, safety gate enforcement (Aletheia), and telemetry synchronization.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: API desync, quota overflow, or socket disconnection.
 * - **Telemetry Link**: Search for `[Governance_Service]` in UI tracing.
 */

import { system_api_service } from './system_api_service';
import { tadpole_os_socket } from './socket';
import type { GovernanceQuotas } from '../contracts/governance';
import { event_bus } from './event_bus';

class Governance_Service {
    private static instance: Governance_Service;
    private quotas: GovernanceQuotas | null = null;
    private listeners: ((quotas: GovernanceQuotas) => void)[] = [];

    private constructor() {
        this.init_listeners();
    }

    public static get_instance(): Governance_Service {
        if (!Governance_Service.instance) {
            Governance_Service.instance = new Governance_Service();
        }
        return Governance_Service.instance;
    }

    private init_listeners() {
        // Event-driven sync: Listen for telemetry pulses from the socket
        tadpole_os_socket.subscribe_custom_event((data: Record<string, unknown>) => {
            if (data.type === 'GOVERNANCE_PULSE' && data.quotas) {
                this.update_quotas(this.map_quotas(data.quotas as Record<string, unknown>));
            }
        });
    }

    private map_quotas(q: Record<string, unknown>): GovernanceQuotas {
        const defense = q.system_defense as Record<string, unknown> | undefined;
        return {
            total_budget: Number(q.total_budget) || 0,
            total_spent: Number(q.total_spent) || 0,
            efficiency: Number(q.efficiency) || (q.total_budget ? Number(q.total_spent) / Number(q.total_budget) : 0),
            remaining: Number(q.remaining) || 0,
            agent_quotas: (q.agent_quotas as GovernanceQuotas['agent_quotas']) || [],
            system_defense: {
                merkle_integrity: Number(defense?.merkle_integrity) || 0,
                aletheia_status: (defense?.aletheia_status as 'verified' | 'bypassed') || 'verified',
                drift_status: (defense?.drift_status as 'stable' | 'drifting') || 'stable',
                memory_pressure: defense?.memory_pressure !== undefined ? Number(defense.memory_pressure) : undefined,
                cpu_load: defense?.cpu_load !== undefined ? Number(defense.cpu_load) : undefined,
                sandbox_status: (defense?.sandbox_status as 'ACTIVE' | 'OFFLINE') || 'OFFLINE',
                sandbox_type: (defense?.sandbox_type as string) || 'Unknown'
            },
            last_sync: (q.last_sync as string) || new Date().toISOString()
        };
    }

    private update_quotas(new_quotas: GovernanceQuotas) {
        this.quotas = new_quotas;
        this.listeners.forEach(cb => cb(new_quotas));
        event_bus.emit_log({
            text: `Governance Pulse: Budget utilization at ${(new_quotas.efficiency * 100).toFixed(1)}%`,
            severity: 'info',
            source: 'System'
        });
    }

    public async sync(): Promise<GovernanceQuotas> {
        const q = await system_api_service.get_security_quotas();
        const mapped = this.map_quotas(q as unknown as Record<string, unknown>);
        this.update_quotas(mapped);
        return mapped;
    }

    public async get_manifest(): Promise<string> {
        return await system_api_service.get_sovereign_manifest();
    }

    public async update_quota(entity_id: string, budget_usd: number): Promise<void> {
        // Enforce basic validation before sending to backend
        if (budget_usd < 0) throw new Error('Budget cannot be negative.');
        
        await system_api_service.update_security_quota(entity_id, budget_usd);
        await this.sync(); // Refresh after update
    }

    public on_pulse(callback: (quotas: GovernanceQuotas) => void): () => void {
        this.listeners.push(callback);
        return () => {
            this.listeners = this.listeners.filter(cb => cb !== callback);
        };
    }

    public get_current_quotas(): GovernanceQuotas | null {
        return this.quotas;
    }
}

export const governance_service = Governance_Service.get_instance();

// Metadata: [governance_service]

// Metadata: [governance_service]
