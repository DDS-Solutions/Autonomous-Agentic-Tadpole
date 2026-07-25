/**
 * @docs ARCHITECTURE:Infrastructure
 * 
 * ### AI Assist Note
 * **Root/Core**: Manages the mock oversight. 
 * Part of the Tadpole-OS core layer.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Tool call mismatch with governance store (state desync) or timestamp parsing error in UI views.
 * - **Telemetry Link**: Search `[mock_oversight]` in console logs.
 */



export interface ToolCall {
    id: string;
    agent_id: string;
    cluster_id?: string;
    skill: string;
    description: string;
    params: Record<string, unknown>;
    timestamp: string;
}

export interface OversightEntry extends Partial<Omit<ToolCall, 'id'>> {
    id: string;
    tool_call?: ToolCall;
    decision: 'pending' | 'approved' | 'rejected';
    decided_by?: string;
    decided_at?: string;
    created_at: string;
}

export interface LedgerEntry extends Partial<Omit<ToolCall, 'id'>> {
    id: string;
    tool_call?: ToolCall;
    decision: 'approved' | 'rejected' | 'auto_approved';
    decided_by?: string;
    auto_approved?: boolean;
    approval_type?: 'hitl' | 'auto';
    requires_oversight?: boolean;
    result?: {
        success: boolean;
        output: string;
        error?: string;
        duration_ms: number;
    };
    is_verified?: boolean;
    timestamp: string;
    decided_at?: string;
    mission_id?: string;
    cluster_id?: string;
}

export const MOCK_PENDING: OversightEntry[] = [
    {
        id: 'ov-1',
        tool_call: {
            id: 'tc-1',
            agent_id: '1', // Agent of Nine
            skill: 'Execute Command',
            description: 'Deploying security patch to production gateway.',
            params: { target: 'gateway-01', payload: 'v1.4.2-sec' },
            timestamp: new Date().toISOString()
        },
        decision: 'pending',
        created_at: new Date(Date.now() - 120000).toISOString()
    },
    {
        id: 'ov-2',
        tool_call: {
            id: 'tc-2',
            agent_id: '3', // Strategic Alpha
            skill: 'Modify File',
            description: 'Updating server firewall rules to block suspicious IP range.',
            params: { path: '/etc/iptables.conf', action: 'append', rules: 'DROP 192.168.1.50/32' },
            timestamp: new Date().toISOString()
        },
        decision: 'pending',
        created_at: new Date(Date.now() - 45000).toISOString()
    }
];

export const MOCK_LEDGER: LedgerEntry[] = [
    {
        id: 'le-1',
        tool_call: {
            id: 'tc-old-1',
            agent_id: '7',
            skill: 'Read Logs',
            description: 'Scanning system logs for anomalies.',
            params: { lines: 50, filter: 'error' },
            timestamp: new Date(Date.now() - 500000).toISOString()
        },
        decision: 'approved',
        decided_by: 'human',
        approval_type: 'hitl',
        is_verified: true,
        result: {
            success: true,
            output: 'Scan complete. 0 critical errors found.',
            duration_ms: 450
        },
        timestamp: new Date(Date.now() - 480000).toISOString()
    },
    {
        id: 'le-2',
        tool_call: {
            id: 'tc-old-2',
            agent_id: '11',
            skill: 'Delete File',
            description: 'Attempting to remove temporary cache directory.',
            params: { path: '/tmp/old_cache' },
            timestamp: new Date(Date.now() - 300000).toISOString()
        },
        decision: 'rejected',
        decided_by: 'human',
        approval_type: 'hitl',
        is_verified: true,
        timestamp: new Date(Date.now() - 290000).toISOString()
    },
    {
        id: 'le-auto-1',
        tool_call: {
            id: 'tc-auto-1',
            agent_id: '2',
            skill: 'verify_ai_context',
            description: 'Auto-approved diagnostic: AI context alignment check across all 1,064 files.',
            params: { path: 'G:/Autonomous-Agentic-Tadpole' },
            timestamp: new Date(Date.now() - 60000).toISOString()
        },
        decision: 'auto_approved',
        decided_by: 'auto_policy',
        auto_approved: true,
        approval_type: 'auto',
        requires_oversight: false,
        is_verified: true,
        result: {
            success: true,
            output: '1063 files scanned. 0 alignment failures.',
            duration_ms: 2140
        },
        timestamp: new Date(Date.now() - 58000).toISOString()
    },
    {
        id: 'le-auto-2',
        tool_call: {
            id: 'tc-auto-2',
            agent_id: '2',
            skill: 'parity_guard',
            description: 'Auto-approved diagnostic: OpenAPI + route parity verification.',
            params: { fix: false },
            timestamp: new Date(Date.now() - 30000).toISOString()
        },
        decision: 'auto_approved',
        decided_by: 'auto_policy',
        auto_approved: true,
        approval_type: 'auto',
        requires_oversight: false,
        is_verified: true,
        result: {
            success: true,
            output: '114 routes verified. Parity 100%.',
            duration_ms: 890
        },
        timestamp: new Date(Date.now() - 28000).toISOString()
    }
];




// Metadata: [mock_oversight]
