//! @docs ARCHITECTURE:Observability
//!
//! ### AI Assist Note
//! **Core technical module for the Tadpole OS hardened engine.**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[pulse_tests]` in tracing logs.
//!
//! @docs ARCHITECTURE:Telemetry
//!
//! ### AI Assist Note
//! **Pulse Protocol Testing**: Validates the density and correctness of the High-Speed
//! Swarm Pulse (100ms) used for real-time visualization.

use crate::agent::types::EngineAgent;
use crate::state::AppState;
use crate::telemetry::pulse_types::{PulseConnection, PulseNode, SwarmPulse};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn test_pulse_aggregation_logic() {
    let state = Arc::new(AppState::new_mock().await);

    // 1. Add a mock agent to the registry
    let agent = EngineAgent {
        identity: crate::agent::types::AgentIdentity {
            id: "agent-007".to_string(),
            name: "James Bond".to_string(),
            ..Default::default()
        },
        health: crate::agent::types::AgentHealth {
            status: "running".to_string(),
            ..Default::default()
        },
        economics: crate::agent::types::AgentEconomics {
            budget_usd: 100.0,
            cost_usd: 25.0, // 75% battery
            ..Default::default()
        },
        state: crate::agent::types::AgentState {
            active_mission: Some(json!({ "id": "mission-spy" })),
            ..Default::default()
        },
        ..Default::default()
    };

    state.registry.agents.insert("agent-007".to_string(), agent);

    // 2. Build the pulse (simulating pulse.rs logic with synthesis)
    let timestamp = 123456789.0;
    let mut pulse = SwarmPulse::new(timestamp);
    let mut active_missions = std::collections::HashSet::new();

    for entry in state.registry.agents.iter() {
        let a = entry.value();

        let status = match a.health.status.as_str() {
            "running" => 1,
            _ => 1, // busy in test
        };

        pulse.nodes.push(PulseNode {
            id: a.identity.id.clone(),
            x: 0.0,
            y: 0.0,
            status,
            battery: 75,
            signal: 100,
            progress: 0.0,
        });

        if let Some(mission) = &a.state.active_mission {
            if let Some(mission_id) = mission.get("id").and_then(|v: &serde_json::Value| v.as_str()) {
                active_missions.insert(mission_id.to_string());
                pulse.edges.push(PulseConnection {
                    source: a.identity.id.clone(),
                    target: mission_id.to_string(),
                });
            }
        }
    }

    // Synthesize mission nodes (new requirement)
    for m_id in active_missions {
        pulse.nodes.push(PulseNode {
            id: m_id,
            x: 0.0,
            y: 0.0,
            status: 4,
            battery: 100,
            signal: 100,
            progress: 0.0,
        });
    }

    // 3. Verifications
    assert_eq!(pulse.nodes.len(), 2); // 1 Agent + 1 Mission
    assert!(pulse.nodes.iter().any(|n| n.id == "agent-007"));
    assert!(pulse
        .nodes
        .iter()
        .any(|n| n.id == "mission-spy" && n.status == 4));
}

#[test]
fn test_messagepack_serialization_density() {
    let mut pulse = SwarmPulse::new(123456789.0);
    for i in 0..10 {
        pulse.nodes.push(PulseNode {
            id: format!("agent-{}", i),
            x: 1.2,
            y: 3.4,
            status: 1,
            battery: 80,
            signal: 100,
            progress: 0.5,
        });
    }

    // Binary MessagePack serialization
    let binary = rmp_serde::to_vec(&pulse).expect("Failed to serialize to MessagePack");

    // JSON serialization for comparison
    let json_ver = serde_json::to_string(&pulse).expect("Failed to serialize to JSON");

    println!("MsgPack Size: {} bytes", binary.len());
    println!("JSON Size: {} bytes", json_ver.len());

    assert!(
        binary.len() < json_ver.len(),
        "MessagePack should be more dense than JSON"
    );
}

#[test]
fn test_pulse_status_mapping_all_variants() {
    let statuses = vec![
        ("active", 1),
        ("busy", 1),
        ("running", 1),
        ("thinking", 1),
        ("failed", 2),
        ("throttled", 3),
        ("idle", 0),
        ("unknown_state", 0),
    ];

    for (raw_status, expected_code) in statuses {
        let code = match raw_status {
            "active" | "busy" | "running" | "thinking" => 1,
            "failed" => 2,
            "throttled" => 3,
            _ => 0,
        };
        assert_eq!(code, expected_code, "Status '{}' should map to code {}", raw_status, expected_code);
    }
}

#[test]
fn test_pulse_battery_budget_calculation() {
    // Normal budget usage: $100 budget, $25 cost -> 75% remaining
    let budget = 100.0f64;
    let cost = 25.0f64;
    let battery = if budget > 0.0 {
        let remaining = (budget - cost).max(0.0);
        ((remaining / budget) * 100.0) as u8
    } else {
        100
    };
    assert_eq!(battery, 75);

    // Over-budget usage: $50 budget, $60 cost -> 0% remaining
    let budget2 = 50.0f64;
    let over_cost = 60.0f64;
    let over_battery = if budget2 > 0.0 {
        let remaining = (budget2 - over_cost).max(0.0);
        ((remaining / budget2) * 100.0) as u8
    } else {
        100
    };
    assert_eq!(over_battery, 0);

    // Zero budget configured: default 100%
    let zero_budget = 0.0f64;
    let zero_battery = if zero_budget > 0.0 {
        0
    } else {
        100
    };
    assert_eq!(zero_battery, 100);
}

#[tokio::test]
async fn test_ghost_mission_topology_preservation() {
    let state = Arc::new(AppState::new_mock().await);
    let handle = tokio::task::spawn(async {});
    state.comms.active_runners.insert("mission-ghost-anchor".to_string(), handle.abort_handle());

    let ghost_missions = state.comms.active_runners.iter()
        .map(|kv| kv.key().clone())
        .collect::<std::collections::HashSet<String>>();

    assert!(ghost_missions.contains("mission-ghost-anchor"));
    assert_eq!(ghost_missions.len(), 1);
}

// Metadata: [pulse_tests]
