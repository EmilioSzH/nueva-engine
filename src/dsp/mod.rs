//! DSP Effects Library (spec §4)
//!
//! Provides traditional parameter-based audio effects:
//! - Gain
//! - Parametric EQ (with shelf and filter types)
//! - Compressor
//! - Gate
//! - Limiter
//! - Reverb
//! - Delay
//! - Saturation

mod audio_buffer;
mod effect;

// Effect implementations
mod compressor;
mod delay;
mod eq;
mod gain;
mod gate;
mod limiter;
mod reverb;
mod saturation;

// Effect chain
mod chain;

// Re-exports
pub use audio_buffer::AudioBuffer;
pub use chain::{EffectChain, EffectPosition};
pub use effect::{Effect, EffectMetadata, ProcessResult};

// Individual effects
pub use compressor::Compressor;
pub use delay::Delay;
pub use eq::{EQBand, FilterType, ParametricEQ};
pub use gain::GainEffect;
pub use gate::Gate;
pub use limiter::Limiter;
pub use reverb::{Reverb, ReverbParams};
pub use saturation::{Saturation, SaturationType};
