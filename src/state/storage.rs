//! Layer 1 Storage Management
//!
//! Manages storage for Layer 1 audio files, including tracking file metadata,
//! pruning orphaned files, and monitoring storage usage.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::state::error::{NuevaError, Result};
use crate::state::project::Project;

/// Storage usage statistics.
#[derive(Debug, Clone)]
pub struct StorageUsage {
    /// Number of Layer 1 files.
    pub file_count: usize,
    /// Total size in bytes.
    pub total_size_bytes: u64,
    /// Total size in megabytes.
    pub total_size_mb: f64,
}

/// Metadata for a single Layer 1 file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer1FileInfo {
    /// When the file was created.
    pub created_at: DateTime<Utc>,
    /// The undo action ID that created this file.
    pub undo_action_id: String,
    /// File size in bytes.
    pub size_bytes: u64,
}

/// Manifest tracking all Layer 1 files.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Layer1Manifest {
    /// Map of filename to file info.
    pub files: HashMap<String, Layer1FileInfo>,
}

/// Manages Layer 1 storage for a project.
pub struct Layer1StorageManager {
    /// Path to the project directory.
    project_path: PathBuf,
    /// Path to the Layer 1 audio directory.
    audio_dir: PathBuf,
}

impl Layer1StorageManager {
    /// Create a new Layer 1 storage manager.
    pub fn new(project_path: &Path) -> Self {
        let audio_dir = project_path.join("audio").join("layer1");
        Self {
            project_path: project_path.to_path_buf(),
            audio_dir,
        }
    }

    /// Get the project path.
    pub fn project_path(&self) -> &Path {
        &self.project_path
    }

    /// Get the audio directory path.
    pub fn audio_dir(&self) -> &Path {
        &self.audio_dir
    }

    /// Get the path to the manifest file.
    fn manifest_path(&self) -> PathBuf {
        self.audio_dir.join("manifest.json")
    }

    /// Load the manifest from disk.
    pub fn load_manifest(&self) -> Result<Layer1Manifest> {
        let manifest_path = self.manifest_path();

        if !manifest_path.exists() {
            return Ok(Layer1Manifest::default());
        }

        let content =
            fs::read_to_string(&manifest_path).map_err(|e| NuevaError::FileReadError {
                path: manifest_path.clone(),
                source: e,
            })?;

        let manifest: Layer1Manifest = serde_json::from_str(&content)?;
        Ok(manifest)
    }

    /// Save the manifest to disk.
    pub fn save_manifest(&self, manifest: &Layer1Manifest) -> Result<()> {
        // Ensure the audio/layer1 directory exists
        if !self.audio_dir.exists() {
            fs::create_dir_all(&self.audio_dir).map_err(|e| NuevaError::DirectoryCreateError {
                path: self.audio_dir.clone(),
                source: e,
            })?;
        }

        let manifest_path = self.manifest_path();
        let content = serde_json::to_string_pretty(manifest)?;

        fs::write(&manifest_path, content).map_err(|e| NuevaError::FileWriteError {
            path: manifest_path,
            source: e,
        })?;

        Ok(())
    }

    /// Record a new Layer 1 file in the manifest.
    pub fn record_new_layer1(&self, audio_path: &Path, undo_action_id: &str) -> Result<()> {
        let mut manifest = self.load_manifest()?;

        // Get the filename
        let filename = audio_path
            .file_name()
            .ok_or_else(|| NuevaError::InvalidProjectPath {
                path: audio_path.to_path_buf(),
            })?
            .to_string_lossy()
            .to_string();

        // Get file size
        let metadata = fs::metadata(audio_path).map_err(|e| NuevaError::FileReadError {
            path: audio_path.to_path_buf(),
            source: e,
        })?;

        let file_info = Layer1FileInfo {
            created_at: Utc::now(),
            undo_action_id: undo_action_id.to_string(),
            size_bytes: metadata.len(),
        };

        manifest.files.insert(filename, file_info);
        self.save_manifest(&manifest)?;

        Ok(())
    }

    /// Prune orphaned Layer 1 files that are not in the reachable action IDs set.
    ///
    /// Returns the total bytes freed.
    pub fn prune_orphaned_files(&self, reachable_action_ids: &HashSet<String>) -> Result<u64> {
        let mut manifest = self.load_manifest()?;
        let mut bytes_freed: u64 = 0;
        let mut files_to_remove: Vec<String> = Vec::new();

        // Find orphaned files
        for (filename, file_info) in &manifest.files {
            if !reachable_action_ids.contains(&file_info.undo_action_id) {
                files_to_remove.push(filename.clone());
            }
        }

        // Delete orphaned files
        for filename in &files_to_remove {
            let file_path = self.audio_dir.join(filename);

            if file_path.exists() {
                // Get file size before deleting
                if let Ok(metadata) = fs::metadata(&file_path) {
                    bytes_freed += metadata.len();
                }

                // Delete the file
                fs::remove_file(&file_path).map_err(|e| NuevaError::FileWriteError {
                    path: file_path,
                    source: e,
                })?;
            }

            // Remove from manifest
            if let Some(file_info) = manifest.files.remove(filename) {
                // If we couldn't get the size from disk, use manifest value
                if bytes_freed == 0 {
                    bytes_freed += file_info.size_bytes;
                }
            }
        }

        // Save updated manifest
        self.save_manifest(&manifest)?;

        Ok(bytes_freed)
    }

