//! Neural model interfaces and implementations
//!
//! This module provides:
//! - `NeuralModel` trait for all neural processors
//! - Model registry with metadata
//! - Context tracking for intentional artifacts
//! - Mock implementations for testing
//! - Real ACE-Step 1.5 integration via Python bridge

mod ace_step;
mod context;
mod mock;
mod model;
mod registry;

pub use ace_step::{AceStep, AceStepMode};
pub use context::NeuralContextTracker;
pub use mock::*;
pub use model::{NeuralModel, NeuralModelInfo, NeuralModelParams, ProcessingResult};
pub use registry::NeuralModelRegistry;
