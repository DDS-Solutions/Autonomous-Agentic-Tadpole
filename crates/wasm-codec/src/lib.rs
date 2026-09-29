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

pub const PULSE_MAGIC: [u8; 4] = *b"TADP";
pub const PULSE_VERSION: u16 = 1;

impl SwarmPulse {
    pub fn new(timestamp: f64) -> Self {
        Self {
            timestamp,
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// Pure Rust binary encoder with magic header and version prefix.
    pub fn encode_to_vec(&self) -> Result<Vec<u8>, String> {
        let payload = postcard::to_allocvec(self)
            .map_err(|e| format!("Encoding error: {}", e))?;
        let mut buf = Vec::with_capacity(6 + payload.len());
        buf.extend_from_slice(&PULSE_MAGIC);
        buf.extend_from_slice(&PULSE_VERSION.to_be_bytes());
        buf.extend_from_slice(&payload);
        Ok(buf)
    }

    /// Pure Rust binary decoder validating magic header and version.
    pub fn decode_from_bytes(bytes: &[u8]) -> Result<SwarmPulse, String> {
        if bytes.len() < 6 {
            return Err("Decoding error: payload too short for magic header".to_string());
        }
        if &bytes[0..4] != PULSE_MAGIC {
            return Err("Decoding error: invalid pulse magic bytes".to_string());
        }
        let version = u16::from_be_bytes([bytes[4], bytes[5]]);
        if version != PULSE_VERSION {
            return Err(format!("Decoding error: unsupported pulse version {}", version));
        }
        postcard::from_bytes(&bytes[6..])
            .map_err(|e| format!("Decoding error: {}", e))
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

        // Test magic envelope roundtrip
        let envelope = pulse.encode_to_vec().expect("envelope serialization failed");
        assert!(envelope.starts_with(&PULSE_MAGIC));
        let decoded_envelope = SwarmPulse::decode_from_bytes(&envelope).expect("envelope deserialization failed");
        assert_eq!(pulse, decoded_envelope);
    }

    #[test]
    fn test_pulse_codec_rejection() {
        let short = vec![1, 2, 3];
        assert!(SwarmPulse::decode_from_bytes(&short).is_err());

        let invalid_magic = vec![0, 0, 0, 0, 0, 1, 10, 20];
        assert!(SwarmPulse::decode_from_bytes(&invalid_magic).is_err());

        let mut bad_version = Vec::new();
        bad_version.extend_from_slice(&PULSE_MAGIC);
        bad_version.extend_from_slice(&99u16.to_be_bytes());
        bad_version.extend_from_slice(&[1, 2, 3]);
        assert!(SwarmPulse::decode_from_bytes(&bad_version).is_err());
    }
}

// Metadata: [wasm_codec]

