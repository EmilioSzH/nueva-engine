//! Neural model interfaces and implementations
//!
//! This module provides:
//! - `NeuralModel` trait for all neural processors
//! - Model registry with metadata
//! - Context tracking for intentional artifacts
//! - Mock implementations for testing

mod context;
mod mock;
mod model;
mod registry;

pub use context::NeuralContextTracker;
pub use mock::*;
pub use model::{NeuralModel, NeuralModelInfo, NeuralModelParams, ProcessingResult};
pub use registry::NeuralModelRegistry;
