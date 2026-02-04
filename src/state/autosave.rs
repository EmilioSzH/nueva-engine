//! Autosave Manager for Nueva projects.
//!
//! Provides automatic periodic saving of project state to prevent data loss.
//! Autosaves are stored as JSON files in the backups directory and are
//! automatically rotated to prevent disk space bloat.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use walkdir::WalkDir;

use crate::state::error::{NuevaError, Result};
use crate::state::project::Project;

/// Default autosave interval in seconds.
const DEFAULT_AUTOSAVE_INTERVAL: u64 = 60;

/// Default maximum number of autosaves to retain.
const DEFAULT_MAX_AUTOSAVES: usize = 10;

/// Prefix for autosave filenames.
const AUTOSAVE_PREFIX: &str = "autosave_";

/// Extension for autosave files.
const AUTOSAVE_EXTENSION: &str = ".json";

/// Manages automatic periodic saving of project state.
#[derive(Debug, Clone)]
pub struct AutosaveManager {
    /// Interval between autosaves in seconds.
    pub autosave_interval_seconds: u64,

    /// Maximum number of autosave files to retain.
    pub max_autosaves: usize,

    /// Timestamp of the last successful autosave.
    pub last_save_time: Option<DateTime<Utc>>,
}

impl Default for AutosaveManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AutosaveManager {
    /// Create a new AutosaveManager with default settings.
    ///
    /// Default interval: 60 seconds
    /// Default max autosaves: 10
    pub fn new() -> Self {
        Self {
            autosave_interval_seconds: DEFAULT_AUTOSAVE_INTERVAL,
            max_autosaves: DEFAULT_MAX_AUTOSAVES,
            last_save_time: None,
        }
    }

    /// Create a new AutosaveManager with custom interval and max autosaves.
    ///
    /// # Arguments
    /// * `interval` - Autosave interval in seconds
    /// * `max` - Maximum number of autosave files to retain
    pub fn with_interval(interval: u64, max: usize) -> Self {
        Self {
            autosave_interval_seconds: interval,
            max_autosaves: max,
            last_save_time: None,
        }
    }

    /// Check if an autosave should be performed.
    ///
    /// Returns true if:
    /// - Enough time has elapsed since the last autosave
    /// - The project has unsaved changes
    /// - The project is not currently processing
    pub fn should_autosave(&self, project: &Project) -> bool {
        // Don't autosave if project is processing
        if project.is_processing() {
            return false;
        }

        // Don't autosave if there are no changes
        if !project.has_unsaved_changes() {
            return false;
        }

        // Check if enough time has elapsed
        match self.last_save_time {
            None => true, // Never saved, should save
            Some(last_time) => {
                let elapsed = Utc::now().signed_duration_since(last_time);
                elapsed.num_seconds() >= self.autosave_interval_seconds as i64
            }
        }
    }

    /// Perform an autosave of the project state.
    ///
    /// Saves the project JSON to the backups directory with a timestamped filename.
    /// Does NOT save audio files - only the JSON state.
    ///
    /// # Arguments
    /// * `project` - The project to save
    ///
    /// # Returns
    /// The path to the created autosave file.
    pub fn autosave(&mut self, project: &Project) -> Result<PathBuf> {
        let backups_dir = project.backups_dir();

        // Ensure backups directory exists
        if !backups_dir.exists() {
            fs::create_dir_all(&backups_dir).map_err(|e| NuevaError::DirectoryCreateError {
                path: backups_dir.clone(),
                source: e,
            })?;
        }

        // Generate filename with timestamp: autosave_YYYYMMDD_HHMMSS.json
        let now = Utc::now();
        let timestamp = now.format("%Y%m%d_%H%M%S");
        let filename = format!("{}{}{}", AUTOSAVE_PREFIX, timestamp, AUTOSAVE_EXTENSION);
        let autosave_path = backups_dir.join(&filename);

        // Serialize project to JSON
        let content = serde_json::to_string_pretty(project)?;

        // Write to file
        fs::write(&autosave_path, content).map_err(|e| NuevaError::FileWriteError {
            path: autosave_path.clone(),
            source: e,
        })?;

        // Update last save time
        self.last_save_time = Some(now);

        // Rotate old autosaves
        self.rotate_autosaves(&backups_dir)?;

        Ok(autosave_path)
    }

    /// Rotate autosaves to keep only the most recent ones.
    ///
    /// Deletes the oldest autosave files when the count exceeds max_autosaves.
    ///
    /// # Arguments
    /// * `backups_dir` - Path to the backups directory
    pub fn rotate_autosaves(&self, backups_dir: &Path) -> Result<()> {
        let mut autosaves = Self::list_autosaves(backups_dir)?;

        // If we have more autosaves than the max, delete the oldest ones
        while autosaves.len() > self.max_autosaves {
            // List is sorted newest first, so pop from the end to get oldest
            if let Some(oldest) = autosaves.pop() {
                fs::remove_file(&oldest).map_err(|e| NuevaError::FileWriteError {
                    path: oldest,
                    source: e,
                })?;
            }
        }

        Ok(())
    }

