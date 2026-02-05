//! Project Container
//!
//! Ties all layers together into a cohesive project structure.
//! Manages project lifecycle, persistence, and layer operations.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::layer0::Layer0;
use super::layer1::{Layer1, Layer1Metadata};
use super::layer2::Layer2;
use crate::error::{NuevaError, Result};

/// Policy for handling Layer 2 (DSP chain) during AI processing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerPreservationPolicy {
    /// Keep the existing DSP chain (default behavior)
    PreserveL2,
    /// Clear the DSP chain
    ResetL2,
    /// Ask the user what to do
    AskUser,
    /// Let the agent decide based on context
    Smart,
}

impl Default for LayerPreservationPolicy {
    fn default() -> Self {
        LayerPreservationPolicy::PreserveL2
    }
}

/// Project manifest stored as JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectManifest {
    name: String,
    version: String,
    created_at: String,
    modified_at: String,
    layer0: Layer0Manifest,
    layer1: Layer1Manifest,
    layer2: Layer2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Layer0Manifest {
    source_path: PathBuf,
    checksum: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Layer1Manifest {
    audio_path: PathBuf,
    metadata: Layer1Metadata,
    is_pristine: bool,
}

/// Project Container
///
/// A Project ties together all three layers and manages the
/// project directory structure, persistence, and lifecycle.
#[derive(Debug)]
pub struct Project {
    /// Project name
    pub name: String,
    /// Path to the project directory
    pub project_dir: PathBuf,
    /// Layer 0: Immutable source
    pub layer0: Layer0,
    /// Layer 1: AI state buffer
    pub layer1: Layer1,
    /// Layer 2: DSP effect chain
    pub layer2: Layer2,
    /// ISO 8601 timestamp of creation
    pub created_at: String,
    /// ISO 8601 timestamp of last modification
    pub modified_at: String,
}

impl Project {
    /// Create a new project from source audio
    ///
    /// This creates the project directory structure and initializes
    /// all three layers.
    ///
    /// # Arguments
    /// * `name` - Project name (used for directory and display)
    /// * `source_audio` - Path to the source audio file
    /// * `project_dir` - Base directory where the project will be created
    ///
    /// # Directory Structure
    /// ```text
    /// project_dir/
    ///   project.json     # Project manifest
    ///   audio/
    ///     source.wav     # Copy of original (Layer 0)
    ///     layer1.wav     # AI processed audio (Layer 1)
    /// ```
    pub fn create(name: &str, source_audio: &Path, project_dir: &Path) -> Result<Self> {
        // Create project directory structure
        let audio_dir = project_dir.join("audio");
        fs::create_dir_all(&audio_dir).map_err(|e| NuevaError::LayerError {
            reason: format!("Failed to create project directory: {}", e),
        })?;

        // Copy source audio to project
        let source_filename = source_audio
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("source.wav");
        let project_source = audio_dir.join(source_filename);

        fs::copy(source_audio, &project_source).map_err(|e| NuevaError::LayerError {
            reason: format!("Failed to copy source audio: {}", e),
        })?;

        // Create Layer 0 from the copied source
        let layer0 = Layer0::new(project_source)?;

        // Create Layer 1 from Layer 0
        let layer1 = Layer1::from_layer0(&layer0, &audio_dir)?;

        // Create empty Layer 2
        let layer2 = Layer2::new();

        let timestamp = current_timestamp();

        let project = Self {
            name: name.to_string(),
            project_dir: project_dir.to_path_buf(),
            layer0,
            layer1,
            layer2,
            created_at: timestamp.clone(),
            modified_at: timestamp,
        };

        // Save the initial project state
        project.save()?;

        Ok(project)
    }

    /// Load an existing project from disk
    ///
    /// # Arguments
    /// * `project_dir` - Path to the project directory
    pub fn load(project_dir: &Path) -> Result<Self> {
        let manifest_path = project_dir.join("project.json");

        if !manifest_path.exists() {
            return Err(NuevaError::FileNotFound {
                path: manifest_path.display().to_string(),
                source: None,
            });
        }

        let file = File::open(&manifest_path)?;
        let reader = BufReader::new(file);
        let manifest: ProjectManifest = serde_json::from_reader(reader)?;

        // Reconstruct Layer 0
        let layer0 = Layer0::new(manifest.layer0.source_path)?;

        // Reconstruct Layer 1
        let layer1 = Layer1::from_path(
            manifest.layer1.audio_path,
            manifest.layer1.metadata,
            manifest.layer1.is_pristine,
        );

        Ok(Self {
            name: manifest.name,
            project_dir: project_dir.to_path_buf(),
            layer0,
            layer1,
            layer2: manifest.layer2,
            created_at: manifest.created_at,
            modified_at: manifest.modified_at,
        })
    }

    /// Save the project state to disk
    pub fn save(&self) -> Result<()> {
        let manifest = ProjectManifest {
            name: self.name.clone(),
            version: "1.0".to_string(),
            created_at: self.created_at.clone(),
            modified_at: current_timestamp(),
            layer0: Layer0Manifest {
                source_path: self.layer0.get_source_path().to_path_buf(),
                checksum: self.layer0.get_checksum().to_string(),
                created_at: self.layer0.get_created_at().to_string(),
            },
            layer1: Layer1Manifest {
                audio_path: self.layer1.get_audio_path().to_path_buf(),
                metadata: self.layer1.get_metadata().clone(),
                is_pristine: self.layer1.is_pristine(),
            },
            layer2: self.layer2.clone(),
        };

        let manifest_path = self.project_dir.join("project.json");
        let file = File::create(&manifest_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &manifest)?;

        Ok(())
    }

    /// Bake all layers into a new Layer 0
    ///
    /// This operation:
    /// 1. Renders the current state (L1 + L2 effects) to a new audio file
    /// 2. Creates a new Layer 0 from that rendered file
    /// 3. Resets Layer 1 to match the new Layer 0
    /// 4. Optionally clears Layer 2 (based on policy or parameter)
    ///
    /// After baking, the "original" becomes the rendered output.
    /// This is destructive in the sense that the previous Layer 0 is replaced.
    pub fn bake(&mut self) -> Result<()> {
        // For now, this is a simplified implementation that:
        // 1. Copies Layer 1 audio to a new "baked" file
        // 2. Creates new Layer 0 from that
        // 3. Resets Layer 1
        // 4. Clears Layer 2

        // In a full implementation, this would render L2 effects onto L1 audio

        let audio_dir = self.project_dir.join("audio");
        let baked_filename = format!("{}_baked_{}.wav", self.name, current_timestamp_compact());
        let baked_path = audio_dir.join(&baked_filename);

        // Copy current Layer 1 audio to baked file
        // (In full implementation, this would render with DSP effects)
        fs::copy(self.layer1.get_audio_path(), &baked_path).map_err(|e| NuevaError::BakeError {
            reason: format!("Failed to create baked audio: {}", e),
        })?;

        // Create new Layer 0 from baked audio
        let new_layer0 = Layer0::new(baked_path)?;

        // Create new Layer 1 from new Layer 0
        let new_layer1 = Layer1::from_layer0(&new_layer0, &audio_dir)?;

        // Update project
        self.layer0 = new_layer0;
        self.layer1 = new_layer1;
        self.layer2.clear();
        self.modified_at = current_timestamp();

        // Save the updated project
        self.save()?;

        Ok(())
    }

    /// Reset Layer 1 to match Layer 0 (discard AI processing)
    pub fn reset_ai(&mut self) -> Result<()> {
        self.layer1.reset_to_source(&self.layer0)?;
        self.modified_at = current_timestamp();
        self.save()?;
        Ok(())
    }

    /// Clear Layer 2 (remove all DSP effects)
    pub fn reset_dsp(&mut self) {
        self.layer2.clear();
        self.modified_at = current_timestamp();
        // Note: save() should be called by the caller if persistence is needed
    }

    /// Reset everything back to the original import state
    ///
    /// This:
    /// - Resets Layer 1 to Layer 0
    /// - Clears Layer 2
    pub fn reset_all(&mut self) -> Result<()> {
        self.reset_ai()?;
        self.reset_dsp();
        self.save()?;
        Ok(())
    }

    /// Determine the Layer 2 preservation policy based on prompt analysis
    ///
    /// This analyzes the user's prompt to determine whether DSP effects
    /// should be preserved when AI processing is applied.
    ///
    /// # Examples
    /// - "make it sound vintage" -> PreserveL2 (style change, DSP might still apply)
    /// - "remove all effects and clean it up" -> ResetL2 (explicit reset)
    /// - "apply reverb" -> PreserveL2 (adding DSP, keep existing)
    pub fn determine_l2_policy(prompt: &str) -> LayerPreservationPolicy {
        let prompt_lower = prompt.to_lowercase();

        // Keywords that suggest resetting DSP
        let reset_keywords = [
            "remove all",
            "clear effects",
            "reset",
            "start fresh",
            "clean slate",
            "from scratch",
            "original",
            "raw",
        ];

        // Keywords that suggest keeping DSP
        let preserve_keywords = [
            "also",
            "in addition",
            "keep",
            "preserve",
            "maintain",
            "add",
            "more",
        ];

        // Check for reset keywords
        for keyword in &reset_keywords {
            if prompt_lower.contains(keyword) {
                return LayerPreservationPolicy::ResetL2;
            }
        }

        // Check for explicit preserve keywords
        for keyword in &preserve_keywords {
            if prompt_lower.contains(keyword) {
                return LayerPreservationPolicy::PreserveL2;
            }
        }

        // Ambiguous case - might need to ask
        if prompt_lower.contains("change") || prompt_lower.contains("different") {
            return LayerPreservationPolicy::Smart;
        }

        // Default: preserve existing effects
        LayerPreservationPolicy::PreserveL2
    }

    /// Get the project name
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Get the project directory path
    pub fn get_project_dir(&self) -> &Path {
        &self.project_dir
    }

    /// Check if the project has any AI processing applied
    pub fn has_ai_processing(&self) -> bool {
        !self.layer1.is_pristine()
    }

    /// Check if the project has any DSP effects
    pub fn has_dsp_effects(&self) -> bool {
        !self.layer2.is_empty()
    }

    /// Get a summary of the current project state
    pub fn get_state_summary(&self) -> ProjectStateSummary {
        ProjectStateSummary {
            name: self.name.clone(),
            has_ai_processing: self.has_ai_processing(),
            ai_model: self.layer1.get_metadata().model_used.clone(),
            dsp_effect_count: self.layer2.len(),
            enabled_effect_count: self.layer2.enabled_count(),
            created_at: self.created_at.clone(),
            modified_at: self.modified_at.clone(),
        }
    }
}

/// Summary of project state for quick inspection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStateSummary {
    pub name: String,
    pub has_ai_processing: bool,
    pub ai_model: Option<String>,
    pub dsp_effect_count: usize,
    pub enabled_effect_count: usize,
    pub created_at: String,
    pub modified_at: String,
}

