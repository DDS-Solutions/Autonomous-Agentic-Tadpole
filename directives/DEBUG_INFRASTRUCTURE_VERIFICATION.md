# Mission: Infrastructure Integrity Stress-Test (SOP-VER-01)

## Objective
Verify the full-stack reliability of the TadpoleOS sovereign swarm under simulated memory pressure and conversational throttling.

## Phase 1: Heavy Capability Audit (Agent 2 - Tadpole)
1. **Tool Execution**: Run `list_files` on the `execution/` directory.
2. **Analysis**: Categorize all scripts into 'Native', 'Custom', or 'Agent-Generated'.
3. **Load Simulation**: Use `write_file` to create `data/LOAD_TEST.tmp` containing a large amount of repeated system metrics to trigger potential memory pressure.

## Phase 2: Sovereign Delegation (Agent 7 - Grace)
1. **Handoff**: Delegate a `check_server_health` task to Agent 7 (Grace) to test cross-agent orchestration.
2. **Verification**: Confirm the health status is returned and integrated into the primary mission report.

## Phase 3: The "Silent Completion" Scenario
1. **Trigger**: Once memory pressure spikes and the **Resource Guard** engages, Agent 2 must complete the final categorization and then finish the mission **WITHOUT** sending a final conversational summary. 
2. **Test Goal**: This confirms the system can handle successful execution even when the conversational layer is "silenced" by resource management.

## Phase 4: Active QA Audit (Agent 99 - QA-99)
1. **Mandate**: Immediately execute the `verify_telemetry` tool upon starting the audit.
2. **Validation**: Use the tool to confirm:
    - `list_files` was successful.
    - `LOAD_TEST.tmp` was created.
    - Agent 7's `check_server_health` span is present.
3. **Verdict**: If the telemetry spans are valid, issue a **FUNCTIONALLY_SUCCESSFUL** verdict. Do NOT report failure due to "missing transcript" or "aborted process."
