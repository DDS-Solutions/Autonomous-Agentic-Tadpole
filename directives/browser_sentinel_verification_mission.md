> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Task execution timeout or unhandled script errors.
> - **Telemetry Link**: Search `[browser_sentinel_verification_mission]` in audit logs.
>
> ### AI Assist Note
> SOP directive for automated testing of the Browser Sentinel & WebGPU specialist core.
>
> ### 🔍 Debugging & Observability
> Traceability via `verify_browser_sentinel.py`.

# 🎯 Mission Directive: Browser Sentinel & WebGPU Core Verification

**Target Agent**: Agent 2 (Alpha Swarm Orchestrator)  
**Execution Constraint**: Autonomous execution. Call tools directly without asking for permission.

---

## 📋 Mission Objective
Run an end-to-end verification mission on the **Browser Sentinel (WebGPU Specialist & Autonomous Daemon)** to validate zero-latency tactical routing, 4-bit ONNX model quantization, VRAM memory pressure protection, autonomous idle DOM evaluations, pipeline disposal/recovery, and the deterministic pre-flight DLP secret shield.

---

## 🛠️ Step-by-Step Instructions

1. **Step 1: Execute Sentinel Unit & Integration Tests**
   - Run `npx vitest run src/services/sentinel_daemon.test.ts src/stores/browser_specialist_store.test.ts src/utils/security_utils.test.ts src/logic/command_processor.test.ts src/pages/Settings_Handoff.test.tsx`.
   - Verify 100% test pass rate across:
     - Autonomous Sentinel Daemon lifecycle, idle loop, and circuit breakers (hidden tab, critical VRAM, busy pipeline).
     - Browser Specialist store reactivity, DOM summarization, and anomaly tracking.
     - Deterministic DLP pre-flight secret redaction (OpenAI, Anthropic, Google, GitHub, Bearer, private keys).
     - Tactical intent interception and Governance/Settings toggle persistence.

2. **Step 2: Run Browser Sentinel Diagnostic Script**
   - Run `python execution/verify_browser_sentinel.py`.
   - Verify that VRAM pressure thresholds (85% warning, 95% entrance, 82% exit) and device loss listeners are verified.

3. **Step 3: Verify AI Context & Code Quality**
   - Run `python execution/verify_ai_context.py`.
   - Confirm 100% alignment across all repository files.

4. **Step 4: Publish Mission Debrief**
   - Synthesize results into `reports/MISSION_DEBRIEF_BROWSER_SENTINEL.md`.

[//]: # (Metadata: [browser_sentinel_verification_mission])
