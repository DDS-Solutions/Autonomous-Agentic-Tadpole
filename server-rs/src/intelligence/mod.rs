//! @docs ARCHITECTURE:Intelligence
//!
//! ### AI Assist Note
//! **Intelligence Layer**: Orchestrates the codebase awareness and 
//! autonomous reasoning capabilities. Features the **Knowledge Graph**: 
//! a symbol-level directed graph that maps cross-file dependencies 
//! and provides the "Blast Radius" for agent-led modifications.
//! Implements **Semantic RAG**: uses the graph topology to prioritize 
//! relevant code context during tool execution (INTEL-01).

pub mod graph;

// Metadata: [mod]
