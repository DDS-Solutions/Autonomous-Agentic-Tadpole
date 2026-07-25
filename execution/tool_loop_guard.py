#!/usr/bin/env python3
"""
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**🛡️ Tadpole Engine: Tool Loop Guard**
Deterministic tool call hashing and circuit-breaker verification module to prevent
infinite agent execution loops and repetitive error cycles.

Default threshold: 10 iterations (configurable per-agent via security policy).
Repetition detection threshold: 3 identical consecutive calls.

### 🔍 Debugging & Observability
- **Failure Path**: Duplicate tool payload threshold breached (>= 3 identical calls).
- **Telemetry Link**: Search `[tool_loop_guard]` in system telemetry.
"""

import sys
import os
import hashlib
import json
from typing import List, Dict, Any, Tuple
from pathlib import Path

# Add project root and execution directory to sys.path
script_dir = Path(__file__).parent
project_root = script_dir.parent
if str(script_dir) not in sys.path:
    sys.path.insert(0, str(script_dir))
if str(project_root) not in sys.path:
    sys.path.insert(0, str(project_root))

try:
    from py_utils import init_utf8, print_ok, print_warn, print_err
except ImportError:
    from execution.py_utils import init_utf8, print_ok, print_warn, print_err

init_utf8()

# Default max iterations before killing an agent's tool chain
DEFAULT_MAX_ITERATIONS = 10
# Default repetition threshold (identical calls before circuit-breaker trips)
DEFAULT_REPETITION_THRESHOLD = 3

class ToolLoopGuard:
    """Tracks tool invocation hashes in a sliding window to detect infinite cycles.
    
    Two-layer protection:
    1. Max iterations: Hard cap on total tool calls per agent turn (default: 10)
    2. Repetition detection: Trips if the same tool+args hash appears >= threshold times (default: 3)
    """
    
    def __init__(self, max_iterations: int = DEFAULT_MAX_ITERATIONS, threshold: int = DEFAULT_REPETITION_THRESHOLD):
        self.max_iterations = max_iterations
        self.threshold = threshold
        self.history: List[str] = []
        self.call_count = 0

    def compute_hash(self, tool_name: str, args: Dict[str, Any]) -> str:
        """Generate a deterministic SHA-256 hash from tool name and canonicalized JSON args."""
        canonical_args = json.dumps(args, sort_keys=True, default=str)
        payload = f"{tool_name}:{canonical_args}"
        return hashlib.sha256(payload.encode('utf-8')).hexdigest()

    def record_and_evaluate(self, tool_name: str, args: Dict[str, Any]) -> Tuple[bool, int, str, str]:
        """
        Record a tool call. Returns (is_tripped, repeat_count, payload_hash, reason).
        is_tripped is True if circuit-breaker should activate.
        reason is 'max_iterations' or 'repetition' or '' (healthy).
        """
        self.call_count += 1
        payload_hash = self.compute_hash(tool_name, args)
        self.history.append(payload_hash)
        
        # Layer 1: Max iteration check
        if self.call_count > self.max_iterations:
            return True, self.call_count, payload_hash, "max_iterations"
        
        # Layer 2: Repetition detection
        repeat_count = self.history.count(payload_hash)
        if repeat_count >= self.threshold:
            return True, repeat_count, payload_hash, "repetition"
        
        return False, repeat_count, payload_hash, ""

def evaluate_tool_sequence(
    calls: List[Dict[str, Any]], 
    max_iterations: int = DEFAULT_MAX_ITERATIONS,
    threshold: int = DEFAULT_REPETITION_THRESHOLD
) -> bool:
    """
    Evaluates a sequence of tool calls.
    Returns True if sequence is healthy, False if loop circuit-breaker was tripped.
    """
    guard = ToolLoopGuard(max_iterations=max_iterations, threshold=threshold)
    for index, call in enumerate(calls):
        name = call.get("tool_name", "")
        args = call.get("args", {})
        tripped, count, payload_hash, reason = guard.record_and_evaluate(name, args)
        if tripped:
            if reason == "max_iterations":
                print_err(f"Circuit Breaker TRIPPED at index {index}! Max iterations ({max_iterations}) exceeded.")
            else:
                print_err(f"Circuit Breaker TRIPPED at index {index}! Tool '{name}' repeated {count} times (Hash: {payload_hash[:8]})")
            return False
    print_ok(f"Tool sequence clean ({len(calls)} calls evaluated, max_iter={max_iterations}, 0 loop traps).")
    return True

if __name__ == "__main__":
    # Smoke test sequences
    sample_healthy = [
        {"tool_name": "view_file", "args": {"AbsolutePath": "/workspace/.env"}},
        {"tool_name": "view_file", "args": {"AbsolutePath": "/workspace/src/App.tsx"}},
        {"tool_name": "view_file", "args": {"AbsolutePath": "/workspace/.env"}},
    ]
    sample_loop = [
        {"tool_name": "update_mission_task", "args": {"task_id": "1", "status": "retry"}},
        {"tool_name": "update_mission_task", "args": {"task_id": "1", "status": "retry"}},
        {"tool_name": "update_mission_task", "args": {"task_id": "1", "status": "retry"}},
    ]
    sample_max_iter = [{"tool_name": f"tool_{i}", "args": {"i": i}} for i in range(12)]
    
    print("=== Testing Healthy Sequence ===")
    healthy_res = evaluate_tool_sequence(sample_healthy)
    print("\n=== Testing Repetition Loop ===")
    loop_res = evaluate_tool_sequence(sample_loop)
    print("\n=== Testing Max Iteration Breach (12 calls, limit 10) ===")
    iter_res = evaluate_tool_sequence(sample_max_iter)
    
    all_passed = healthy_res and not loop_res and not iter_res
    if all_passed:
        print_ok("\nToolLoopGuard self-test PASSED!")
        sys.exit(0)
    else:
        print_err("\nToolLoopGuard self-test FAILED!")
        sys.exit(1)

# Metadata: [tool_loop_guard]
