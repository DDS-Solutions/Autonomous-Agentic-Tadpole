> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Task execution timeout or unhandled script errors.
> - **Telemetry Link**: Search `[codebase_health_mission]` in audit logs.
>
> ### AI Assist Note
> SOP directive for automated agent codebase health & integrity verification.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py` and `verify_ai_context.py`.

# 🎯 Mission Directive: Automated Codebase Health & Integrity Audit

**Target Agent**: Agent 2 (Alpha Swarm Orchestrator)  
**Execution Constraint**: Autonomous execution. Call tools directly without asking for permission.

---

## 📋 Mission Objective
Perform an autonomous codebase health check to verify AI context alignment, system parity, and security controls across all repository files.

---

## 🛠️ Step-by-Step Instructions

1. **Step 1: AI Context Alignment**
   - Call tool / script `python execution/verify_ai_context.py`.
   - Verify that all 1,064+ files contain required Knowledge Heritage headers.

2. **Step 2: System Parity & OpenAPI Check**
   - Call tool / script `python execution/parity_guard.py`.
   - Ensure REST routes, environment schemas, and version descriptors are 100% in sync.

3. **Step 3: Security & Key Audit**
   - Call tool / script `python execution/sovereign_audit.py`.
   - Confirm zero secret key leaks, zeroized RAM buffers, and database WAL integrity.

4. **Step 4: Report Synthesis**
   - Format results into an executive summary markdown report.
   - Output pass/fail status for each pillar.

[//]: # (Metadata: [codebase_health_mission])
