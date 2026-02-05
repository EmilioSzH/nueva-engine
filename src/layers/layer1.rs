//! Layer 1 - AI State Buffer
//!
//! Layer 1 is the output of AI/Neural transformations.
//! Initially a copy of Layer 0, it holds the result of any
//! AI-based processing (style transfer, denoising, restoration, etc.)

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::layer0::Layer0;
use crate::error::{NuevaError, Result};

/// Metadata about AI processing applied to Layer 1
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Layer1Metadata {
    /// Name of the AI model used (e.g., "style-transfer", "denoise", "restore")
    pub model_used: Option<String>,
    /// The user prompt that triggered the AI processing
    pub prompt: Option<String>,
    /// Parameters passed to the AI model
    pub processing_params: Value,
    /// ISO 8601 timestamp of when processing was applied
    pub processed_at: Option<String>,
    /// List of intentional artifacts for context-aware processing
    /// (e.g., ["vinyl_crackle", "tape_hiss"] for lo-fi aesthetic)
    pub intentional_artifacts: Vec<String>,
}

impl Layer1Metadata {
    /// Create empty metadata (for pristine state)
    pub fn new() -> Self {
        Self {
            model_used: None,
            prompt: None,
            processing_params: Value::Null,
            processed_at: None,
            intentional_artifacts: Vec::new(),
        }
    }

    /// Check if this metadata indicates AI processing was applied
    pub fn has_processing(&self) -> bool {
        self.model_used.is_some()
    }

    /// Clear all metadata (reset to pristine)
    pub fn clear(&mut self) {
        self.model_used = None;
        self.prompt = None;
        self.processing_params = Value::Null;
        self.processed_at = None;
        self.intentional_artifacts.clear();
    }
}

/// Layer 1: AI State Buffer
///
/// This layer holds the output of AI/Neural transformations.
/// It starts as a copy of Layer 0 and is updated whenever AI
/// processing is applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer1 {
    /// Path to the processed audio WAV file
    audio_path: PathBuf,
    /// Metadata about what AI processing was applied
    metadata: Layer1Metadata,
    /// True if this layer is identical to Layer 0 (no AI processing)
    is_pristine: bool,
}

impl Layer1 {
    /// Create Layer 1 from Layer 0 (initial copy)
    ///
    /// This creates Layer 1 as a copy of the source audio from Layer 0.
    /// The new file will be stored in the project directory with a
    /// distinguishing name.
    ///
    /// # Arguments
    /// * `l0` - Reference to Layer 0
    /// * `project_dir` - Directory where the Layer 1 audio should be stored
    ///
    /// # Errors
    /// Returns error if the source file cannot be read or copied
    pub fn from_layer0(l0: &Layer0, project_dir: &Path) -> Result<Self> {
        let source_path = l0.get_source_path();

        // Create the Layer 1 audio path
        let file_name = source_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("audio");
        let l1_filename = format!("{}_layer1.wav", file_name);
        let audio_path = project_dir.join(&l1_filename);

        // Copy the source file to Layer 1 location
        fs::copy(source_path, &audio_path).map_err(|e| NuevaError::LayerError {
            reason: format!("Failed to create Layer 1 audio: {}", e),
        })?;

        Ok(Self {
            audio_path,
            metadata: Layer1Metadata::new(),
            is_pristine: true,
        })
    }

    /// Create Layer 1 from an existing path (for loading saved projects)
    pub fn from_path(audio_path: PathBuf, metadata: Layer1Metadata, is_pristine: bool) -> Self {
        Self {
            audio_path,
            metadata,
            is_pristine,
        }
    }

    /// Get the path to the processed audio file
    pub fn get_audio_path(&self) -> &Path {
        &self.audio_path
    }

    /// Get the metadata about AI processing
    pub fn get_metadata(&self) -> &Layer1Metadata {
        &self.metadata
    }

    /// Get mutable reference to metadata
    pub fn get_metadata_mut(&mut self) -> &mut Layer1Metadata {
        &mut self.metadata
    }

    /// Check if this layer is pristine (no AI processing applied)
    pub fn is_pristine(&self) -> bool {
        self.is_pristine
    }

    /// Reset Layer 1 to match Layer 0 (discard AI processing)
    ///
    /// This copies the source audio from Layer 0, overwriting any
    /// AI-processed audio in Layer 1.
    ///
    /// # Arguments
    /// * `l0` - Reference to Layer 0
    ///
    /// # Errors
    /// Returns error if the source file cannot be read or copied
    pub fn reset_to_source(&mut self, l0: &Layer0) -> Result<()> {
        let source_path = l0.get_source_path();

        // Copy source back to Layer 1 location
        fs::copy(source_path, &self.audio_path).map_err(|e| NuevaError::LayerError {
            reason: format!("Failed to reset Layer 1: {}", e),
        })?;

        // Clear metadata and mark as pristine
        self.metadata.clear();
        self.is_pristine = true;

        Ok(())
    }

