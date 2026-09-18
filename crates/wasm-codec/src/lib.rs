//! @docs ARCHITECTURE:Core
//!
//! ### AI Assist Note
//! **wasm_codec**: Clean-room high-performance binary codec utilizing Postcard serialization
//! for high-density Swarm Pulse frames over WebSockets / IPC.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Postcard serialization or deserialization error, buffer underflow.
//! - **Telemetry Link**: Search `[wasm_codec]` in browser logs.
//!
//! Copyright (c) 2026 DDS Solutions / Tadpole OS Authors
//! SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PulseNode {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub status: u8, // 0: idle, 1: busy, 2: error, 3: degraded
    pub battery: u8,
    pub signal: u8,
    pub progress: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PulseConnection {
    pub source: String,
    pub target: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SwarmPulse {
    pub timestamp: f64,
    pub nodes: Vec<PulseNode>,
    pub edges: Vec<PulseConnection>,
}

impl SwarmPulse {
    pub fn new(timestamp: f64) -> Self {
        Self {
            timestamp,
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}

/// Encode a `SwarmPulse` into a compact postcard binary payload.
pub fn encode_pulse(pulse: &SwarmPulse) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_allocvec(pulse)
}

/// Decode a compact postcard binary payload into a `SwarmPulse`.
pub fn decode_pulse(bytes: &[u8]) -> Result<SwarmPulse, postcard::Error> {
    postcard::from_bytes(bytes)
}

// ── WASM Bindings for Web Browser / Frontend ──────────────────

#[wasm_bindgen]
pub fn wasm_encode_pulse(val: JsValue) -> Result<Vec<u8>, JsValue> {
    let pulse: SwarmPulse = serde_wasm_bindgen::from_value(val)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse pulse object: {e}")))?;
    encode_pulse(&pulse)
        .map_err(|e| JsValue::from_str(&format!("Failed to encode pulse with postcard: {e}")))
}

#[wasm_bindgen]
pub fn wasm_decode_pulse(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let pulse = decode_pulse(bytes)
        .map_err(|e| JsValue::from_str(&format!("Failed to decode pulse with postcard: {e}")))?;
    serde_wasm_bindgen::to_value(&pulse)
        .map_err(|e| JsValue::from_str(&format!("Failed to convert pulse to JsValue: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swarm_pulse_roundtrip() {
        let mut pulse = SwarmPulse::new(1741200000.0);
        pulse.nodes.push(PulseNode {
            id: "agent-007".into(),
            x: 120.5,
            y: 84.2,
            status: 1,
            battery: 95,
            signal: 88,
            progress: 0.75,
        });
        pulse.nodes.push(PulseNode {
            id: "agent-009".into(),
            x: 300.0,
            y: 450.0,
            status: 0,
            battery: 100,
            signal: 99,
            progress: 1.0,
        });
        pulse.edges.push(PulseConnection {
            source: "agent-007".into(),
            target: "agent-009".into(),
        });

        let encoded = encode_pulse(&pulse).expect("serialization failed");
        assert!(!encoded.is_empty());

        let decoded = decode_pulse(&encoded).expect("deserialization failed");
        assert_eq!(pulse, decoded);
    }
}

// Metadata: [wasm_codec]

