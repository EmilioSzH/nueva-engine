//! Error types for Nueva state management.

use std::path::PathBuf;
use thiserror::Error;

/// Result type for Nueva operations.
pub type Result<T> = std::result::Result<T, NuevaError>;

/// Errors that can occur in Nueva state management.
#[derive(Error, Debug)]
pub enum NuevaError {
    // File Errors
    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },

    #[error("Failed to read file: {path}: {source}")]
    FileReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to write file: {path}: {source}")]
    FileWriteError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Directory creation failed: {path}: {source}")]
    DirectoryCreateError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Invalid project path: {path}")]
    InvalidProjectPath { path: PathBuf },

    // Serialization Errors
    #[error("JSON serialization error: {0}")]
    JsonSerializationError(#[from] serde_json::Error),

    #[error("Invalid project schema version: {version}")]
    InvalidSchemaVersion { version: String },

    #[error("Migration failed from {from} to {to}: {reason}")]
    MigrationError {
        from: String,
        to: String,
        reason: String,
    },

    // Project Errors
    #[error("Project already exists: {path}")]
    ProjectAlreadyExists { path: PathBuf },

    #[error("Project not found: {path}")]
    ProjectNotFound { path: PathBuf },

    #[error("Project is locked (possible crash): {path}")]
    ProjectLocked { path: PathBuf },

    #[error("Invalid project structure: {reason}")]
    InvalidProjectStructure { reason: String },

    // Undo/Redo Errors
    #[error("Nothing to undo")]
    NothingToUndo,

    #[error("Nothing to redo")]
    NothingToRedo,

    #[error("Undo action not found: {action_id}")]
    UndoActionNotFound { action_id: String },

    // Audio Errors
    #[error("Audio file not found: {path}")]
    AudioNotFound { path: PathBuf },

    #[error("Invalid audio format: {reason}")]
    InvalidAudioFormat { reason: String },

    #[error("Audio validation failed: {reason}")]
    AudioValidationFailed { reason: String },

    // Processing Errors
    #[error("Bake operation failed: {reason}")]
    BakeError { reason: String },

    #[error("Processing in progress")]
    ProcessingInProgress,

    // Storage Errors
    #[error(
        "Insufficient disk space: needed {needed_bytes} bytes, available {available_bytes} bytes"
    )]
    InsufficientDiskSpace {
        needed_bytes: u64,
        available_bytes: u64,
    },

    #[error("Storage quota exceeded: Layer 1 using {used_mb:.1} MB")]
    StorageQuotaExceeded { used_mb: f64 },

    // Recovery Errors
    #[error("Recovery failed: {reason}")]
    RecoveryFailed { reason: String },

    #[error("No autosave found for recovery")]
    NoAutosaveFound,

    // Generic Errors
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl NuevaError {
    /// Returns true if this error indicates the operation can be retried.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            NuevaError::ProcessingInProgress
                | NuevaError::InsufficientDiskSpace { .. }
                | NuevaError::FileWriteError { .. }
        )
    }

    /// Returns a user-friendly recovery suggestion.
    pub fn recovery_suggestion(&self) -> Option<&'static str> {
        match self {
            NuevaError::FileNotFound { .. } => Some("Check the file path and try again."),
            NuevaError::ProjectLocked { .. } => {
                Some("The project may have crashed. Try 'nueva recover <path>'.")
            }
            NuevaError::InsufficientDiskSpace { .. } => {
                Some("Free up disk space or prune project history.")
            }
            NuevaError::NothingToUndo => Some("There are no actions to undo."),
            NuevaError::NothingToRedo => Some("There are no undone actions to redo."),
            NuevaError::StorageQuotaExceeded { .. } => {
                Some("Consider baking to flatten layers or pruning history.")
            }
            _ => None,
        }
    }
}
