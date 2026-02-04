//! Nueva - Functional Audio Processing with AI Agent Interface
//!
//! This crate provides:
//! - Neural model interfaces and mock implementations
//! - AI agent decision logic
//! - DSP/Neural tool routing
//!
//! # Architecture
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

pub mod agent;
pub mod error;
pub mod neural;

// Re-export commonly used types
pub use agent::{Agent, AgentResponse, ToolDecision};
pub use error::{NuevaError, Result};
pub use neural::{NeuralModel, NeuralModelRegistry};
