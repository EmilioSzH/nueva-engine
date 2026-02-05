//! Neural model interfaces and implementations
//!
//! This module provides:
//! - `NeuralModel` trait for all neural processors
//! - Model registry with metadata
//! - Context tracking for intentional artifacts
//! - Mock implementations for testing
//! - GPU detection for hardware capability checks
//! - ACE-Step integration (when `acestep` feature is enabled)

mod context;
mod gpu;
mod mock;
mod model;
mod registry;

#[cfg(feature = "acestep")]
mod acestep;

pub use context::{IntentionalArtifact, NeuralContextTracker, NeuralOperation};
pub use gpu::{can_run_ace_step, gpu_status_summary, GpuInfo, QuantizationLevel};
pub use mock::*;
pub use model::{NeuralModel, NeuralModelInfo, NeuralModelParams, ParamSpec, ParamType, ProcessingResult};
pub use registry::NeuralModelRegistry;

#[cfg(feature = "acestep")]
pub use acestep::{AceStepMode, AceStepModel};
