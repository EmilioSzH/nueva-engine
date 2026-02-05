//! DSP Effects Library
//!
//! Traditional signal processing effects for Layer 2.
//! All effects implement the `Effect` trait for uniform processing.

mod chain;
mod compressor;
mod delay;
mod effect;
mod eq;
mod gain;
mod gate;
mod limiter;
mod reverb;
mod saturation;

pub use chain::{ChainPosition, EffectChain};
pub use compressor::Compressor;
pub use delay::Delay;
pub use effect::{Effect, EffectParams};
pub use eq::{EQBand, FilterType, ParametricEQ};
pub use gain::Gain;
pub use gate::Gate;
pub use limiter::Limiter;
pub use reverb::Reverb;
pub use saturation::{Saturation, SaturationType};

/// Default order priority for effect types (lower = earlier in chain)
/// Per spec §4.3: Gate → EQ → Compression → Saturation → Delay → Reverb → Limiter
pub fn get_default_order_priority(effect_type: &str) -> u32 {
    match effect_type {
        "gate" => 0,
        "eq" | "parametric_eq" => 1,
        "compressor" => 2,
        "saturation" => 3,
        "gain" => 4,
        "delay" => 5,
        "reverb" => 6,
        "limiter" => 7,
        _ => 4, // Default to middle
    }
}
