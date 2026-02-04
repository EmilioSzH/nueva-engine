//! Nueva - Functional Audio Processing System
//!
//! Nueva provides two parallel interfaces for audio manipulation:
//! 1. Traditional DSP Controls - Parameter-based effects (EQ, compression, reverb)
//! 2. AI Agent Interface - Natural language commands invoking AI audio processing
//!
//! # Architecture
//!
//! The system uses a three-layer model:
//! - Layer 0: Immutable source audio (never modified after creation)
//! - Layer 1: AI state buffer (output of neural transformations)
//! - Layer 2: DSP chain (real-time adjustable effects)
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        AI AGENT                              │
//! │                   (LLM-based reasoning)                      │
//! ├─────────────────────────────────────────────────────────────┤
//! │   User Prompt → Intent Analysis → Tool Selection            │
//! │                          │                                   │
//! │            ┌─────────────┼─────────────┐                    │
//! │            ▼             ▼             ▼                    │
//! │       DSP Tool     Neural Tool      Both                    │
//! │      (Layer 2)      (Layer 1)                               │
//! └─────────────────────────────────────────────────────────────┘
//! ```

// Core modules
pub mod engine;
pub mod error;
pub mod layers;

// AI/Agent modules
pub mod agent;
pub mod neural;

// Re-export commonly used types
pub use agent::{Agent, AgentResponse, ToolDecision};
pub use error::{NuevaError, Result};
pub use neural::{NeuralModel, NeuralModelRegistry};
