/**
 * @file Security_Dashboard.test.tsx
 * @description Verifies the rendering and data handling of the Security Dashboard components.
 */
import { render, screen, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest';
import Security_Dashboard from './Security_Dashboard';
import { tadpole_os_service } from '../services/tadpoleos_service';
import { governance_service } from '../services/governance_service';
import { use_agent_store } from '../stores/agent_store';

vi.mock('../services/tadpoleos_service', () => ({
    tadpole_os_service: {
        get_audit_trail: vi.fn(),
        get_agent_health: vi.fn(),
    }
}));

vi.mock('../services/governance_service', () => ({
    governance_service: {
        get_current_quotas: vi.fn(),
        sync: vi.fn(),
        on_pulse: vi.fn(() => vi.fn()),
        update_quota: vi.fn(),
    }
}));

vi.mock('../stores/agent_store', () => ({
    use_agent_store: Object.assign(vi.fn(), {
        getState: vi.fn(),
        subscribe: vi.fn(),
    }),
}));

describe('Security_Dashboard', () => {
    const mock_audit_data = [
        {
            id: 'audit-1',
            agent_id: 'agent-1',
            skill: 'file_write',
            status: 'recorded',
            decision: 'approved',
            decided_at: new Date().toISOString(),
            created_at: new Date().toISOString(),
            is_verified: true
        }
    ];

    const mock_agent_health = {
        agents: [
            {
                agent_id: 'agent-1',
                name: 'Surveyor',
                status: 'active',
                failure_count: 0,
                is_healthy: true,
                is_throttled: false
            },
            {
                agent_id: 'agent-2',
                name: 'Guardian',
                status: 'degraded',
                failure_count: 5,
                is_healthy: false,
                is_throttled: true
            }
        ]
    };

    beforeEach(() => {
        vi.clearAllMocks();
        
        (governance_service.sync as Mock).mockResolvedValue({
            total_spent: 100,
            efficiency: 95,
            agent_quotas: [
                {
                    entity_id: 'agent-1',
                    used_usd: 0.123,
                    budget_usd: 1.0,
                    reset_period: 'daily'
                }
            ],
            system_defense: { 
                merkle_integrity: 1.0,
                memory_pressure: 0.15,
                sandbox_status: 'ACTIVE',
                sandbox_type: 'Isolated VM'
            }
        });
        
        (tadpole_os_service.get_audit_trail as Mock).mockResolvedValue({ data: mock_audit_data, total: 1 });
        (tadpole_os_service.get_agent_health as Mock).mockResolvedValue(mock_agent_health);
        
        (use_agent_store as unknown as Mock).mockReturnValue({
            agents: [
                { id: 'agent-1', name: 'Surveyor' },
                { id: 'agent-2', name: 'Guardian' }
            ]
        });
    });

    it('validates the audit trail rendering', async () => {
        render(<MemoryRouter><Security_Dashboard /></MemoryRouter>);
        await screen.findByText('file_write');
        // Use getAllByText since names appear in header and health monitor too
        expect(screen.getAllByText('Surveyor').length).toBeGreaterThan(0);
    });

    it('validates the Swarm Health Monitor functionality', async () => {
        render(<MemoryRouter><Security_Dashboard /></MemoryRouter>);
        
        // Wait for health monitor to render (it contains 'Swarm Health Monitor' heading)
        await screen.findByText(/Swarm Health Monitor/i);
        
        // Check healthy agent
        const surveyor_elements = screen.getAllByText('Surveyor');
        expect(surveyor_elements.length).toBeGreaterThan(0);
        expect(screen.getByText('0 Failures')).toBeInTheDocument();
        
        // Check degraded/throttled agent
        const guardian_elements = screen.getAllByText('Guardian');
        expect(guardian_elements.length).toBeGreaterThan(0);
        expect(screen.getByText('5 Failures')).toBeInTheDocument();
        expect(screen.getByText(/Throttled/i)).toBeInTheDocument();
        
        // Verify agent IDs are visible
        expect(screen.getByText('agent-1')).toBeInTheDocument();
        expect(screen.getByText('agent-2')).toBeInTheDocument();
    });

    it('validates the Verified Decisions card rendering', async () => {
        render(<MemoryRouter><Security_Dashboard /></MemoryRouter>);
        
        // Wait for dashboard to load
        await screen.findByText(/Verified Decisions/i);
        
        // 1. Check decision count (audit_trail_state.length)
        // In beforeEach, mock_audit_data has 1 entry
        expect(screen.getByText('1')).toBeInTheDocument();
        
        // 2. Check integrity percentage
        // In beforeEach, merkle_integrity is 1.0
        expect(screen.getByText(/100% Cryptographic integrity/i)).toBeInTheDocument();
    });

    it('validates the Dashboard Header section', async () => {
        render(<MemoryRouter><Security_Dashboard /></MemoryRouter>);
        
        // 1. Check Title and Subtitle (Wait for it)
        expect(await screen.findByText(/Oversight & Compliance Dashboard/i)).toBeInTheDocument();
        expect(await screen.findByText(/Real-time verification of agent autonomy/i)).toBeInTheDocument();
        
        // 2. Check Agent Status Dots (initials S and G)
        const status_container = await screen.findByLabelText(/Agent Status Indicators/i);
        expect(within(status_container).getByText('S')).toBeInTheDocument();
        expect(within(status_container).getByText('G')).toBeInTheDocument();
        
        // 3. Check System Integrity Status
        expect(await screen.findByText(/System Secured/i)).toBeInTheDocument();
    });

    it('validates the Periodic Resource Quotas card rendering', async () => {
        render(<MemoryRouter><Security_Dashboard /></MemoryRouter>);
        
        // Wait for dashboard to load
        await screen.findByText(/Periodic Resource Quotas/i);
        
        // 1. Check quota details for agent-1
        // We look for the formatted usage string "$0.123 / $1.00"
        expect(await screen.findByText(/\$0.123 \/ \$1.00/i)).toBeInTheDocument();
        
        // 2. Check usage percentage (0.123 / 1.0 * 100 = 12.3%)
        expect(await screen.findByText(/USAGE: 12.3%/i)).toBeInTheDocument();
        
        // 3. Check reset period
        expect(await screen.findByText(/daily/i)).toBeInTheDocument();
        expect(await screen.findByText(/Reset/i)).toBeInTheDocument();
    });

    it('validates the Proactive Defense Matrix rendering', async () => {
        render(<MemoryRouter><Security_Dashboard /></MemoryRouter>);
        
        // Wait for dashboard to load
        await screen.findByText(/Defense Matrix/i);
        
        // 1. Check Memory Pressure (0.15 = 15.0%)
        expect(await screen.findByText(/Memory Pressure/i)).toBeInTheDocument();
        expect(await screen.findByText(/15.0%/i)).toBeInTheDocument();
        
        // 2. Check Sandbox Type
        expect(await screen.findByText(/Isolated VM/i)).toBeInTheDocument();
        
        // 3. Check Secret Leak Prevention (hardcoded enabled in UI for now)
        expect(await screen.findByText(/Enabled/i)).toBeInTheDocument();
    });
});
