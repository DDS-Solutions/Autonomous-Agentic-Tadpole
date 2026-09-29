//! @docs ARCHITECTURE:Core
//! 
//! ### AI Assist Note
//! **Core technical module for the Tadpole OS hardened engine.**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[lib]` in tracing logs.

//!   @docs ARCHITECTURE:Performance
//!
//! ### AI Assist Note
//! **Binary Codec**: Implements Postcard-based binary serialization for sub-millisecond
//! telemetry. Bridges the Rust backend with the WASM-powered frontend pulse decoder.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// A single agent node in the visual swarm pulse.
/// 
/// ### 📊 Status Codes
/// - **0**: `Idle` - Agent is awaiting mission assignment.
/// - **1**: `Busy` - Agent is actively executing a mission loop.
/// - **2**: `Error` - Agent has encountered a terminal mission failure.
/// - **3**: `Degraded` - Agent is online but experiencing rate-limiting or sub-optimal performance.
#[wasm_bindgen]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PulseNode {
    #[wasm_bindgen(getter_with_clone)]
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub status: u8, 
    pub battery: u8,
    pub signal: u8,
    pub progress: f32,
}

#[wasm_bindgen]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PulseConnection {
    #[wasm_bindgen(getter_with_clone)]
    pub source: String,
    #[wasm_bindgen(getter_with_clone)]
    pub target: String,
}

#[wasm_bindgen]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SwarmPulse {
    pub timestamp: u64,
    #[wasm_bindgen(getter_with_clone)]
    pub nodes: Vec<PulseNode>,
    #[wasm_bindgen(getter_with_clone)]
    pub edges: Vec<PulseConnection>,
}

pub const PULSE_MAGIC: [u8; 4] = *b"TADP";
pub const PULSE_VERSION: u16 = 1;

impl SwarmPulse {
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

#[wasm_bindgen]
impl SwarmPulse {
    #[wasm_bindgen(constructor)]
    pub fn new(timestamp: u64) -> Self {
        Self {
            timestamp,
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// Encodes the pulse into a binary Postcard buffer with magic header and version prefix.
    /// 
    /// ### ⚡ Efficiency Requirements
    /// Utilizes a variable-length binary encoding designed for 
    /// sub-millisecond serialization. Essential for maintaining a 60FPS 
    /// swarm visualization without UI stutter (PERF-01).
    pub fn encode(&self) -> Result<Vec<u8>, JsValue> {
        self.encode_to_vec()
            .map_err(|e| JsValue::from_str(&e))
    }

    /// Decodes a binary pulse back into a SwarmPulse structure, validating header and version.
    pub fn decode(bytes: &[u8]) -> Result<SwarmPulse, JsValue> {
        Self::decode_from_bytes(bytes)
            .map_err(|e| JsValue::from_str(&e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pulse_codec_parity() {
        let mut pulse = SwarmPulse::new(123456789);
        pulse.nodes.push(PulseNode {
            id: "agent-1".to_string(),
            x: 10.0,
            y: 20.0,
            status: 1,
            battery: 85,
            signal: 90,
            progress: 0.5,
        });
        pulse.edges.push(PulseConnection {
            source: "agent-1".to_string(),
            target: "agent-2".to_string(),
        });

        let encoded = pulse.encode_to_vec().expect("Failed to encode");
        assert!(encoded.starts_with(&PULSE_MAGIC));
        assert_eq!(&encoded[4..6], &1u16.to_be_bytes());

        let decoded = SwarmPulse::decode_from_bytes(&encoded).expect("Failed to decode");

        assert_eq!(decoded.timestamp, pulse.timestamp);
        assert_eq!(decoded.nodes.len(), 1);
        assert_eq!(decoded.nodes[0].id, "agent-1");
        assert_eq!(decoded.nodes[0].status, 1);
        assert_eq!(decoded.edges.len(), 1);
        assert_eq!(decoded.edges[0].source, "agent-1");
        assert_eq!(decoded.edges[0].target, "agent-2");
    }

    #[test]
    fn test_pulse_codec_rejection() {
        // Payload too short
        let short = vec![1, 2, 3];
        assert!(SwarmPulse::decode_from_bytes(&short).is_err());

        // Invalid magic
        let invalid_magic = vec![0, 0, 0, 0, 0, 1, 10, 20];
        assert!(SwarmPulse::decode_from_bytes(&invalid_magic).is_err());

        // Unsupported version
        let mut bad_version = Vec::new();
        bad_version.extend_from_slice(&PULSE_MAGIC);
        bad_version.extend_from_slice(&99u16.to_be_bytes());
        bad_version.extend_from_slice(&[1, 2, 3]);
        assert!(SwarmPulse::decode_from_bytes(&bad_version).is_err());
    }
}





// Metadata: [lib]