/// Get current ISO 8601 timestamp
fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let secs = duration.as_secs();

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

/// Get a compact timestamp for filenames
fn current_timestamp_compact() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    format!("{}", duration.as_secs())
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
    fn test_project_create() {
        let source_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let source_wav = create_test_wav(source_dir.path(), "source.wav");

        let project = Project::create("TestProject", &source_wav, project_dir.path()).unwrap();

        assert_eq!(project.name, "TestProject");
        assert!(project.project_dir.join("project.json").exists());
        assert!(project.layer1.is_pristine());
        assert!(project.layer2.is_empty());
    }

    #[test]
    fn test_project_save_load() {
        let source_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let source_wav = create_test_wav(source_dir.path(), "source.wav");

        // Create and save
        let _project = Project::create("TestProject", &source_wav, project_dir.path()).unwrap();

        // Modify Layer 2
        {
            let mut project = Project::load(project_dir.path()).unwrap();
            project
                .layer2
                .add_effect(super::super::layer2::EffectState::new("eq-1", "eq"));
            project.save().unwrap();
        }

        // Load again and verify
        let loaded = Project::load(project_dir.path()).unwrap();
        assert_eq!(loaded.name, "TestProject");
        assert_eq!(loaded.layer2.len(), 1);
    }

    #[test]
    fn test_reset_ai() {
        let source_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let source_wav = create_test_wav(source_dir.path(), "source.wav");

        let mut project = Project::create("TestProject", &source_wav, project_dir.path()).unwrap();

        // Mark as processed
        project
            .layer1
            .mark_processed("style-transfer", "make it vintage", serde_json::json!({}));
        assert!(!project.layer1.is_pristine());

        // Reset
        project.reset_ai().unwrap();
        assert!(project.layer1.is_pristine());
    }

    #[test]
    fn test_reset_dsp() {
        let source_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let source_wav = create_test_wav(source_dir.path(), "source.wav");

        let mut project = Project::create("TestProject", &source_wav, project_dir.path()).unwrap();

        // Add effects
        project
            .layer2
            .add_effect(super::super::layer2::EffectState::new("eq-1", "eq"));
        project
            .layer2
            .add_effect(super::super::layer2::EffectState::new(
                "comp-1",
                "compressor",
            ));
        assert!(!project.layer2.is_empty());

        // Reset
        project.reset_dsp();
        assert!(project.layer2.is_empty());
    }

    #[test]
    fn test_reset_all() {
        let source_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let source_wav = create_test_wav(source_dir.path(), "source.wav");

        let mut project = Project::create("TestProject", &source_wav, project_dir.path()).unwrap();

        // Modify both layers
        project
            .layer1
            .mark_processed("denoise", "clean it", serde_json::json!({}));
        project
            .layer2
            .add_effect(super::super::layer2::EffectState::new("eq-1", "eq"));

        assert!(!project.layer1.is_pristine());
        assert!(!project.layer2.is_empty());

        // Reset all
        project.reset_all().unwrap();

        assert!(project.layer1.is_pristine());
        assert!(project.layer2.is_empty());
    }

    #[test]
    fn test_determine_l2_policy() {
        // Reset keywords
        assert_eq!(
            Project::determine_l2_policy("remove all effects and start fresh"),
            LayerPreservationPolicy::ResetL2
        );

        assert_eq!(
            Project::determine_l2_policy("give me the original sound"),
            LayerPreservationPolicy::ResetL2
        );

        // Preserve keywords
        assert_eq!(
            Project::determine_l2_policy("also add some reverb"),
            LayerPreservationPolicy::PreserveL2
        );

        assert_eq!(
            Project::determine_l2_policy("keep the EQ but add compression"),
            LayerPreservationPolicy::PreserveL2
        );

        // Default preserve
        assert_eq!(
            Project::determine_l2_policy("make it sound warmer"),
            LayerPreservationPolicy::PreserveL2
        );
    }

    #[test]
    fn test_project_state_summary() {
        let source_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let source_wav = create_test_wav(source_dir.path(), "source.wav");

        let mut project = Project::create("TestProject", &source_wav, project_dir.path()).unwrap();

        // Initial state
        let summary = project.get_state_summary();
        assert_eq!(summary.name, "TestProject");
        assert!(!summary.has_ai_processing);
        assert!(summary.ai_model.is_none());
        assert_eq!(summary.dsp_effect_count, 0);

        // After modifications
        project
            .layer1
            .mark_processed("style-transfer", "vintage", serde_json::json!({}));
        project
            .layer2
            .add_effect(super::super::layer2::EffectState::new("eq-1", "eq"));

        let summary = project.get_state_summary();
        assert!(summary.has_ai_processing);
        assert_eq!(summary.ai_model.as_deref(), Some("style-transfer"));
        assert_eq!(summary.dsp_effect_count, 1);
    }
}
