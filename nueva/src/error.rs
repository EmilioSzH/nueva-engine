//! Error types for Nueva
//!
//! All errors in Nueva use the NuevaError type, providing
//! consistent error handling with recovery paths.

use thiserror::Error;

/// Result type alias using NuevaError
pub type Result<T> = std::result::Result<T, NuevaError>;

/// All possible errors in Nueva
#[derive(Error, Debug)]
pub enum NuevaError {
    // Audio I/O errors
    #[error("Failed to read audio file: {path}")]
    AudioReadError { path: String, #[source] source: hound::Error },

    #[error("Failed to write audio file: {path}")]
    AudioWriteError { path: String, #[source] source: hound::Error },

    #[error("Unsupported audio format: {details}")]
    UnsupportedFormat { details: String },

    // Processing errors
    #[error("Audio buffer is empty")]
    EmptyBuffer,

    #[error("Sample rate mismatch: expected {expected}, got {actual}")]
    SampleRateMismatch { expected: u32, actual: u32 },

    #[error("Channel count mismatch: expected {expected}, got {actual}")]
    ChannelMismatch { expected: u16, actual: u16 },

    // DSP errors
    #[error("Invalid effect parameter: {param} = {value} (valid range: {min}..{max})")]
    InvalidParameter {
        param: String,
        value: f32,
        min: f32,
        max: f32,
    },

    #[error("Effect not found: {id}")]
    EffectNotFound { id: String },

    #[error("DSP chain error: {details}")]
    ChainError { details: String },

    // Layer errors
    #[error("Layer {layer} is locked and cannot be modified")]
    LayerLocked { layer: u8 },

    #[error("No audio data in layer {layer}")]
    LayerEmpty { layer: u8 },

    // State errors
    #[error("Nothing to undo")]
    NothingToUndo,

    #[error("Nothing to redo")]
    NothingToRedo,

    #[error("Project file error: {details}")]
    ProjectError { details: String },

    // Agent errors
    #[error("Could not understand request: {prompt}")]
    IntentParseError { prompt: String },

    #[error("Conflicting goals detected: {goals:?}")]
    ConflictingGoals { goals: Vec<String> },

    // Safety errors
    #[error("Audio would clip: peak level {peak_db:.1} dBFS exceeds 0 dBFS")]
    WouldClip { peak_db: f32 },

    #[error("Phase correlation too low: {correlation:.2} (minimum: 0.2)")]
    PhaseIssue { correlation: f32 },

    #[error("Loudness exceeds limit: {lufs:.1} LUFS (maximum: -5 LUFS)")]
    LoudnessExceeded { lufs: f32 },

    // Generic I/O
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

impl NuevaError {
    /// Returns a suggested recovery action for this error
    pub fn recovery_hint(&self) -> &'static str {
        match self {
            Self::AudioReadError { .. } => "Check that the file exists and is a valid audio file",
            Self::UnsupportedFormat { .. } => "Convert to WAV format (16/24/32-bit, 44.1/48/96 kHz)",
            Self::EmptyBuffer => "Load audio before processing",
            Self::InvalidParameter { .. } => "Adjust the parameter to be within valid range",
            Self::WouldClip { .. } => "Reduce gain or add a limiter",
            Self::PhaseIssue { .. } => "Check for phase-inverted channels or use mono",
            Self::LoudnessExceeded { .. } => "Reduce overall level or add limiting",
            Self::NothingToUndo => "Make some changes first",
            Self::ConflictingGoals { .. } => "Choose one goal to prioritize",
            _ => "Check the error details and try again",
        }
    }
}
