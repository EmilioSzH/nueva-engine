//! Crash Recovery Module
//!
//! Provides crash detection and recovery functionality for Nueva projects.
//! Detects potential crashes via lock files and recovers from autosaves.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::state::error::{NuevaError, Result};
use crate::state::project::{BACKUPS_DIR, LOCK_FILE, PROJECT_FILE};

/// Result of a crash recovery check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryResult {
    /// Whether recovery is needed (lock file exists).
    pub needed: bool,

    /// Whether a recovery state was found (autosave exists).
    pub success: bool,

    /// User-facing message describing the recovery status.
    pub message: String,

    /// Path to the autosave file to recover from, if found.
    pub recovery_state_path: Option<PathBuf>,
}

impl RecoveryResult {
    /// Create a result indicating no recovery is needed.
    fn no_recovery_needed() -> Self {
        Self {
            needed: false,
            success: false,
            message: "No recovery needed. Project closed cleanly.".to_string(),
            recovery_state_path: None,
        }
    }

    /// Create a result indicating recovery is needed but no autosave found.
    fn recovery_needed_no_autosave() -> Self {
        Self {
            needed: true,
            success: false,
            message: "Warning: Project may have crashed, but no autosave was found. \
                      The project state may be incomplete or corrupted."
                .to_string(),
            recovery_state_path: None,
        }
    }

    /// Create a result indicating recovery is needed and autosave was found.
    fn recovery_available(autosave_path: PathBuf, timestamp: DateTime<Utc>) -> Self {
        Self {
            needed: true,
            success: true,
            message: format!(
                "Recovery available from autosave at {}. \
                 Use 'apply_recovery' to restore this state.",
                timestamp.format("%Y-%m-%d %H:%M:%S UTC")
            ),
            recovery_state_path: Some(autosave_path),
        }
    }
}

/// Lock file content structure.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LockFileContent {
    /// Process ID that created the lock.
    #[allow(dead_code)]
    pid: u32,

    /// Timestamp when the lock was created.
    #[allow(dead_code)]
    started_at: String,
}

/// Check if a project needs crash recovery and find available autosaves.
///
/// This function checks for the presence of a lock file, which indicates
/// the project may not have been closed cleanly (potential crash).
///
/// # Arguments
///
/// * `project_path` - Path to the project directory
///
/// # Returns
///
/// A `RecoveryResult` indicating whether recovery is needed and available.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use nueva::state::crash_recovery::recover_from_crash;
///
/// let result = recover_from_crash(Path::new("/path/to/project")).unwrap();
/// if result.needed && result.success {
///     println!("Recovery available: {}", result.message);
/// }
/// ```
pub fn recover_from_crash(project_path: &Path) -> Result<RecoveryResult> {
    let lock_path = project_path.join(LOCK_FILE);

    // Check if lock file exists
    if !lock_path.exists() {
        return Ok(RecoveryResult::no_recovery_needed());
    }

    // Lock file exists - potential crash detected
    // Try to find the most recent autosave
    let backups_dir = project_path.join(BACKUPS_DIR);

    if !backups_dir.exists() {
        return Ok(RecoveryResult::recovery_needed_no_autosave());
    }

    // Find all autosave files
    let autosaves = find_autosave_files(&backups_dir)?;

    if autosaves.is_empty() {
        return Ok(RecoveryResult::recovery_needed_no_autosave());
    }

    // Find the most recent autosave by parsing timestamps
    let mut most_recent: Option<(PathBuf, DateTime<Utc>)> = None;

    for autosave_path in autosaves {
        if let Some(timestamp) = parse_timestamp_from_filename(&autosave_path) {
            match &most_recent {
                None => {
                    most_recent = Some((autosave_path, timestamp));
                }
                Some((_, existing_ts)) if timestamp > *existing_ts => {
                    most_recent = Some((autosave_path, timestamp));
                }
                _ => {}
            }
        }
    }

    match most_recent {
        Some((path, timestamp)) => Ok(RecoveryResult::recovery_available(path, timestamp)),
        None => Ok(RecoveryResult::recovery_needed_no_autosave()),
    }
}

