//! Project State Schema
//!
//! Defines the project.json schema per §8.2 of the Nueva spec.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::state::error::{NuevaError, Result};
use crate::state::migration::{migrate_project, CURRENT_SCHEMA_VERSION, NUEVA_VERSION};

/// Project directory structure constants.
pub const PROJECT_FILE: &str = "project.json";
pub const AUDIO_DIR: &str = "audio";
pub const HISTORY_DIR: &str = "history";
pub const BACKUPS_DIR: &str = "backups";
pub const EXPORTS_DIR: &str = "exports";
pub const CACHE_DIR: &str = "cache";
pub const LOCK_FILE: &str = ".lock";

/// Layer 0 source file name.
pub const LAYER0_FILE: &str = "layer0_source.wav";
/// Layer 1 AI state file name.
pub const LAYER1_FILE: &str = "layer1_ai.wav";
/// Layer 1 metadata file name.
pub const LAYER1_META_FILE: &str = "layer1_ai_meta.json";

/// Main project state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Schema version for migration support.
    #[serde(default = "default_schema_version")]
    pub schema_version: String,

    /// Timestamp when project was created.
    pub created_at: DateTime<Utc>,

    /// Timestamp of last modification.
    pub modified_at: DateTime<Utc>,

    /// Nueva version that last modified this project.
    pub nueva_version: String,

    /// Source file information.
    pub source: SourceInfo,

    /// Layer 0 (immutable source) state.
    pub layer0: Layer0,

    /// Layer 1 (AI state) information.
    pub layer1: Layer1,

    /// Layer 2 (DSP chain) state.
    pub layer2: Layer2,

    /// Conversation context (for agent continuity).
    #[serde(default)]
    pub conversation: ConversationContext,

    /// Path to the project directory (not serialized).
    #[serde(skip)]
    pub project_path: PathBuf,

    /// Unknown fields preserved for forward compatibility.
    #[serde(flatten)]
    pub unknown_fields: HashMap<String, serde_json::Value>,
}

fn default_schema_version() -> String {
    CURRENT_SCHEMA_VERSION.to_string()
}

/// Information about the original source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Original filename before import.
    pub original_filename: String,

    /// Original path (may not exist anymore).
    pub original_path: PathBuf,

    /// Import settings applied during conversion.
    #[serde(default)]
    pub import_settings: ImportSettings,
}

/// Settings applied when importing audio.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportSettings {
    /// Original sample rate before conversion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub converted_from_sample_rate: Option<u32>,

    /// Original bit depth before conversion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub converted_from_bit_depth: Option<u16>,
}

/// Layer 0: Immutable source audio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer0 {
    /// Path to the audio file (relative to project).
    pub path: PathBuf,

    /// Sample rate in Hz (standardized to 48000).
    pub sample_rate: u32,

    /// Bit depth (standardized to 32-bit float).
    pub bit_depth: u16,

    /// Number of channels (1 = mono, 2 = stereo).
    pub channels: u8,

    /// Duration in seconds.
    pub duration_seconds: f64,

    /// SHA-256 hash for integrity verification.
    pub hash_sha256: String,
}

/// Layer 1: AI-processed state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer1 {
    /// Path to the AI-processed audio file.
    pub path: PathBuf,

    /// Whether AI processing has been applied.
    pub is_processed: bool,

    /// Whether Layer 1 is identical to Layer 0.
    pub identical_to_layer0: bool,

    /// Processing information (if AI was applied).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processing: Option<Layer1Processing>,
}

/// Information about AI processing applied to Layer 1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer1Processing {
    /// Model used for processing.
    pub model: String,

    /// User prompt that triggered processing.
    pub prompt: String,

    /// Model-specific parameters.
    #[serde(default)]
    pub params: HashMap<String, serde_json::Value>,

    /// When processing was completed.
    pub processed_at: DateTime<Utc>,

    /// How long processing took in milliseconds.
    pub processing_time_ms: u64,
}

/// Layer 2: DSP effect chain.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Layer2 {
    /// Ordered list of effects in the chain.
    #[serde(default)]
    pub chain: Vec<Effect>,
}

/// A single effect in the DSP chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Effect {
    /// Unique identifier for this effect instance.
    pub id: String,

    /// Effect type (e.g., "parametric_eq", "compressor").
    #[serde(rename = "type")]
    pub effect_type: String,

    /// Whether the effect is currently enabled.
    pub enabled: bool,

    /// Effect-specific parameters.
    pub params: HashMap<String, serde_json::Value>,

    /// When the effect was added.
    pub added_at: DateTime<Utc>,

    /// Who added the effect ("user" or "agent").
    pub added_by: String,
}