    /// Calculate current storage usage for Layer 1 files.
    pub fn get_storage_usage(&self) -> Result<StorageUsage> {
        let manifest = self.load_manifest()?;

        let file_count = manifest.files.len();
        let total_size_bytes: u64 = manifest.files.values().map(|f| f.size_bytes).sum();
        let total_size_mb = total_size_bytes as f64 / (1024.0 * 1024.0);

        Ok(StorageUsage {
            file_count,
            total_size_bytes,
            total_size_mb,
        })
    }
}

/// Check storage health and return warnings.
///
/// Checks:
/// - If Layer 1 usage exceeds 1GB, adds a warning
/// - If more than 10 Layer 1 files exist, suggests pruning history
pub fn check_storage_health(project: &Project) -> Result<Vec<String>> {
    let mut warnings: Vec<String> = Vec::new();

    let manager = Layer1StorageManager::new(&project.project_path);
    let usage = manager.get_storage_usage()?;

    // Check if Layer 1 usage exceeds 1GB (1024 MB)
    if usage.total_size_mb > 1024.0 {
        warnings.push(format!(
            "Layer 1 storage usage is {:.1} MB (exceeds 1 GB). Consider pruning history or baking to reduce storage.",
            usage.total_size_mb
        ));
    }

    // Check if more than 10 Layer 1 files exist
    if usage.file_count > 10 {
        warnings.push(format!(
            "Layer 1 has {} files. Consider pruning history to free up disk space.",
            usage.file_count
        ));
    }

    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_project_path() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_storage_manager_new() {
        let temp_dir = create_test_project_path();
        let manager = Layer1StorageManager::new(temp_dir.path());

        assert_eq!(manager.project_path, temp_dir.path());
        assert_eq!(
            manager.audio_dir,
            temp_dir.path().join("audio").join("layer1")
        );
    }

    #[test]
    fn test_load_empty_manifest() {
        let temp_dir = create_test_project_path();
        let manager = Layer1StorageManager::new(temp_dir.path());

        let manifest = manager.load_manifest().unwrap();
        assert!(manifest.files.is_empty());
    }

    #[test]
    fn test_save_and_load_manifest() {
        let temp_dir = create_test_project_path();
        let manager = Layer1StorageManager::new(temp_dir.path());

        let mut manifest = Layer1Manifest::default();
        manifest.files.insert(
            "test_file.wav".to_string(),
            Layer1FileInfo {
                created_at: Utc::now(),
                undo_action_id: "action-123".to_string(),
                size_bytes: 1024,
            },
        );

        manager.save_manifest(&manifest).unwrap();
        let loaded = manager.load_manifest().unwrap();

        assert_eq!(loaded.files.len(), 1);
        assert!(loaded.files.contains_key("test_file.wav"));
        assert_eq!(loaded.files["test_file.wav"].undo_action_id, "action-123");
    }

    #[test]
    fn test_record_new_layer1() {
        let temp_dir = create_test_project_path();
        let manager = Layer1StorageManager::new(temp_dir.path());

        // Create the audio directory
        fs::create_dir_all(&manager.audio_dir).unwrap();

        // Create a test audio file
        let test_file = manager.audio_dir.join("test_audio.wav");
        fs::write(&test_file, vec![0u8; 2048]).unwrap();

        // Record the file
        manager.record_new_layer1(&test_file, "action-456").unwrap();

        // Verify it was recorded
        let manifest = manager.load_manifest().unwrap();
        assert_eq!(manifest.files.len(), 1);
        assert!(manifest.files.contains_key("test_audio.wav"));
        assert_eq!(manifest.files["test_audio.wav"].size_bytes, 2048);
    }

    #[test]
    fn test_prune_orphaned_files() {
        let temp_dir = create_test_project_path();
        let manager = Layer1StorageManager::new(temp_dir.path());

        // Create the audio directory
        fs::create_dir_all(&manager.audio_dir).unwrap();

        // Create test files
        let file1 = manager.audio_dir.join("keep_me.wav");
        let file2 = manager.audio_dir.join("delete_me.wav");
        fs::write(&file1, vec![0u8; 1024]).unwrap();
        fs::write(&file2, vec![0u8; 2048]).unwrap();

        // Record both files
        manager.record_new_layer1(&file1, "action-keep").unwrap();
        manager.record_new_layer1(&file2, "action-delete").unwrap();

        // Prune with only "action-keep" reachable
        let mut reachable = HashSet::new();
        reachable.insert("action-keep".to_string());

        let bytes_freed = manager.prune_orphaned_files(&reachable).unwrap();

        assert_eq!(bytes_freed, 2048);
        assert!(file1.exists());
        assert!(!file2.exists());

        let manifest = manager.load_manifest().unwrap();
        assert_eq!(manifest.files.len(), 1);
        assert!(manifest.files.contains_key("keep_me.wav"));
    }

    #[test]
    fn test_get_storage_usage() {
        let temp_dir = create_test_project_path();
        let manager = Layer1StorageManager::new(temp_dir.path());

        // Create the audio directory
        fs::create_dir_all(&manager.audio_dir).unwrap();

        // Create test files
        let file1 = manager.audio_dir.join("file1.wav");
        let file2 = manager.audio_dir.join("file2.wav");
        fs::write(&file1, vec![0u8; 1024]).unwrap();
        fs::write(&file2, vec![0u8; 2048]).unwrap();

        // Record the files
        manager.record_new_layer1(&file1, "action-1").unwrap();
        manager.record_new_layer1(&file2, "action-2").unwrap();

        let usage = manager.get_storage_usage().unwrap();
        assert_eq!(usage.file_count, 2);
        assert_eq!(usage.total_size_bytes, 3072);
        assert!((usage.total_size_mb - 0.00293).abs() < 0.001);
    }
}