/// Find all autosave files in the backups directory.
///
/// Autosave files follow the pattern: `autosave_YYYYMMDD_HHMMSS.json`
fn find_autosave_files(backups_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut autosaves = Vec::new();

    let entries = fs::read_dir(backups_dir).map_err(|e| NuevaError::FileReadError {
        path: backups_dir.to_path_buf(),
        source: e,
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| NuevaError::FileReadError {
            path: backups_dir.to_path_buf(),
            source: e,
        })?;

        let path = entry.path();

        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            // Match pattern: autosave_YYYYMMDD_HHMMSS.json
            if filename.starts_with("autosave_") && filename.ends_with(".json") {
                autosaves.push(path);
            }
        }
    }

    Ok(autosaves)
}

/// Parse a timestamp from an autosave filename.
///
/// Expected format: `autosave_YYYYMMDD_HHMMSS.json`
///
/// # Arguments
///
/// * `path` - Path to the autosave file
///
/// # Returns
///
/// The parsed timestamp as `DateTime<Utc>`, or `None` if parsing fails.
///
/// # Example
///
/// ```
/// use std::path::Path;
/// use nueva::state::crash_recovery::parse_timestamp_from_filename;
///
/// let path = Path::new("autosave_20240115_143022.json");
/// let timestamp = parse_timestamp_from_filename(path);
/// assert!(timestamp.is_some());
/// ```
pub fn parse_timestamp_from_filename(path: &Path) -> Option<DateTime<Utc>> {
    let filename = path.file_stem()?.to_str()?;

    // Expected format: autosave_YYYYMMDD_HHMMSS
    if !filename.starts_with("autosave_") {
        return None;
    }

    // Extract the timestamp portion: YYYYMMDD_HHMMSS
    let timestamp_str = filename.strip_prefix("autosave_")?;

    // Parse: YYYYMMDD_HHMMSS
    let naive = NaiveDateTime::parse_from_str(timestamp_str, "%Y%m%d_%H%M%S").ok()?;

    Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

/// Apply recovery by restoring from an autosave file.
///
/// This function:
/// 1. Loads the autosave JSON
/// 2. Writes it to project.json
/// 3. Removes the lock file
///
/// # Arguments
///
/// * `project_path` - Path to the project directory
/// * `autosave_path` - Path to the autosave file to recover from
///
/// # Returns
///
/// `Ok(())` on success, or an error if recovery fails.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use nueva::state::crash_recovery::{recover_from_crash, apply_recovery};
///
/// let project_path = Path::new("/path/to/project");
/// let result = recover_from_crash(project_path).unwrap();
///
/// if result.needed && result.success {
///     if let Some(autosave_path) = result.recovery_state_path {
///         apply_recovery(project_path, &autosave_path).unwrap();
///     }
/// }
/// ```
pub fn apply_recovery(project_path: &Path, autosave_path: &Path) -> Result<()> {
    // Validate autosave exists
    if !autosave_path.exists() {
        return Err(NuevaError::FileNotFound {
            path: autosave_path.to_path_buf(),
        });
    }

    // Load autosave JSON
    let autosave_content =
        fs::read_to_string(autosave_path).map_err(|e| NuevaError::FileReadError {
            path: autosave_path.to_path_buf(),
            source: e,
        })?;

    // Validate JSON is parseable
    let _: serde_json::Value =
        serde_json::from_str(&autosave_content).map_err(|e| NuevaError::RecoveryFailed {
            reason: format!("Invalid autosave JSON: {}", e),
        })?;

    // Write to project.json
    let project_file = project_path.join(PROJECT_FILE);
    fs::write(&project_file, &autosave_content).map_err(|e| NuevaError::FileWriteError {
        path: project_file,
        source: e,
    })?;

    // Remove the lock file
    let lock_path = project_path.join(LOCK_FILE);
    if lock_path.exists() {
        fs::remove_file(&lock_path).map_err(|e| NuevaError::FileWriteError {
            path: lock_path,
            source: e,
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_project() -> TempDir {
        let temp = TempDir::new().unwrap();
        let backups = temp.path().join(BACKUPS_DIR);
        fs::create_dir_all(&backups).unwrap();
        temp
    }

    #[test]
    fn test_no_recovery_needed_without_lock() {
        let temp = create_test_project();
        let result = recover_from_crash(temp.path()).unwrap();

        assert!(!result.needed);
        assert!(!result.success);
        assert!(result.recovery_state_path.is_none());
    }

    #[test]
    fn test_recovery_needed_with_lock_no_autosave() {
        let temp = create_test_project();
        let lock_path = temp.path().join(LOCK_FILE);
        fs::write(
            &lock_path,
            r#"{"pid": 1234, "started_at": "2024-01-15T10:00:00Z"}"#,
        )
        .unwrap();

        let result = recover_from_crash(temp.path()).unwrap();

        assert!(result.needed);
        assert!(!result.success);
        assert!(result.recovery_state_path.is_none());
        assert!(result.message.contains("Warning"));
    }

    #[test]
    fn test_recovery_available_with_autosave() {
        let temp = create_test_project();
        let lock_path = temp.path().join(LOCK_FILE);
        fs::write(
            &lock_path,
            r#"{"pid": 1234, "started_at": "2024-01-15T10:00:00Z"}"#,
        )
        .unwrap();

        let backups = temp.path().join(BACKUPS_DIR);
        let autosave_path = backups.join("autosave_20240115_143022.json");
        fs::write(&autosave_path, r#"{"schema_version": "1.0"}"#).unwrap();

        let result = recover_from_crash(temp.path()).unwrap();

        assert!(result.needed);
        assert!(result.success);
        assert!(result.recovery_state_path.is_some());
        assert_eq!(result.recovery_state_path.unwrap(), autosave_path);
    }

    #[test]
    fn test_most_recent_autosave_selected() {
        let temp = create_test_project();
        let lock_path = temp.path().join(LOCK_FILE);
        fs::write(
            &lock_path,
            r#"{"pid": 1234, "started_at": "2024-01-15T10:00:00Z"}"#,
        )
        .unwrap();

        let backups = temp.path().join(BACKUPS_DIR);

        // Create older autosave
        let older_path = backups.join("autosave_20240115_100000.json");
        fs::write(&older_path, r#"{"schema_version": "1.0"}"#).unwrap();

        // Create newer autosave
        let newer_path = backups.join("autosave_20240115_150000.json");
        fs::write(&newer_path, r#"{"schema_version": "1.0"}"#).unwrap();

        let result = recover_from_crash(temp.path()).unwrap();

        assert!(result.needed);
        assert!(result.success);
        assert_eq!(result.recovery_state_path.unwrap(), newer_path);
    }

    #[test]
    fn test_parse_timestamp_from_filename() {
        let path = Path::new("autosave_20240115_143022.json");
        let timestamp = parse_timestamp_from_filename(path);

        assert!(timestamp.is_some());
        let ts = timestamp.unwrap();
        assert_eq!(ts.format("%Y%m%d_%H%M%S").to_string(), "20240115_143022");
    }

    #[test]
    fn test_parse_timestamp_invalid_format() {
        let path = Path::new("not_an_autosave.json");
        assert!(parse_timestamp_from_filename(path).is_none());

        let path = Path::new("autosave_invalid.json");
        assert!(parse_timestamp_from_filename(path).is_none());
    }

    #[test]
    fn test_apply_recovery() {
        let temp = create_test_project();

        // Create lock file
        let lock_path = temp.path().join(LOCK_FILE);
        fs::write(
            &lock_path,
            r#"{"pid": 1234, "started_at": "2024-01-15T10:00:00Z"}"#,
        )
        .unwrap();

        // Create autosave
        let backups = temp.path().join(BACKUPS_DIR);
        let autosave_path = backups.join("autosave_20240115_143022.json");
        let autosave_content = r#"{"schema_version": "1.0", "test": "data"}"#;
        fs::write(&autosave_path, autosave_content).unwrap();

        // Apply recovery
        apply_recovery(temp.path(), &autosave_path).unwrap();

        // Verify project.json was created
        let project_file = temp.path().join(PROJECT_FILE);
        assert!(project_file.exists());

        let content = fs::read_to_string(&project_file).unwrap();
        assert_eq!(content, autosave_content);

        // Verify lock file was removed
        assert!(!lock_path.exists());
    }

    #[test]
    fn test_apply_recovery_missing_autosave() {
        let temp = create_test_project();
        let fake_autosave = temp.path().join("nonexistent.json");

        let result = apply_recovery(temp.path(), &fake_autosave);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_recovery_invalid_json() {
        let temp = create_test_project();

        let autosave_path = temp
            .path()
            .join(BACKUPS_DIR)
            .join("autosave_20240115_143022.json");
        fs::write(&autosave_path, "not valid json").unwrap();

        let result = apply_recovery(temp.path(), &autosave_path);
        assert!(result.is_err());
    }
}