/// Conversation context for agent continuity.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationContext {
    /// Number of conversation sessions.
    #[serde(default)]
    pub session_count: u32,

    /// Timestamp of last session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_session: Option<DateTime<Utc>>,

    /// Total messages across all sessions.
    #[serde(default)]
    pub total_messages: u32,

    /// Learned user preferences.
    #[serde(default)]
    pub user_preferences: UserPreferences,
}

/// User preferences learned from interactions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserPreferences {
    /// Whether user prefers DSP before neural processing.
    #[serde(default)]
    pub prefers_dsp_first: bool,

    /// Compression style preference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression_preference: Option<String>,

    /// Typical genre for this project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typical_genre: Option<String>,
}

impl Project {
    /// Create a new project at the given path.
    pub fn create(path: &Path, input: Option<&Path>) -> Result<Self> {
        // Check if project already exists
        if path.exists() {
            return Err(NuevaError::ProjectAlreadyExists {
                path: path.to_path_buf(),
            });
        }

        // Create directory structure
        Self::create_directory_structure(path)?;

        let now = Utc::now();

        // Create project state
        let mut project = Project {
            schema_version: CURRENT_SCHEMA_VERSION.to_string(),
            created_at: now,
            modified_at: now,
            nueva_version: NUEVA_VERSION.to_string(),
            source: SourceInfo {
                original_filename: input
                    .map(|p| {
                        p.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    })
                    .unwrap_or_default(),
                original_path: input.map(|p| p.to_path_buf()).unwrap_or_default(),
                import_settings: ImportSettings::default(),
            },
            layer0: Layer0 {
                path: PathBuf::from(AUDIO_DIR).join(LAYER0_FILE),
                sample_rate: 48000,
                bit_depth: 32,
                channels: 2,
                duration_seconds: 0.0,
                hash_sha256: String::new(),
            },
            layer1: Layer1 {
                path: PathBuf::from(AUDIO_DIR).join(LAYER1_FILE),
                is_processed: false,
                identical_to_layer0: true,
                processing: None,
            },
            layer2: Layer2::default(),
            conversation: ConversationContext::default(),
            project_path: path.to_path_buf(),
            unknown_fields: HashMap::new(),
        };

        // Import audio if provided
        if let Some(input_path) = input {
            project.import_audio(input_path)?;
        }

        // Create lock file
        project.create_lock()?;

        Ok(project)
    }

    /// Load an existing project from disk.
    pub fn load(path: &Path) -> Result<Self> {
        let project_file = Self::project_file_path(path);

        if !project_file.exists() {
            return Err(NuevaError::ProjectNotFound {
                path: path.to_path_buf(),
            });
        }

        // Read and parse project.json
        let content = fs::read_to_string(&project_file).map_err(|e| NuevaError::FileReadError {
            path: project_file.clone(),
            source: e,
        })?;

        let mut data: serde_json::Value = serde_json::from_str(&content)?;

        // Migrate if needed
        data = migrate_project(data)?;

        // Parse into Project struct
        let mut project: Project = serde_json::from_value(data)?;
        project.project_path = path.to_path_buf();

        // Create/update lock file
        project.create_lock()?;

        Ok(project)
    }

    /// Save the project to disk.
    pub fn save(&mut self) -> Result<()> {
        self.modified_at = Utc::now();
        self.nueva_version = NUEVA_VERSION.to_string();

        let project_file = Self::project_file_path(&self.project_path);

        let content = serde_json::to_string_pretty(self)?;
        fs::write(&project_file, content).map_err(|e| NuevaError::FileWriteError {
            path: project_file,
            source: e,
        })?;

        Ok(())
    }

    /// Get the path to the project.json file.
    pub fn project_file_path(base: &Path) -> PathBuf {
        base.join(PROJECT_FILE)
    }

    /// Get the path to the history directory.
    pub fn history_dir(&self) -> PathBuf {
        self.project_path.join(HISTORY_DIR)
    }

    /// Get the path to the backups directory.
    pub fn backups_dir(&self) -> PathBuf {
        self.project_path.join(BACKUPS_DIR)
    }

    /// Get the path to the audio directory.
    pub fn audio_dir(&self) -> PathBuf {
        self.project_path.join(AUDIO_DIR)
    }

    /// Create the project directory structure.
    fn create_directory_structure(path: &Path) -> Result<()> {
        let dirs = [AUDIO_DIR, HISTORY_DIR, BACKUPS_DIR, EXPORTS_DIR, CACHE_DIR];

        fs::create_dir_all(path).map_err(|e| NuevaError::DirectoryCreateError {
            path: path.to_path_buf(),
            source: e,
        })?;

        for dir in dirs {
            let dir_path = path.join(dir);
            fs::create_dir_all(&dir_path).map_err(|e| NuevaError::DirectoryCreateError {
                path: dir_path,
                source: e,
            })?;
        }

        Ok(())
    }