    /// List all autosave files in the backups directory.
    ///
    /// Returns files sorted by time (newest first), based on the filename timestamp.
    ///
    /// # Arguments
    /// * `backups_dir` - Path to the backups directory
    pub fn list_autosaves(backups_dir: &Path) -> Result<Vec<PathBuf>> {
        if !backups_dir.exists() {
            return Ok(Vec::new());
        }

        let mut autosaves: Vec<PathBuf> = WalkDir::new(backups_dir)
            .max_depth(1)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(AUTOSAVE_PREFIX)
                    && entry
                        .file_name()
                        .to_string_lossy()
                        .ends_with(AUTOSAVE_EXTENSION)
            })
            .map(|entry| entry.path().to_path_buf())
            .collect();

        // Sort by filename (which includes timestamp) in descending order (newest first)
        // Format: autosave_YYYYMMDD_HHMMSS.json
        autosaves.sort_by(|a, b| {
            let a_name = a.file_name().unwrap_or_default().to_string_lossy();
            let b_name = b.file_name().unwrap_or_default().to_string_lossy();
            b_name.cmp(&a_name) // Reverse order for newest first
        });

        Ok(autosaves)
    }

    /// Get the most recent autosave file.
    ///
    /// # Arguments
    /// * `backups_dir` - Path to the backups directory
    ///
    /// # Returns
    /// The path to the most recent autosave, or None if no autosaves exist.
    pub fn get_latest_autosave(backups_dir: &Path) -> Result<Option<PathBuf>> {
        let autosaves = Self::list_autosaves(backups_dir)?;
        Ok(autosaves.into_iter().next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_new_creates_defaults() {
        let manager = AutosaveManager::new();
        assert_eq!(manager.autosave_interval_seconds, DEFAULT_AUTOSAVE_INTERVAL);
        assert_eq!(manager.max_autosaves, DEFAULT_MAX_AUTOSAVES);
        assert!(manager.last_save_time.is_none());
    }

    #[test]
    fn test_with_interval_creates_custom() {
        let manager = AutosaveManager::with_interval(120, 5);
        assert_eq!(manager.autosave_interval_seconds, 120);
        assert_eq!(manager.max_autosaves, 5);
        assert!(manager.last_save_time.is_none());
    }

    #[test]
    fn test_list_autosaves_empty_dir() {
        let temp = tempdir().unwrap();
        let result = AutosaveManager::list_autosaves(temp.path());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_list_autosaves_nonexistent_dir() {
        let path = PathBuf::from("/nonexistent/path/that/does/not/exist");
        let result = AutosaveManager::list_autosaves(&path);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_list_autosaves_filters_correctly() {
        let temp = tempdir().unwrap();

        // Create some autosave files
        fs::write(temp.path().join("autosave_20240115_120000.json"), "{}").unwrap();
        fs::write(temp.path().join("autosave_20240115_130000.json"), "{}").unwrap();

        // Create some non-autosave files
        fs::write(temp.path().join("other_file.json"), "{}").unwrap();
        fs::write(temp.path().join("autosave_incomplete"), "{}").unwrap();

        let result = AutosaveManager::list_autosaves(temp.path()).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_list_autosaves_sorted_newest_first() {
        let temp = tempdir().unwrap();

        // Create autosaves with different timestamps
        fs::write(temp.path().join("autosave_20240115_100000.json"), "{}").unwrap();
        fs::write(temp.path().join("autosave_20240115_120000.json"), "{}").unwrap();
        fs::write(temp.path().join("autosave_20240115_110000.json"), "{}").unwrap();

        let result = AutosaveManager::list_autosaves(temp.path()).unwrap();
        assert_eq!(result.len(), 3);

        // Verify sorted newest first
        let names: Vec<String> = result
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        assert_eq!(names[0], "autosave_20240115_120000.json");
        assert_eq!(names[1], "autosave_20240115_110000.json");
        assert_eq!(names[2], "autosave_20240115_100000.json");
    }

    #[test]
    fn test_get_latest_autosave_returns_newest() {
        let temp = tempdir().unwrap();

        fs::write(temp.path().join("autosave_20240115_100000.json"), "{}").unwrap();
        fs::write(temp.path().join("autosave_20240115_120000.json"), "{}").unwrap();

        let result = AutosaveManager::get_latest_autosave(temp.path()).unwrap();
        assert!(result.is_some());
        assert!(result
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("120000"));
    }

    #[test]
    fn test_get_latest_autosave_empty() {
        let temp = tempdir().unwrap();
        let result = AutosaveManager::get_latest_autosave(temp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_rotate_autosaves_removes_oldest() {
        let temp = tempdir().unwrap();

        // Create 5 autosave files
        for i in 0..5 {
            fs::write(
                temp.path()
                    .join(format!("autosave_20240115_10000{}.json", i)),
                "{}",
            )
            .unwrap();
        }

        // Set max to 3
        let manager = AutosaveManager::with_interval(60, 3);
        manager.rotate_autosaves(temp.path()).unwrap();

        let remaining = AutosaveManager::list_autosaves(temp.path()).unwrap();
        assert_eq!(remaining.len(), 3);

        // Verify the newest 3 remain
        let names: Vec<String> = remaining
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        assert!(names.contains(&"autosave_20240115_100004.json".to_string()));
        assert!(names.contains(&"autosave_20240115_100003.json".to_string()));
        assert!(names.contains(&"autosave_20240115_100002.json".to_string()));
    }
}
