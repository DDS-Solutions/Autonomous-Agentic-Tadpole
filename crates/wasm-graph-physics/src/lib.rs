//! @docs ARCHITECTURE:Core
//!
//! ### AI Context Alignment
//! - **Subsystem**: Sovereign Engine / WASM Graph Physics / Experimental O(N^2) Prototype (Unwired)
//! - **Primary Entrypoints**: `GraphPhysicsSimulator`, `WasmGraphSimulator`
//!
//! ### ⚠️ Invariants & Non-Negotiables
//! - `[Status]` Prototype currently uses direct all-pairs O(N^2) repulsion; replace with a quadtree before wiring 20,000+ node graphs.
//! - `[Numerical Stability]` Finite vector clamping: zero distance never propagates NaN/Inf.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[wasm_graph_physics]` in console logs.

pub const TELEMETRY_TAG: &str = "[wasm_graph_physics]";

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicsNode {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub mass: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicsEdge {
    pub source: usize,
    pub target: usize,
    pub length: f32,
    pub stiffness: f32,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GraphPhysicsSimulator {
    pub nodes: Vec<PhysicsNode>,
    pub edges: Vec<PhysicsEdge>,
}

impl GraphPhysicsSimulator {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn add_node(&mut self, id: String, x: f32, y: f32) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(PhysicsNode {
            id,
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            mass: 1.0,
        });
        idx
    }

    pub fn add_edge(&mut self, source: usize, target: usize, length: f32, stiffness: f32) {
        if source < self.nodes.len() && target < self.nodes.len() {
            self.edges.push(PhysicsEdge {
                source,
                target,
                length,
                stiffness,
            });
        }
    }

    /// Advances the simulation by `dt` seconds using Velocity Verlet integration.
    pub fn step(&mut self, dt: f32, repulsion_k: f32, damping: f32) {
        let n = self.nodes.len();
        if n == 0 {
            return;
        }

        let mut fx = vec![0.0f32; n];
        let mut fy = vec![0.0f32; n];

        // 1. Repulsion forces (Coulomb's Law with softening parameter epsilon)
        let epsilon = 1.0f32;
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = self.nodes[j].x - self.nodes[i].x;
                let dy = self.nodes[j].y - self.nodes[i].y;
                let dist_sq = dx * dx + dy * dy + epsilon;
                let dist = dist_sq.sqrt();

                // F = k / r^2
                let force = repulsion_k / dist_sq;
                let nx = dx / dist;
                let ny = dy / dist;

                fx[i] -= force * nx;
                fy[i] -= force * ny;
                fx[j] += force * nx;
                fy[j] += force * ny;
            }
        }

        // 2. Attraction forces along edges (Hooke's Law: F = -k * (dist - length))
        for edge in &self.edges {
            let i = edge.source;
            let j = edge.target;
            let dx = self.nodes[j].x - self.nodes[i].x;
            let dy = self.nodes[j].y - self.nodes[i].y;
            let dist = (dx * dx + dy * dy + 0.001f32).sqrt();

            let displacement = dist - edge.length;
            let force = edge.stiffness * displacement;
            let nx = dx / dist;
            let ny = dy / dist;

            fx[i] += force * nx;
            fy[i] += force * ny;
            fx[j] -= force * nx;
            fy[j] -= force * ny;
        }

        // 3. Integration & velocity damping
        for i in 0..n {
            let node = &mut self.nodes[i];
            node.vx = (node.vx + (fx[i] / node.mass) * dt) * damping;
            node.vy = (node.vy + (fy[i] / node.mass) * dt) * damping;

            node.x += node.vx * dt;
            node.y += node.vy * dt;

            // Defensive clamping: prevent non-finite coordinate corruption
            if !node.x.is_finite() { node.x = 0.0; node.vx = 0.0; }
            if !node.y.is_finite() { node.y = 0.0; node.vy = 0.0; }
        }
    }

    /// Returns a flat vector of [x0, y0, x1, y1, ...] for zero-copy WebGL/Canvas buffer transfer.
    pub fn get_flat_positions(&self) -> Vec<f32> {
        let mut flat = Vec::with_capacity(self.nodes.len() * 2);
        for node in &self.nodes {
            flat.push(node.x);
            flat.push(node.y);
        }
        flat
    }
}

/// WASM exported simulator handle for JavaScript Web Workers.
#[wasm_bindgen]
pub struct WasmGraphSimulator {
    sim: GraphPhysicsSimulator,
}

#[wasm_bindgen]
impl WasmGraphSimulator {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            sim: GraphPhysicsSimulator::new(),
        }
    }

    pub fn add_node(&mut self, id: String, x: f32, y: f32) -> usize {
        self.sim.add_node(id, x, y)
    }

    pub fn add_edge(&mut self, source: usize, target: usize, length: f32, stiffness: f32) {
        self.sim.add_edge(source, target, length, stiffness);
    }

    pub fn tick(&mut self, dt: f32, repulsion: f32, damping: f32) {
        self.sim.step(dt, repulsion, damping);
    }

    pub fn get_positions(&self) -> Vec<f32> {
        self.sim.get_flat_positions()
    }

    pub fn node_count(&self) -> usize {
        self.sim.nodes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_two_nodes_repulsion() {
        let mut sim = GraphPhysicsSimulator::new();
        sim.add_node("A".to_string(), 0.0, 0.0);
        sim.add_node("B".to_string(), 1.0, 0.0);

        // Step simulation with repulsion
        sim.step(0.1, 500.0, 0.9);

        // Nodes must push apart along X axis
        assert!(sim.nodes[0].x < 0.0);
        assert!(sim.nodes[1].x > 1.0);
        assert!(sim.nodes[0].x.is_finite());
        assert!(sim.nodes[1].x.is_finite());
    }

    #[test]
    fn test_spring_attraction() {
        let mut sim = GraphPhysicsSimulator::new();
        let a = sim.add_node("A".to_string(), 0.0, 0.0);
        let b = sim.add_node("B".to_string(), 100.0, 0.0);
        sim.add_edge(a, b, 20.0, 1.0); // Rest length 20, currently at 100

        sim.step(0.1, 0.0, 0.9); // No repulsion, only spring attraction

        // Nodes must pull toward each other
        assert!(sim.nodes[0].x > 0.0);
        assert!(sim.nodes[1].x < 100.0);
    }
}