    /// Create the lock file.
    fn create_lock(&self) -> Result<()> {
        let lock_path = self.project_path.join(LOCK_FILE);
        let lock_content = format!(
            "{{\"pid\": {}, \"started_at\": \"{}\"}}",
            std::process::id(),
            Utc::now().to_rfc3339()
        );
        fs::write(&lock_path, lock_content).map_err(|e| NuevaError::FileWriteError {
            path: lock_path,
            source: e,
        })?;
        Ok(())
    }

    /// Remove the lock file (call on clean exit).
    pub fn release_lock(&self) -> Result<()> {
        let lock_path = self.project_path.join(LOCK_FILE);
        if lock_path.exists() {
            fs::remove_file(&lock_path)?;
        }
        Ok(())
    }

    /// Import audio file into the project.
    pub fn import_audio(&mut self, input_path: &Path) -> Result<()> {
        if !input_path.exists() {
            return Err(NuevaError::AudioNotFound {
                path: input_path.to_path_buf(),
            });
        }

        // For now, just copy the file (real implementation would convert to 48kHz/32-bit)
        let layer0_path = self.project_path.join(&self.layer0.path);
        fs::copy(input_path, &layer0_path).map_err(|e| NuevaError::FileWriteError {
            path: layer0_path.clone(),
            source: e,
        })?;

        // Copy to Layer 1 as well (initially identical)
        let layer1_path = self.project_path.join(&self.layer1.path);
        fs::copy(input_path, &layer1_path).map_err(|e| NuevaError::FileWriteError {
            path: layer1_path,
            source: e,
        })?;

        // Calculate hash
        let content = fs::read(&layer0_path)?;
        let hash = Sha256::digest(&content);
        self.layer0.hash_sha256 = format!("{:x}", hash);

        // Update source info
        self.source.original_filename = input_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        self.source.original_path = input_path.to_path_buf();

        // TODO: Read actual audio metadata (sample rate, duration, etc.)
        // For now, use placeholder values
        self.layer0.duration_seconds = 0.0; // Would need audio library to read

        Ok(())
    }

    /// Validate that the project is ready for bake operation.
    pub fn validate_for_bake(&self) -> Result<()> {
        // Check Layer 1 exists
        let layer1_path = self.project_path.join(&self.layer1.path);
        if !layer1_path.exists() {
            return Err(NuevaError::BakeError {
                reason: "Layer 1 audio file not found".to_string(),
            });
        }

        // TODO: Additional validations per spec:
        // - Layer 1 is not silence
        // - Final output is not clipping
        // - Duration matches source

        Ok(())
    }

    /// Bake all layers into new Layer 0.
    pub fn bake(&mut self) -> Result<()> {
        self.validate_for_bake()?;

        let now = Utc::now();
        let timestamp = now.format("%Y%m%d_%H%M%S");

        // 1. Backup current Layer 0
        let layer0_path = self.project_path.join(&self.layer0.path);
        let backup_name = format!("layer0_pre_bake_{}.wav", timestamp);
        let backup_path = self.backups_dir().join(&backup_name);

        if layer0_path.exists() {
            fs::copy(&layer0_path, &backup_path).map_err(|e| NuevaError::FileWriteError {
                path: backup_path.clone(),
                source: e,
            })?;
        }

        // 2. Render L1 → L2 DSP chain → temp file
        // TODO: Implement actual DSP processing
        // For now, just copy Layer 1 to Layer 0 (no DSP rendering yet)
        let layer1_path = self.project_path.join(&self.layer1.path);

        // 3. Replace Layer 0 with rendered result
        fs::copy(&layer1_path, &layer0_path).map_err(|e| NuevaError::FileWriteError {
            path: layer0_path.clone(),
            source: e,
        })?;

        // 4. Update Layer 0 hash
        let content = fs::read(&layer0_path)?;
        let hash = Sha256::digest(&content);
        self.layer0.hash_sha256 = format!("{:x}", hash);

        // 5. Reset Layer 1 to copy of new Layer 0
        fs::copy(&layer0_path, &layer1_path).map_err(|e| NuevaError::FileWriteError {
            path: layer1_path,
            source: e,
        })?;

        self.layer1.is_processed = false;
        self.layer1.identical_to_layer0 = true;
        self.layer1.processing = None;

        // 6. Clear Layer 2 DSP chain
        self.layer2.chain.clear();

        // 7. Save project state
        self.save()?;

        Ok(())
    }

    /// Mark the project as having unsaved changes.
    pub fn has_unsaved_changes(&self) -> bool {
        // In a real implementation, this would track dirty state
        false
    }

    /// Check if the project is currently processing.
    pub fn is_processing(&self) -> bool {
        // In a real implementation, this would check processing state
        false
    }
}
