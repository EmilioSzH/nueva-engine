//! Audio buffer and I/O utilities
//!
//! This module provides the core audio data structures and file I/O.

mod buffer;
mod io;
pub mod verification;

pub use buffer::AudioBuffer;
pub use io::{load_wav, save_wav};
pub use verification::AudioAnalysis;
