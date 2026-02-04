//! DSP Effects Library
//!
//! Real-time audio effects for Layer 2 processing.
//! Implementation: wt-dsp worktree
//!
//! Effects to implement:
//! - EQ (parametric, shelf, HP/LP filters)
//! - Dynamics (compressor, limiter, gate)
//! - Time-based (delay, reverb)
//! - Utility (gain, pan, stereo tools)

pub mod chain;
pub mod effects;

pub use chain::DspChain;
pub use effects::Effect;
