//! Undo/Redo System
//!
//! Provides action-based undo/redo with state snapshots per spec section 8.
//! Each action stores complete state_before and state_after snapshots
//! to enable reliable state restoration.

use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::error::{NuevaError, Result};
use crate::state::project::Project;

/// Default maximum number of undo levels to keep.
pub const DEFAULT_MAX_UNDO_LEVELS: usize = 50;

/// File name for the undo stack persistence.
const UNDO_STACK_FILE: &str = "undo_stack.json";

/// File name for the redo stack persistence.
const REDO_STACK_FILE: &str = "redo_stack.json";

/// File name for the action log persistence.
const ACTION_LOG_FILE: &str = "action_log.json";

/// Types of actions that can be undone/redone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    /// A DSP parameter or chain change (Layer 2).
    DspChange,

    /// AI/neural processing applied (Layer 1).
    AiProcessing,

    /// Bake operation flattening layers.
    Bake,

    /// Audio file import.
    Import,

    /// Project reset to initial state.
    Reset,
}

impl std::fmt::Display for ActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionType::DspChange => write!(f, "DSP Change"),
            ActionType::AiProcessing => write!(f, "AI Processing"),
            ActionType::Bake => write!(f, "Bake"),
            ActionType::Import => write!(f, "Import"),
            ActionType::Reset => write!(f, "Reset"),
        }
    }
}

/// A single undoable action with complete state snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoAction {
    /// Unique identifier for this action.
    pub id: String,

    /// Type of action performed.
    pub action_type: ActionType,

    /// Human-readable description of the action.
    pub description: String,

    /// When the action was performed.
    pub timestamp: DateTime<Utc>,

    /// Complete project state before the action.
    pub state_before: serde_json::Value,

    /// Complete project state after the action.
    pub state_after: serde_json::Value,
}

impl UndoAction {
    /// Create a new undo action with a generated UUID.
    pub fn new(
        action_type: ActionType,
        description: impl Into<String>,
        state_before: serde_json::Value,
        state_after: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            action_type,
            description: description.into(),
            timestamp: Utc::now(),
            state_before,
            state_after,
        }
    }

    /// Create an undo action with a specific ID (for testing or import).
    pub fn with_id(
        id: impl Into<String>,
        action_type: ActionType,
        description: impl Into<String>,
        state_before: serde_json::Value,
        state_after: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            action_type,
            description: description.into(),
            timestamp: Utc::now(),
            state_before,
            state_after,
        }
    }
}

/// Manages undo/redo operations for a project.
///
/// The undo manager maintains:
/// - An undo stack of recent actions (limited by max_undo_levels)
/// - A redo stack of undone actions
/// - A complete action log for history viewing
/// - A list of discarded action IDs when history is trimmed
#[derive(Debug, Clone)]
pub struct UndoManager {
    /// Stack of actions that can be undone.
    undo_stack: Vec<UndoAction>,

    /// Stack of actions that can be redone.
    redo_stack: Vec<UndoAction>,

    /// Maximum number of undo levels to keep.
    max_undo_levels: usize,

    /// Complete history of all actions (for reference/display).
    action_log: Vec<UndoAction>,

    /// IDs of actions that were discarded due to history trimming.
    discarded_action_ids: Vec<String>,
}

impl Default for UndoManager {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_UNDO_LEVELS)
    }
}

