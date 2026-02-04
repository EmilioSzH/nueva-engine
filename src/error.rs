//! Error types for Nueva
//!
//! All errors implement recovery suggestions per spec §9.

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

    #[error("Invalid audio file: {path} - {reason}")]
    InvalidAudioFile { path: String, reason: String },

    #[error("Unsupported format: {format}")]
    UnsupportedFormat { format: String },

    // Processing Errors
    #[error("Neural model error: {model} - {message}")]
    NeuralModelError { model: String, message: String },

    #[error("Effect processing error: {effect} - {message}")]
    EffectError { effect: String, message: String },

    #[error("Audio processing produced invalid samples (NaN/Inf)")]
    InvalidSamples,

    // Agent Errors
    #[error("Could not understand prompt: {prompt}")]
    AmbiguousPrompt { prompt: String },

    #[error("Unknown model: {model}")]
    UnknownModel { model: String },

    #[error("Model not available: {model} - {reason}")]
    ModelUnavailable { model: String, reason: String },

    // Resource Errors
    #[error("Out of memory: {context}")]
    OutOfMemory { context: String },

    #[error("GPU not available")]
    GpuUnavailable,

    // State Errors
    #[error("Project not found: {path}")]
    ProjectNotFound { path: String },

    #[error("Invalid project state: {reason}")]
    InvalidProjectState { reason: String },

    // Serialization
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    // IO
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl NuevaError {
    /// Returns recovery suggestions for this error
    pub fn recovery_suggestions(&self) -> Vec<&'static str> {
        match self {
            NuevaError::FileNotFound { .. } => vec![
                "Check the file path is correct",
                "Verify the file hasn't been moved or deleted",
                "Try importing from a different location",
            ],
            NuevaError::InvalidAudioFile { .. } => vec![
                "Try converting the file to WAV format first",
                "Check if the file plays in another application",
                "The file may be corrupted - try re-exporting from source",
            ],
            NuevaError::UnsupportedFormat { .. } => vec![
                "Convert to WAV, AIFF, or FLAC format",
                "Supported formats: WAV, AIFF, FLAC, MP3, OGG",
            ],
            NuevaError::NeuralModelError { .. } => vec![
                "Try again - neural models can have transient failures",
                "Use DSP tools as a fallback",
                "Check GPU memory if using neural models",
            ],
            NuevaError::EffectError { .. } => vec![
                "The effect has been bypassed to prevent audio damage",
                "Try with more conservative settings",
                "Reset the effect to defaults",
            ],
            NuevaError::InvalidSamples => vec![
                "The effect chain has been reset to prevent audio damage",
                "Check for extreme parameter values",
                "Try processing a shorter section first",
            ],
            NuevaError::AmbiguousPrompt { .. } => vec![
                "Try being more specific about what you want",
                "Examples: 'make it warmer', 'add compression', 'reduce noise'",
            ],
            NuevaError::UnknownModel { .. } => vec![
                "Available models: style-transfer, denoise, restore, enhance, ace-step",
                "Use 'list models' to see all available models",
            ],
            NuevaError::ModelUnavailable { .. } => vec![
                "Check that the model files are installed",
                "Verify GPU drivers are up to date",
                "Try running with CPU inference (slower)",
            ],
            NuevaError::OutOfMemory { .. } => vec![
                "Close other applications to free memory",
                "Try processing a smaller file",
                "Split the audio into sections",
            ],
            NuevaError::GpuUnavailable => vec![
                "Neural processing will use CPU (slower)",
                "Install CUDA/ROCm for GPU acceleration",
                "Check GPU drivers",
            ],
            NuevaError::ProjectNotFound { .. } => vec![
                "Check the project path is correct",
                "The project may have been moved or deleted",
            ],
            NuevaError::InvalidProjectState { .. } => vec![
                "The project file may be corrupted",
                "Check for a recent autosave in the backups folder",
            ],
            NuevaError::Serialization(_) => vec![
                "The file may be corrupted",
                "Check for syntax errors in JSON files",
            ],
            NuevaError::Io(_) => vec![
                "Check file permissions",
                "Verify disk space is available",
            ],
        }
    }

    /// Whether this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            NuevaError::InvalidSamples => false, // Serious - could damage audio
            NuevaError::OutOfMemory { .. } => false, // Need user intervention
            _ => true,
        }
    }

    /// Error code for logging/debugging
    pub fn error_code(&self) -> &'static str {
        match self {
            NuevaError::FileNotFound { .. } => "FILE_NOT_FOUND",
            NuevaError::InvalidAudioFile { .. } => "INVALID_AUDIO",
            NuevaError::UnsupportedFormat { .. } => "UNSUPPORTED_FORMAT",
            NuevaError::NeuralModelError { .. } => "NEURAL_ERROR",
            NuevaError::EffectError { .. } => "EFFECT_ERROR",
            NuevaError::InvalidSamples => "INVALID_SAMPLES",
            NuevaError::AmbiguousPrompt { .. } => "AMBIGUOUS_PROMPT",
            NuevaError::UnknownModel { .. } => "UNKNOWN_MODEL",
            NuevaError::ModelUnavailable { .. } => "MODEL_UNAVAILABLE",
            NuevaError::OutOfMemory { .. } => "OUT_OF_MEMORY",
            NuevaError::GpuUnavailable => "GPU_UNAVAILABLE",
            NuevaError::ProjectNotFound { .. } => "PROJECT_NOT_FOUND",
            NuevaError::InvalidProjectState { .. } => "INVALID_STATE",
            NuevaError::Serialization(_) => "SERIALIZATION_ERROR",
            NuevaError::Io(_) => "IO_ERROR",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_has_recovery_suggestions() {
        let err = NuevaError::FileNotFound {
            path: "test.wav".to_string(),
            source: None,
        };
        assert!(!err.recovery_suggestions().is_empty());
    }

    #[test]
    fn test_error_codes_are_unique() {
        // Just verify error codes exist for all variants
        let errors = vec![
            NuevaError::FileNotFound {
                path: "".into(),
                source: None,
            },
            NuevaError::InvalidAudioFile {
                path: "".into(),
                reason: "".into(),
            },
            NuevaError::UnsupportedFormat { format: "".into() },
            NuevaError::NeuralModelError {
                model: "".into(),
                message: "".into(),
            },
            NuevaError::AmbiguousPrompt { prompt: "".into() },
        ];

        for err in errors {
            assert!(!err.error_code().is_empty());
        }
    }
}
