//! Audio Engine Module
//!
//! Core audio processing engine including:
//! - Audio buffer management
//! - Transport state machine
//! - File I/O operations

pub mod buffer;
pub mod io;
pub mod transport;

pub use buffer::{AudioBuffer, AudioValidation, ChannelLayout};
pub use io::{
    export_audio, generate_stereo_test_tone, generate_test_tone, import_audio, ExportFormat,
};
pub use transport::{TransportManager, TransportState};
