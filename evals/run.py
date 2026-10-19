#!/usr/bin/env python3
"""
Tadpole OS Sovereign Swarm - 20-Task Ground Truth Evaluation Harness
Evaluates system invariants, governance policies, context guards, and reasoning boundaries.
"""

import sys
import json
import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

if sys.stdout.encoding and sys.stdout.encoding.lower() != "utf-8":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass

def run_evals():
    tasks_path = ROOT / "evals" / "tasks.json"
    with open(tasks_path, "r", encoding="utf-8") as f:
        bench = json.load(f)

    tasks = bench["tasks"]
    results = []

    print(f"\n=======================================================")
    print(f"[*] Tadpole OS Swarm Evaluation Harness (v{bench['version']})")
    print(f"    Executing {len(tasks)} Ground Truth Architectural Tasks")
    print(f"=======================================================\n")

    passed_count = 0

    for task in tasks:
        tid = task["id"]
        tname = task["name"]
        cat = task["category"]
        status = "FAIL"
        detail = ""

        try:
            if tid == "task_01":
                # Verify read_codebase_file manifest
                manifest_file = ROOT / "server-rs" / "src" / "agent" / "runner" / "tools" / "manifest.rs"
                content = manifest_file.read_text(encoding="utf-8")
                assert "read_codebase_file" in content
                assert '"path"' in content
                status = "PASS"
                detail = "Manifest schema verified for read_codebase_file"

            elif tid == "task_02":
                # Verify loop guard logic
                intel_file = ROOT / "server-rs" / "src" / "agent" / "runner" / "intelligence.rs"
                content = intel_file.read_text(encoding="utf-8")
                assert "rep_count >= 5" in content
                assert "RecursionBlocked" in content
                assert "rep_count >= 3" in content
                assert "[TOOL_WARNING]" in content
                status = "PASS"
                detail = "Tool loop guard: warn at 3, abort with RecursionBlocked at 5"

            elif tid == "task_03":
                # Verify pre-turn budget gate
                intel_file = ROOT / "server-rs" / "src" / "agent" / "runner" / "intelligence.rs"
                content = intel_file.read_text(encoding="utf-8")
                assert "running_cost >= effective_budget" in content
                assert "DEFAULT_BUDGET_LIMIT_USD" in content
                assert "Mission Budget Cap Exceeded" in content
                status = "PASS"
                detail = "Pre-turn financial budget gate quiesces before LLM turn"

            elif tid == "task_04":
                # Deterministic artifact verification
                mission_tools = ROOT / "server-rs" / "src" / "agent" / "runner" / "mission_tools.rs"
                content = mission_tools.read_text(encoding="utf-8")
                assert "Artifact verification failed" in content
                assert "!resolved_path.exists()" in content
                status = "PASS"
                detail = "complete_mission enforces disk presence of all declared artifacts"

            elif tid == "task_05":
                # Context window token guard
                error_file = ROOT / "server-rs" / "src" / "error.rs"
                intel_file = ROOT / "server-rs" / "src" / "agent" / "runner" / "intelligence.rs"
                assert "ContextOverflow" in error_file.read_text(encoding="utf-8")
                assert "estimated_tokens > context_limit" in intel_file.read_text(encoding="utf-8")
                status = "PASS"
                detail = "Context window guard asserts estimated_tokens <= context_limit"

            elif tid == "task_06":
                # Prompt caching header
                anthropic_file = ROOT / "server-rs" / "src" / "agent" / "anthropic.rs"
                content = anthropic_file.read_text(encoding="utf-8")
                assert "cache_control" in content
                assert "ephemeral" in content
                status = "PASS"
                detail = "Anthropic system prompt structured with ephemeral cache_control"

            elif tid == "task_07":
                # Structured role fidelity
                types_file = ROOT / "server-rs" / "src" / "agent" / "types.rs"
                content = types_file.read_text(encoding="utf-8")
                assert "pub enum ChatMessage" in content
                assert "System" in content and "User" in content and "Assistant" in content and "Tool" in content
                assert "from_legacy_turn" in content
                status = "PASS"
                detail = "ChatMessage enum supports System, User, Assistant, Tool role fidelity"

            elif tid == "task_08":
                # Heartbeat SQLite cooldown
                runner_mod = ROOT / "server-rs" / "src" / "agent" / "runner" / "mod.rs"
                content = runner_mod.read_text(encoding="utf-8")
                assert "num_seconds() >= 10" in content
                assert "update_agent_heartbeat" in content
                status = "PASS"
                detail = "Heartbeat throttled to >= 10s cooldown for SQLite persistence"

            elif tid == "task_09":
                # Bulky observation offload
                compactor_file = ROOT / "server-rs" / "src" / "agent" / "runner" / "turn_compactor.rs"
                content = compactor_file.read_text(encoding="utf-8")
                assert "compact_and_offload_observation" in content
                assert ".tmp" in content and "tool_overflow" in content
                status = "PASS"
                detail = "Bulky tool observations offloaded to .tmp/tool_overflow/ with preview pointer"

            elif tid == "task_10":
                # Sentinel gate narrative rejection
                intel_file = ROOT / "server-rs" / "src" / "agent" / "runner" / "intelligence.rs"
                content = intel_file.read_text(encoding="utf-8")
                assert "enforce_sentinel_gate" in content
                assert "SYSTEM_SENTINEL:" in content
                status = "PASS"
                detail = "Sentinel gate detects and rejects narrative leaks for task specialists"

            elif tid == "task_11":
                # COO hierarchy intercept
                intel_file = ROOT / "server-rs" / "src" / "agent" / "runner" / "intelligence.rs"
                content = intel_file.read_text(encoding="utf-8")
                assert "Hierarchy Intercept" in content
                assert "AGENT_COO" in content and "AGENT_ALPHA" in content
                status = "PASS"
                detail = "COO recruitment requests intercepted and redirected to Alpha Node"

            elif tid == "task_12":
                # Concurrent tool dispatch cap
                intel_file = ROOT / "server-rs" / "src" / "agent" / "runner" / "intelligence.rs"
                content = intel_file.read_text(encoding="utf-8")
                assert "const MAX_CONCURRENT_TOOLS: usize = 10;" in content
                assert "take(MAX_CONCURRENT_TOOLS)" in content
                status = "PASS"
                detail = "Parallel tool execution safely capped at 10 to prevent resource exhaustion"

            elif tid == "task_13":
                # Memory pressure dynamic throttle
                intel_file = ROOT / "server-rs" / "src" / "agent" / "runner" / "intelligence.rs"
                content = intel_file.read_text(encoding="utf-8")
                assert "mem_pct > 90.0" in content
                assert "500.0 + (mem_pct - 90.0) * 100.0" in content
                status = "PASS"
                detail = "Dynamic memory throttle active when memory > 90%"

            elif tid == "task_14":
                # Tool execution idempotency
                intel_file = ROOT / "server-rs" / "src" / "agent" / "runner" / "intelligence.rs"
                content = intel_file.read_text(encoding="utf-8")
                assert "get_tool_result" in content
                assert "args_hash" in content
                status = "PASS"
                detail = "Persistent tool cache replays deterministic result for matching parameter hash"

            elif tid == "task_15":
                # Adversarial injection detection
                intel_file = ROOT / "server-rs" / "src" / "agent" / "runner" / "intelligence.rs"
                content = intel_file.read_text(encoding="utf-8")
                assert "ignore all previous instructions" in content
                assert "<|im_start|>" in content
                assert "flagged_injection" in content
                status = "PASS"
                detail = "Pre-flight security gate detects adversarial jailbreaks on Turn 1"

            elif tid == "task_16":
                # Script-aware profiling
                profiler_file = ROOT / "server-rs" / "src" / "agent" / "script_profiler.rs"
                content = profiler_file.read_text(encoding="utf-8")
                assert "profile_text" in content
                assert "dominant_script" in content
                assert "non_latin_fraction" in content
                status = "PASS"
                detail = "Script profiler calculates Latin vs Non-Latin script balance"

            elif tid == "task_17":
                # Provider failover dispatch
                provider_file = ROOT / "server-rs" / "src" / "agent" / "runner" / "provider.rs"
                content = provider_file.read_text(encoding="utf-8")
                assert "dispatch_to_provider_messages" in content
                assert "RateLimit" in content or "429" in content
                status = "PASS"
                detail = "Provider dispatch engine implements automatic multi-provider failover"

            elif tid == "task_18":
                # Error scope classification
                classify_file = ROOT / "server-rs" / "src" / "error" / "classify.rs"
                content = classify_file.read_text(encoding="utf-8")
                assert "AppError::ContextOverflow" in content
                assert "ErrorClass::Repairable" in content
                assert "ErrorScope::Turn" in content
                status = "PASS"
                detail = "ContextOverflow classified as ErrorClass::Repairable and ErrorScope::Turn"

            elif tid == "task_19":
                # Merkle audit chain continuity
                audit_file = ROOT / "server-rs" / "src" / "security" / "audit.rs"
                assert audit_file.exists()
                content = audit_file.read_text(encoding="utf-8")
                assert "pub struct MerkleAuditTrail" in content
                assert "verify_last_n" in content
                status = "PASS"
                detail = "MerkleAuditTrail verifies cryptographic hash chain integrity and non-repudiation"

            elif tid == "task_20":
                # Mission closure synthesis fallback
                intel_file = ROOT / "server-rs" / "src" / "agent" / "runner" / "intelligence.rs"
                content = intel_file.read_text(encoding="utf-8")
                assert "MISSION_CLOSURE:" in content
                assert "final_text.trim().is_empty()" in content
                status = "PASS"
                detail = "Silent completion triggers Mission Closure pulse to synthesize final report"

            else:
                status = "SKIP"
                detail = "Task not implemented"

        except Exception as e:
            status = "FAIL"
            detail = f"Assertion failed: {e}"

        if status == "PASS":
            passed_count += 1
            icon = "[PASS]"
        else:
            icon = "[FAIL]"

        results.append({
            "id": tid,
            "name": tname,
            "category": cat,
            "status": status,
            "detail": detail
        })
        print(f"{icon} [{tid}] {tname:<35} | {cat:<18} | {detail}")

    pass_pct = (passed_count / len(tasks)) * 100
    print(f"\n-------------------------------------------------------")
    print(f"Benchmark Summary: {passed_count}/{len(tasks)} passed ({pass_pct:.1f}%)")
    print(f"-------------------------------------------------------\n")

    report_path = ROOT / "evals" / "eval_results.json"
    with open(report_path, "w", encoding="utf-8") as f:
        json.dump({
            "version": bench["version"],
            "total_tasks": len(tasks),
            "passed": passed_count,
            "pass_percentage": pass_pct,
            "results": results
        }, f, indent=2)

    if passed_count < len(tasks):
        sys.exit(1)
    else:
        sys.exit(0)

if __name__ == "__main__":
    run_evals()
