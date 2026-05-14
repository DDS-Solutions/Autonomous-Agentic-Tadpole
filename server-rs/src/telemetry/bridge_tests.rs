use std::sync::Arc;
use crate::state::AppState;
use crate::telemetry::TELEMETRY_TX;

#[tokio::test]
async fn test_telemetry_bridge_persistence() {
    let state = Arc::new(AppState::new_mock().await);
    let pool = &state.resources.pool;

    // 1. Setup agent and mission
    sqlx::query("INSERT INTO agents (id, name, role, department, description, status, metadata) VALUES ('bridge-test-agent', 'Test Agent', 'Tester', 'QA', 'Testing bridge', 'idle', '{}')").execute(pool).await.unwrap();
    
    let mission = crate::agent::mission::create_mission(pool, "bridge-test-agent", "Bridge Test Mission", 1.0).await.unwrap();
    let mission_id = mission.id.clone();

    // 2. Start bridge
    let bridge_rx_telemetry = TELEMETRY_TX.subscribe();
    let bridge_rx_logs = state.comms.tx.subscribe();
    let bridge = crate::telemetry::bridge::TelemetryBridge::new(state.clone());
    tokio::spawn(bridge.run(bridge_rx_telemetry, bridge_rx_logs));

    // 3. Emit a telemetry event
    let event = serde_json::json!({
        "type": "trace:span",
        "span": {
            "id": "test-span-id",
            "name": "think",
            "mission_id": mission_id,
            "agent_id": "bridge-test-agent",
            "start_time": 123456789,
            "attributes": {
                "thought": "I am testing the telemetry bridge persistence."
            }
        }
    });
    TELEMETRY_TX.send(event).unwrap();

    // 4. Wait for bridge to process
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // 5. Verify log persistence
    let logs = crate::agent::mission::get_mission_logs(pool, &mission_id).await.unwrap();
    
    assert!(!logs.is_empty(), "Logs should not be empty after bridge processing");
    let found = logs.iter().any(|l| l.text.contains("testing the telemetry bridge"));
    assert!(found, "Expected log message not found in database");
}