impl UndoManager {
    /// Create a new undo manager with the specified maximum undo levels.
    pub fn new(max_levels: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_undo_levels: max_levels,
            action_log: Vec::new(),
            discarded_action_ids: Vec::new(),
        }
    }

    /// Load undo manager state from the history directory.
    ///
    /// Expects files:
    /// - `history/undo_stack.json`
    /// - `history/redo_stack.json`
    /// - `history/action_log.json`
    pub fn load(history_dir: &Path) -> Result<Self> {
        let undo_stack_path = history_dir.join(UNDO_STACK_FILE);
        let redo_stack_path = history_dir.join(REDO_STACK_FILE);
        let action_log_path = history_dir.join(ACTION_LOG_FILE);

        // Load undo stack (or empty if not exists)
        let undo_stack: Vec<UndoAction> = if undo_stack_path.exists() {
            let content =
                fs::read_to_string(&undo_stack_path).map_err(|e| NuevaError::FileReadError {
                    path: undo_stack_path.clone(),
                    source: e,
                })?;
            serde_json::from_str(&content)?
        } else {
            Vec::new()
        };

        // Load redo stack (or empty if not exists)
        let redo_stack: Vec<UndoAction> = if redo_stack_path.exists() {
            let content =
                fs::read_to_string(&redo_stack_path).map_err(|e| NuevaError::FileReadError {
                    path: redo_stack_path.clone(),
                    source: e,
                })?;
            serde_json::from_str(&content)?
        } else {
            Vec::new()
        };

        // Load action log (or empty if not exists)
        let action_log: Vec<UndoAction> = if action_log_path.exists() {
            let content =
                fs::read_to_string(&action_log_path).map_err(|e| NuevaError::FileReadError {
                    path: action_log_path.clone(),
                    source: e,
                })?;
            serde_json::from_str(&content)?
        } else {
            Vec::new()
        };

        Ok(Self {
            undo_stack,
            redo_stack,
            max_undo_levels: DEFAULT_MAX_UNDO_LEVELS,
            action_log,
            discarded_action_ids: Vec::new(),
        })
    }

    /// Save undo manager state to the history directory.
    ///
    /// Creates files:
    /// - `history/undo_stack.json`
    /// - `history/redo_stack.json`
    /// - `history/action_log.json`
    pub fn save(&self, history_dir: &Path) -> Result<()> {
        // Ensure history directory exists
        if !history_dir.exists() {
            fs::create_dir_all(history_dir).map_err(|e| NuevaError::DirectoryCreateError {
                path: history_dir.to_path_buf(),
                source: e,
            })?;
        }

        // Save undo stack
        let undo_stack_path = history_dir.join(UNDO_STACK_FILE);
        let undo_content = serde_json::to_string_pretty(&self.undo_stack)?;
        fs::write(&undo_stack_path, undo_content).map_err(|e| NuevaError::FileWriteError {
            path: undo_stack_path,
            source: e,
        })?;

        // Save redo stack
        let redo_stack_path = history_dir.join(REDO_STACK_FILE);
        let redo_content = serde_json::to_string_pretty(&self.redo_stack)?;
        fs::write(&redo_stack_path, redo_content).map_err(|e| NuevaError::FileWriteError {
            path: redo_stack_path,
            source: e,
        })?;

        // Save action log
        let action_log_path = history_dir.join(ACTION_LOG_FILE);
        let log_content = serde_json::to_string_pretty(&self.action_log)?;
        fs::write(&action_log_path, log_content).map_err(|e| NuevaError::FileWriteError {
            path: action_log_path,
            source: e,
        })?;

        Ok(())
    }

    /// Push a new action onto the undo stack.
    ///
    /// This clears the redo stack (since the history has diverged)
    /// and trims the undo stack if it exceeds max_undo_levels.
    pub fn push(&mut self, action: UndoAction) {
        // Clear redo stack since we're adding a new action
        self.redo_stack.clear();

        // Add to action log
        self.action_log.push(action.clone());

        // Add to undo stack
        self.undo_stack.push(action);

        // Trim if over max
        self.trim_history();
    }

    /// Undo the last action, restoring the project to its previous state.
    ///
    /// Returns the undone action on success.
    pub fn undo(&mut self, project: &mut Project) -> Result<UndoAction> {
        let action = self.undo_stack.pop().ok_or(NuevaError::NothingToUndo)?;

        // Restore project state from state_before
        let restored_project: Project = serde_json::from_value(action.state_before.clone())?;

        // Copy all serializable fields to the existing project
        project.schema_version = restored_project.schema_version;
        project.created_at = restored_project.created_at;
        project.modified_at = restored_project.modified_at;
        project.nueva_version = restored_project.nueva_version;
        project.source = restored_project.source;
        project.layer0 = restored_project.layer0;
        project.layer1 = restored_project.layer1;
        project.layer2 = restored_project.layer2;
        project.conversation = restored_project.conversation;
        project.unknown_fields = restored_project.unknown_fields;
        // Note: project_path is not serialized, so it's preserved

        // Move action to redo stack
        self.redo_stack.push(action.clone());

        Ok(action)
    }

    /// Redo the last undone action, restoring the project to the state after the action.
    ///
    /// Returns the redone action on success.
    pub fn redo(&mut self, project: &mut Project) -> Result<UndoAction> {
        let action = self.redo_stack.pop().ok_or(NuevaError::NothingToRedo)?;

        // Restore project state from state_after
        let restored_project: Project = serde_json::from_value(action.state_after.clone())?;

        // Copy all serializable fields to the existing project
        project.schema_version = restored_project.schema_version;
        project.created_at = restored_project.created_at;
        project.modified_at = restored_project.modified_at;
        project.nueva_version = restored_project.nueva_version;
        project.source = restored_project.source;
        project.layer0 = restored_project.layer0;
        project.layer1 = restored_project.layer1;
        project.layer2 = restored_project.layer2;
        project.conversation = restored_project.conversation;
        project.unknown_fields = restored_project.unknown_fields;
        // Note: project_path is not serialized, so it's preserved

        // Move action back to undo stack
        self.undo_stack.push(action.clone());

        Ok(action)
    }

    /// Get the complete action history log.
    pub fn get_history(&self) -> &[UndoAction] {
        &self.action_log
    }

    /// Get the current position in the undo history.
    ///
    /// Returns the number of actions that have been performed (undo stack size).
    /// Position 0 means at the beginning (nothing to undo).
    pub fn current_position(&self) -> usize {
        self.undo_stack.len()
    }

    /// Get the number of actions that can be undone.
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Get the number of actions that can be redone.
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }

    /// Trim the undo stack to the maximum allowed levels.
    ///
    /// When the stack exceeds max_undo_levels, the oldest actions are removed
    /// and their IDs are tracked in discarded_action_ids.
    pub fn trim_history(&mut self) {
        while self.undo_stack.len() > self.max_undo_levels {
            if let Some(removed) = self.undo_stack.first().cloned() {
                self.discarded_action_ids.push(removed.id);
                self.undo_stack.remove(0);
            }
        }
    }

    /// Get the maximum number of undo levels.
    pub fn max_undo_levels(&self) -> usize {
        self.max_undo_levels
    }

    /// Set the maximum number of undo levels.
    ///
    /// If the new limit is lower than the current stack size, the stack will be trimmed.
    pub fn set_max_undo_levels(&mut self, max_levels: usize) {
        self.max_undo_levels = max_levels;
        self.trim_history();
    }

    /// Get the IDs of actions that were discarded due to history trimming.
    pub fn discarded_action_ids(&self) -> &[String] {
        &self.discarded_action_ids
    }

    /// Check if there are actions that can be undone.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if there are actions that can be redone.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Get the most recent action that can be undone (if any).
    pub fn peek_undo(&self) -> Option<&UndoAction> {
        self.undo_stack.last()
    }

    /// Get the most recent action that can be redone (if any).
    pub fn peek_redo(&self) -> Option<&UndoAction> {
        self.redo_stack.last()
    }

    /// Clear all undo/redo history.
    ///
    /// This is typically called after a bake operation or when starting fresh.
    pub fn clear(&mut self) {
        // Track all discarded actions
        for action in &self.undo_stack {
            self.discarded_action_ids.push(action.id.clone());
        }
        for action in &self.redo_stack {
            self.discarded_action_ids.push(action.id.clone());
        }

        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Get a summary of the undo stack for display.
    pub fn undo_stack_summary(&self) -> Vec<(String, ActionType, String)> {
        self.undo_stack
            .iter()
            .rev() // Most recent first
            .map(|a| (a.id.clone(), a.action_type, a.description.clone()))
            .collect()
    }

    /// Get a summary of the redo stack for display.
    pub fn redo_stack_summary(&self) -> Vec<(String, ActionType, String)> {
        self.redo_stack
            .iter()
            .rev() // Most recently undone first
            .map(|a| (a.id.clone(), a.action_type, a.description.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_state(name: &str) -> serde_json::Value {
        serde_json::json!({
            "schema_version": "1.0.0",
            "created_at": "2024-01-01T00:00:00Z",
            "modified_at": "2024-01-01T00:00:00Z",
            "nueva_version": "0.1.0",
            "source": {
                "original_filename": name,
                "original_path": format!("/test/{}", name),
                "import_settings": {}
            },
            "layer0": {
                "path": "audio/layer0_source.wav",
                "sample_rate": 48000,
                "bit_depth": 32,
                "channels": 2,
                "duration_seconds": 10.0,
                "hash_sha256": "abc123"
            },
            "layer1": {
                "path": "audio/layer1_ai.wav",
                "is_processed": false,
                "identical_to_layer0": true
            },
            "layer2": {
                "chain": []
            },
            "conversation": {
                "session_count": 0,
                "total_messages": 0,
                "user_preferences": {
                    "prefers_dsp_first": false
                }
            }
        })
    }

    #[test]
    fn test_new_undo_manager() {
        let manager = UndoManager::new(10);
        assert_eq!(manager.max_undo_levels(), 10);
        assert_eq!(manager.undo_count(), 0);
        assert_eq!(manager.redo_count(), 0);
        assert!(!manager.can_undo());
        assert!(!manager.can_redo());
    }

    #[test]
    fn test_push_action() {
        let mut manager = UndoManager::new(10);

        let action = UndoAction::new(
            ActionType::DspChange,
            "Add EQ",
            create_test_state("before"),
            create_test_state("after"),
        );

        manager.push(action);

        assert_eq!(manager.undo_count(), 1);
        assert_eq!(manager.redo_count(), 0);
        assert!(manager.can_undo());
        assert!(!manager.can_redo());
    }

    #[test]
    fn test_trim_history() {
        let mut manager = UndoManager::new(3);

        for i in 0..5 {
            let action = UndoAction::new(
                ActionType::DspChange,
                format!("Action {}", i),
                create_test_state("before"),
                create_test_state("after"),
            );
            manager.push(action);
        }

        assert_eq!(manager.undo_count(), 3);
        assert_eq!(manager.discarded_action_ids().len(), 2);
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let history_dir = temp_dir.path();

        let mut manager = UndoManager::new(10);
        let action = UndoAction::new(
            ActionType::AiProcessing,
            "Apply style transfer",
            create_test_state("before"),
            create_test_state("after"),
        );
        manager.push(action);

        manager.save(history_dir).unwrap();

        let loaded = UndoManager::load(history_dir).unwrap();
        assert_eq!(loaded.undo_count(), 1);
        assert_eq!(loaded.get_history().len(), 1);
    }

    #[test]
    fn test_action_type_display() {
        assert_eq!(ActionType::DspChange.to_string(), "DSP Change");
        assert_eq!(ActionType::AiProcessing.to_string(), "AI Processing");
        assert_eq!(ActionType::Bake.to_string(), "Bake");
        assert_eq!(ActionType::Import.to_string(), "Import");
        assert_eq!(ActionType::Reset.to_string(), "Reset");
    }

    #[test]
    fn test_push_clears_redo_stack() {
        let mut manager = UndoManager::new(10);

        // Add some actions
        for i in 0..3 {
            let action = UndoAction::new(
                ActionType::DspChange,
                format!("Action {}", i),
                create_test_state("before"),
                create_test_state("after"),
            );
            manager.push(action);
        }

        // Simulate undo by moving to redo stack manually
        if let Some(action) = manager.undo_stack.pop() {
            manager.redo_stack.push(action);
        }

        assert_eq!(manager.redo_count(), 1);

        // Push new action should clear redo stack
        let new_action = UndoAction::new(
            ActionType::DspChange,
            "New action",
            create_test_state("before"),
            create_test_state("after"),
        );
        manager.push(new_action);

        assert_eq!(manager.redo_count(), 0);
    }

    #[test]
    fn test_peek_methods() {
        let mut manager = UndoManager::new(10);

        assert!(manager.peek_undo().is_none());
        assert!(manager.peek_redo().is_none());

        let action = UndoAction::with_id(
            "test-id",
            ActionType::Import,
            "Import audio",
            create_test_state("before"),
            create_test_state("after"),
        );
        manager.push(action);

        let peeked = manager.peek_undo().unwrap();
        assert_eq!(peeked.id, "test-id");
        assert_eq!(peeked.action_type, ActionType::Import);
    }

    #[test]
    fn test_clear() {
        let mut manager = UndoManager::new(10);

        for i in 0..3 {
            let action = UndoAction::new(
                ActionType::DspChange,
                format!("Action {}", i),
                create_test_state("before"),
                create_test_state("after"),
            );
            manager.push(action);
        }

        // Move one to redo
        if let Some(action) = manager.undo_stack.pop() {
            manager.redo_stack.push(action);
        }

        manager.clear();

        assert_eq!(manager.undo_count(), 0);
        assert_eq!(manager.redo_count(), 0);
        assert_eq!(manager.discarded_action_ids().len(), 3);
    }

    #[test]
    fn test_current_position() {
        let mut manager = UndoManager::new(10);

        assert_eq!(manager.current_position(), 0);

        for i in 0..3 {
            let action = UndoAction::new(
                ActionType::DspChange,
                format!("Action {}", i),
                create_test_state("before"),
                create_test_state("after"),
            );
            manager.push(action);
        }

        assert_eq!(manager.current_position(), 3);
    }

    #[test]
    fn test_set_max_undo_levels() {
        let mut manager = UndoManager::new(10);

        for i in 0..5 {
            let action = UndoAction::new(
                ActionType::DspChange,
                format!("Action {}", i),
                create_test_state("before"),
                create_test_state("after"),
            );
            manager.push(action);
        }

        assert_eq!(manager.undo_count(), 5);

        manager.set_max_undo_levels(3);

        assert_eq!(manager.undo_count(), 3);
        assert_eq!(manager.max_undo_levels(), 3);
    }

    #[test]
    fn test_stack_summaries() {
        let mut manager = UndoManager::new(10);

        let action1 = UndoAction::with_id(
            "id-1",
            ActionType::Import,
            "Import audio.wav",
            create_test_state("before"),
            create_test_state("after"),
        );
        let action2 = UndoAction::with_id(
            "id-2",
            ActionType::DspChange,
            "Add compressor",
            create_test_state("before"),
            create_test_state("after"),
        );

        manager.push(action1);
        manager.push(action2);

        let undo_summary = manager.undo_stack_summary();
        assert_eq!(undo_summary.len(), 2);

        // Most recent first
        assert_eq!(undo_summary[0].0, "id-2");
        assert_eq!(undo_summary[0].1, ActionType::DspChange);
        assert_eq!(undo_summary[0].2, "Add compressor");

        assert_eq!(undo_summary[1].0, "id-1");
        assert_eq!(undo_summary[1].1, ActionType::Import);
    }
}
