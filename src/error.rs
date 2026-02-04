//! Error handling for Nueva
//!
//! All errors include recovery suggestions per spec §9.

use thiserror::Error;

/// Result type alias for Nueva operations
pub type Result<T> = std::result::Result<T, NuevaError>;

/// Main error type for Nueva operations
#[derive(Error, Debug)]
pub enum NuevaError {
    // File Errors
    #[error("File not found: {path}")]
    FileNotFound {
        path: String,
        #[source]
        source: Option<std::io::Error>,
    },

    #[error("Invalid audio file: {reason}")]
    InvalidAudio {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Unsupported audio format: {format}")]
    UnsupportedFormat { format: String },

    // Audio Validation Errors
    #[error("Audio too short: {duration_secs:.3}s (minimum 0.1s)")]
    AudioTooShort { duration_secs: f64 },

    #[error("Audio too long: {duration_secs:.1}s (maximum 2 hours)")]
    AudioTooLong { duration_secs: f64 },

    #[error("Audio contains no samples")]
    EmptyAudio,

    // Processing Errors
    #[error("Processing error: {reason}")]
    ProcessingError { reason: String },

    #[error("DSP overflow: effect produced invalid audio (NaN/Inf)")]
    DspOverflow { effect_id: String },

    #[error("AI processing error: {reason}")]
    AiProcessingError { reason: String },

    #[error("Model not found: {model}")]
    ModelNotFound { model: String },

    // Resource Errors
    #[error("Out of memory: {details}")]
    OutOfMemory { details: String },

    #[error("Disk full: cannot write to {path}")]
    DiskFull { path: String },

    // Agent Errors
    #[error("Ambiguous prompt: {question}")]
    AmbiguousPrompt { question: String },

    #[error("Conflicting request: {conflict}")]
    ConflictingRequest { conflict: String },

    // Layer Errors
    #[error("Layer operation failed: {reason}")]
    LayerError { reason: String },

    #[error("Bake operation failed: {reason}")]
    BakeError { reason: String },

    // I/O Errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // Serialization Errors
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl NuevaError {
    /// Get the error code for this error type
    pub fn error_code(&self) -> &'static str {
        match self {
            NuevaError::FileNotFound { .. } => "FILE_NOT_FOUND",
            NuevaError::InvalidAudio { .. } => "INVALID_AUDIO",
            NuevaError::UnsupportedFormat { .. } => "UNSUPPORTED_FORMAT",
            NuevaError::AudioTooShort { .. } => "AUDIO_TOO_SHORT",
            NuevaError::AudioTooLong { .. } => "AUDIO_TOO_LONG",
            NuevaError::EmptyAudio => "EMPTY_AUDIO",
            NuevaError::ProcessingError { .. } => "PROCESSING_ERROR",
            NuevaError::DspOverflow { .. } => "DSP_OVERFLOW",
            NuevaError::AiProcessingError { .. } => "AI_PROCESSING_ERROR",
            NuevaError::ModelNotFound { .. } => "MODEL_NOT_FOUND",
            NuevaError::OutOfMemory { .. } => "OUT_OF_MEMORY",
            NuevaError::DiskFull { .. } => "DISK_FULL",
            NuevaError::AmbiguousPrompt { .. } => "AMBIGUOUS_PROMPT",
            NuevaError::ConflictingRequest { .. } => "CONFLICTING_REQUEST",
            NuevaError::LayerError { .. } => "LAYER_ERROR",
            NuevaError::BakeError { .. } => "BAKE_ERROR",
            NuevaError::Io(_) => "IO_ERROR",
            NuevaError::Serialization(_) => "SERIALIZATION_ERROR",
        }
    }

    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            NuevaError::DspOverflow { .. } => true,
            NuevaError::OutOfMemory { .. } => true,
            NuevaError::AmbiguousPrompt { .. } => true,
            NuevaError::ConflictingRequest { .. } => true,
            NuevaError::FileNotFound { .. } => true,
            NuevaError::InvalidAudio { .. } => true,
            NuevaError::UnsupportedFormat { .. } => true,
            _ => false,
        }
    }

    /// Get recovery suggestions for this error
    pub fn recovery_suggestions(&self) -> Vec<&'static str> {
        match self {
            NuevaError::FileNotFound { .. } => vec![
                "Check the file path is correct",
                "Verify the file hasn't been moved or deleted",
                "Try importing from a different location",
            ],
            NuevaError::InvalidAudio { .. } => vec![
                "Try converting the file to WAV format first",
                "Check if the file plays in another application",
                "The file may be corrupted - try re-exporting from source",
            ],
            NuevaError::UnsupportedFormat { .. } => vec![
                "Convert to WAV, AIFF, or FLAC format",
                "Supported formats: WAV, AIFF, FLAC, MP3, OGG",
            ],
            NuevaError::DspOverflow { .. } => vec![
                "The effect settings may be too extreme",
                "Try reducing the effect intensity",
                "Effect has been bypassed to prevent audio corruption",
            ],
            NuevaError::AiProcessingError { .. } => vec![
                "Try a different AI model",
                "Use DSP effects instead for similar result",
                "Reduce audio length and try again",
            ],
            NuevaError::ModelNotFound { .. } => vec![
                "The requested model is not installed",
                "Run 'nueva install-model <model_name>' to install",
                "Available models: style-transfer, denoise, restore",
            ],
            NuevaError::OutOfMemory { .. } => vec![
                "Close other applications to free memory",
                "Try processing a shorter audio segment",
                "Use CPU processing instead of GPU",
            ],
            NuevaError::DiskFull { .. } => vec![
                "Free up disk space",
                "Change the project location to a drive with more space",
                "Export to a different location",
            ],
            _ => vec![],
        }
    }

    /// Get a user-friendly message for this error
    pub fn friendly_message(&self) -> String {
        match self {
            NuevaError::FileNotFound { path, .. } => {
                format!("I couldn't find the file at '{}'. Could you check if it's in the right location?", path)
            }
            NuevaError::InvalidAudio { reason, .. } => {
                format!("This file doesn't appear to be valid audio: {}. What happened when you try to play it in another app?", reason)
            }
            NuevaError::DspOverflow { effect_id } => {
                format!("Whoa, the '{}' effect created some problematic audio! I've bypassed it to protect your ears.", effect_id)
            }
            NuevaError::AiProcessingError { reason } => {
                format!("The AI processing didn't work this time: {}. Want me to try a DSP-based approach instead?", reason)
            }
            NuevaError::OutOfMemory { .. } => {
                "We're running low on memory for this operation. A few options:\n\
                 1. Close some other apps\n\
                 2. Process just a section of the audio\n\
                 3. Use lighter processing"
                    .to_string()
            }
            _ => self.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        let err = NuevaError::FileNotFound {
            path: "test.wav".to_string(),
            source: None,
        };
        assert_eq!(err.error_code(), "FILE_NOT_FOUND");
    }

    #[test]
    fn test_recovery_suggestions() {
        let err = NuevaError::DspOverflow {
            effect_id: "eq-1".to_string(),
        };
        assert!(!err.recovery_suggestions().is_empty());
        assert!(err.is_recoverable());
    }
}
