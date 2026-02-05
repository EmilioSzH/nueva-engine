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

pub mod dsp;
pub mod engine;
pub mod error;
pub mod layers;

// These modules are placeholders for other worktrees
// pub mod agent;
// pub mod neural;
// pub mod state;
// pub mod cli;

pub use error::{NuevaError, Result};
