//! Nueva - Functional Audio Processing System
//!
//! Nueva provides two parallel interfaces for audio processing:
//! 1. Traditional DSP Controls: Parameter-based effects (EQ, compression, reverb)
//! 2. AI Agent Interface: Natural language commands invoking AI audio processing
//!
//! # Architecture
//!
//! The system uses a three-layer model:
//! - Layer 0: Immutable source storage (original audio)
//! - Layer 1: AI state buffer (neural processing results)
//! - Layer 2: DSP chain (real-time effects)

pub mod engine;
pub mod layers;
pub mod dsp;
pub mod agent;
pub mod neural;
pub mod state;
pub mod error;
pub mod audio;

// Re-export commonly used types
pub use error::{NuevaError, Result};
pub use audio::AudioBuffer;
