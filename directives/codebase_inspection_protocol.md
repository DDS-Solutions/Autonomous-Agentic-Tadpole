> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Agent:SwarmInspection**
> - **Failure Path**: Tool selection mismatch (`read_file` vs `read_codebase_file`), sandbox path isolation, or ungrounded speculative auditing.
> - **Telemetry Link**: Search `[codebase_inspection_protocol]` in audit logs.
>
> ### AI Assist Note
> 🗺️ Directive: Sovereign Codebase Inspection Protocol (SOP-SWARM-03)
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py` and `server.log.YYYY-MM-DD`.

# 🗺️ Directive: Sovereign Codebase Inspection Protocol (SOP-SWARM-03)

## 🎯 Primary Objective
Standardize the delegation and toolbelt invocation protocols for multi-tier swarm codebase audits. Ensure that when inspecting repository source files, Swarm Mission Commanders (Agent Alpha) and tactical specialists (e.g. Linus - Agent 8) utilize `read_codebase_file` instead of sandboxed `read_file`, guaranteeing zero "Proof-of-Work" failures while preserving strict sandbox isolation.

---

## 🏗️ Architectural Context: Sandbox vs. Codebase Root

In Tadpole OS (`server-rs`), filesystem operations are partitioned into two distinct security domains:

| Domain | Underlying Handler | Target Path Root | Permitted Operations | Typical Tools |
| :--- | :--- | :--- | :--- | :--- |
| **Workspace Sandbox** | `ctx.fs_adapter` | `data/workspaces/{cluster_id}/` (e.g. `executive-core`) | Ephemeral task scratchpad, intermediate files, test artifacts | `read_file`, `write_file`, `list_files`, `delete_file` |
| **Repository Codebase** | `runner.handle_read_codebase_file` | `self.state.base_dir` (`G:\Autonomous-Agentic-Tadpole`) | Read-only inspection of application source code (`server-rs`, `src`, `execution`) | `read_codebase_file`, `grep_search` |

> [!WARNING]
> Calling `read_file("server-rs/src/services/acl_service.rs")` attempts to locate the file inside `data/workspaces/executive-core/server-rs/...`, returning `(READ FAILED: File not found)`. **All repository-level codebase inspections MUST use `read_codebase_file`.**

---

## 📋 The 3-Tier Delegation Protocol

To ensure seamless vertical traversal without tripping Hierarchy Guards or CBS (Capability-Based Security) blocks:

1. **CEO (Agent 1 - Router):**
   * Must invoke `issue_alpha_directive`.
   * Prohibited from direct worker recruitment.
   * Explicitly mandates the target file and the requirement to use `read_codebase_file`.
2. **COO (Agent 2 - Operations):**
   * Must invoke `spawn_subagent(agent_id="alpha", message="...")`.
   * Prohibited from direct specialist recruitment.
3. **Swarm Commander (Agent Alpha - Node Commander):**
   * Spawns specialist (e.g., Linus / ID: 8).
   * Mandates:
     - Tool: `read_codebase_file`
     - Target path: Relative to repository root (e.g., `server-rs/src/services/acl_service.rs`).
     - Tool: `share_finding` to publish atomic findings back to the swarm.
     - Tool: `complete_mission` to trigger final synthesis and QA-99 audit.

---

## 🚀 Reusable Mission Payloads

### 1. HTTP Dispatch Payload (For `POST /v1/agents/1/tasks`)
Use this exact JSON structure when dispatching a mission to Agent 1 via the REST API or Python execution scripts:

```json
{
  "message": "CEO DIRECTIVE: Execute a rigorous codebase inspection of 'server-rs/src/services/acl_service.rs'. Issue an alpha directive to Tadpole Alpha (Agent 2). Instruct Alpha to recruit specialist Linus (ID: 8) using tool 'read_codebase_file' (path: 'server-rs/src/services/acl_service.rs') to inspect access control policies, error handling, and security boundaries. Linus must share findings via 'share_finding' and report completion confirmation back to the swarm.",
  "primaryGoal": "Audit server-rs/src/services/acl_service.rs via read_codebase_file and verify swarm toolbelt integrity",
  "analysis": true,
  "auto_resume": true
}
```

### 2. Alpha Commander Delegation Directive (Agent 2 -> Agent Alpha)
```text
Initiate a high-scrutiny codebase audit for the target repository file: 'server-rs/src/services/acl_service.rs'.
Recruit tactical specialist Linus (ID: 8) and instruct him to execute the following protocol:
1. Tool Invocation: Use 'read_codebase_file' with path parameter 'server-rs/src/services/acl_service.rs'.
   (DO NOT use 'read_file', as the target file resides in the project root outside the local workspace sandbox).
2. Code Analysis: Perform a deep audit of role-based permissions, the CEO/COO delegation rules, and error handling.
3. Knowledge Sharing: Broadcast verified facts using 'share_finding' with topic 'access_control_audit'.
4. Mission Finalization: Emit structured mission completion confirmation back to the Swarm Commander.
```

### 3. Specialist Execution Directive (Agent Alpha -> Specialist Linus)
```text
Specialist Linus (ID: 8):
Your objective is to audit the access control logic of Tadpole OS.
- Command: Call read_codebase_file(path="server-rs/src/services/acl_service.rs")
- Review Pillars:
  (a) Architectural Integrity: SOLID adherence, enum exhaustiveness.
  (b) Reliability: Error propagation and nil/none handling.
  (c) Security Posture: CBS bypass vectors and parameter isolation.
- Output: Call share_finding(topic="security", finding="...") followed by your synthesis.
```

---

## 🛡️ Invariant Rules & Guardrails

1. **Anti-Confabulation Rule:** If `read_codebase_file` fails or reports access denied, the specialist **MUST NEVER** invent hypothetical code or speculative vulnerabilities. It must emit a Proof-of-Work failure notice.
2. **Credential Sanitization:** `read_codebase_file` automatically blocks any path containing `.env`, `key`, `token`, `secret`, or `credential`. Never direct an agent to read configuration secrets.
3. **Identity Enactment:** All internal and outbound agent communication must carry `User-Agent: TadpoleOS/1.1.58`.
4. **Oversight Mode:** In strict governance environments, ensure `permission_policies` maps `read_codebase_file` to `allow` for automated testing.

---

## 📝 Expected Output Report Template (QA-99 Standard)

```markdown
# Codebase Audit Report: [Target File Name]

**Status**: SUCCESS / COMPLETED
**Target File**: `server-rs/src/services/acl_service.rs`
**Auditor**: Linus (ID: 8) | **Commander**: Alpha (Swarm Commander)
**Tool Utilized**: `read_codebase_file` (Verified Ground-Truth Source)

## 1. Architectural Integrity
- Modularity & Layering: [Findings]
- Design Pattern Adherence: [Findings]

## 2. Reliability & Robustness
- Type Safety & Boundary Checks: [Findings]
- Concurrency & Async Safety: [Findings]

## 3. Security Posture
- Privilege Escalation Risks: [Findings]
- Input Sanitization & Parameter Isolation: [Findings]

## 4. Verification Evidence
- Exact Symbols Inspected: `AclService`, `is_tool_allowed`, `get_role_protocols`
- Ground-Truth Snippet Hash: [Hash/Signature]
```

<!-- Telemetry Tag: [codebase_inspection_protocol] -->