    /// Mark this layer as having been processed by AI
    ///
    /// This updates the metadata and clears the pristine flag.
    ///
    /// # Arguments
    /// * `model` - Name of the AI model used
    /// * `prompt` - User prompt that triggered the processing
    /// * `params` - Processing parameters
    pub fn mark_processed(&mut self, model: &str, prompt: &str, params: Value) {
        self.metadata.model_used = Some(model.to_string());
        self.metadata.prompt = Some(prompt.to_string());
        self.metadata.processing_params = params;
        self.metadata.processed_at = Some(current_timestamp());
        self.is_pristine = false;
    }

    /// Add intentional artifacts to the metadata
    ///
    /// This is used for context-aware processing, allowing the system
    /// to know which "imperfections" are intentional (e.g., lo-fi aesthetic).
    pub fn add_intentional_artifact(&mut self, artifact: &str) {
        if !self
            .metadata
            .intentional_artifacts
            .contains(&artifact.to_string())
        {
            self.metadata
                .intentional_artifacts
                .push(artifact.to_string());
        }
    }

    /// Remove an intentional artifact from the metadata
    pub fn remove_intentional_artifact(&mut self, artifact: &str) {
        self.metadata
            .intentional_artifacts
            .retain(|a| a != artifact);
    }

    /// Check if a specific artifact is marked as intentional
    pub fn is_artifact_intentional(&self, artifact: &str) -> bool {
        self.metadata
            .intentional_artifacts
            .contains(&artifact.to_string())
    }

    /// Update the audio path (used when writing processed audio)
    pub fn set_audio_path(&mut self, path: PathBuf) {
        self.audio_path = path;
    }
}

/// Get current ISO 8601 timestamp
fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let secs = duration.as_secs();

    // Calculate date components
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    let mut year = 1970i32;
    let mut remaining_days = days as i32;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let days_in_months: [i32; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for &days_in_month in &days_in_months {
        if remaining_days < days_in_month {
            break;
        }
        remaining_days -= days_in_month;
        month += 1;
    }

    let day = remaining_days + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_wav(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..(44100 * 2) {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();
        path
    }

    #[test]
    fn test_layer1_from_layer0() {
        let dir = tempdir().unwrap();
        let wav_path = create_test_wav(dir.path(), "source.wav");

        let layer0 = Layer0::new(wav_path).unwrap();
        let layer1 = Layer1::from_layer0(&layer0, dir.path()).unwrap();

        assert!(layer1.is_pristine());
        assert!(layer1.get_audio_path().exists());
        assert!(!layer1.get_metadata().has_processing());
    }

    #[test]
    fn test_mark_processed() {
        let dir = tempdir().unwrap();
        let wav_path = create_test_wav(dir.path(), "source.wav");

        let layer0 = Layer0::new(wav_path).unwrap();
        let mut layer1 = Layer1::from_layer0(&layer0, dir.path()).unwrap();

        assert!(layer1.is_pristine());

        layer1.mark_processed(
            "style-transfer",
            "make it sound vintage",
            serde_json::json!({"strength": 0.8}),
        );

        assert!(!layer1.is_pristine());
        assert_eq!(
            layer1.get_metadata().model_used.as_deref(),
            Some("style-transfer")
        );
        assert_eq!(
            layer1.get_metadata().prompt.as_deref(),
            Some("make it sound vintage")
        );
        assert!(layer1.get_metadata().processed_at.is_some());
    }

    #[test]
    fn test_reset_to_source() {
        let dir = tempdir().unwrap();
        let wav_path = create_test_wav(dir.path(), "source.wav");

        let layer0 = Layer0::new(wav_path).unwrap();
        let mut layer1 = Layer1::from_layer0(&layer0, dir.path()).unwrap();

        // Process the layer
        layer1.mark_processed("denoise", "clean it up", serde_json::json!({}));
        assert!(!layer1.is_pristine());

        // Reset to source
        layer1.reset_to_source(&layer0).unwrap();

        assert!(layer1.is_pristine());
        assert!(!layer1.get_metadata().has_processing());
    }

    #[test]
    fn test_intentional_artifacts() {
        let dir = tempdir().unwrap();
        let wav_path = create_test_wav(dir.path(), "source.wav");

        let layer0 = Layer0::new(wav_path).unwrap();
        let mut layer1 = Layer1::from_layer0(&layer0, dir.path()).unwrap();

        layer1.add_intentional_artifact("vinyl_crackle");
        layer1.add_intentional_artifact("tape_hiss");

        assert!(layer1.is_artifact_intentional("vinyl_crackle"));
        assert!(layer1.is_artifact_intentional("tape_hiss"));
        assert!(!layer1.is_artifact_intentional("digital_noise"));

        layer1.remove_intentional_artifact("vinyl_crackle");
        assert!(!layer1.is_artifact_intentional("vinyl_crackle"));
    }

    #[test]
    fn test_layer1_metadata_default() {
        let metadata = Layer1Metadata::new();

        assert!(metadata.model_used.is_none());
        assert!(metadata.prompt.is_none());
        assert!(metadata.processing_params.is_null());
        assert!(metadata.processed_at.is_none());
        assert!(metadata.intentional_artifacts.is_empty());
        assert!(!metadata.has_processing());
    }
}
