//! Undo/Redo management
//!
//! Every agent action is undoable.
//! Implements §7.4 from the spec.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Maximum number of undo levels
pub const MAX_UNDO_LEVELS: usize = 50;

/// An action that can be undone
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoableAction {
    /// Unique action ID
    pub action_id: String,

    /// When the action was performed
    pub timestamp: DateTime<Utc>,

    /// Human-readable description
    pub description: String,

    /// DSP chain state before the action
    pub dsp_chain_before: Vec<EffectState>,

    /// DSP chain state after the action
    pub dsp_chain_after: Vec<EffectState>,

    /// Layer 1 path before (for neural actions)
    pub layer1_path_before: Option<String>,

    /// Layer 1 path after (for neural actions)
    pub layer1_path_after: Option<String>,
}

impl UndoableAction {
    /// Create a new undoable action
    pub fn new(description: &str) -> Self {
        Self {
            action_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            description: description.to_string(),
            dsp_chain_before: Vec::new(),
            dsp_chain_after: Vec::new(),
            layer1_path_before: None,
            layer1_path_after: None,
        }
    }

    /// Set DSP chain states
    pub fn with_dsp_states(
        mut self,
        before: Vec<EffectState>,
        after: Vec<EffectState>,
    ) -> Self {
        self.dsp_chain_before = before;
        self.dsp_chain_after = after;
        self
    }

    /// Set Layer 1 paths for neural actions
    pub fn with_layer1_paths(
        mut self,
        before: Option<String>,
        after: Option<String>,
    ) -> Self {
        self.layer1_path_before = before;
        self.layer1_path_after = after;
        self
    }

    /// Check if this is a neural action (modifies Layer 1)
    pub fn is_neural_action(&self) -> bool {
        self.layer1_path_after.is_some()
    }
}

/// Serializable effect state for undo/redo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectState {
    /// Effect ID
    pub id: String,

    /// Effect type
    pub effect_type: String,

    /// Whether enabled
    pub enabled: bool,

    /// Effect parameters
    pub params: HashMap<String, serde_json::Value>,
}

/// Manages undo/redo stacks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoManager {
    /// Stack of actions that can be undone
    undo_stack: Vec<UndoableAction>,

    /// Stack of actions that can be redone
    redo_stack: Vec<UndoableAction>,

    /// Maximum undo levels
    max_undo_levels: usize,
}

impl UndoManager {
    /// Create a new undo manager
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_undo_levels: MAX_UNDO_LEVELS,
        }
    }

    /// Create with custom max levels
    pub fn with_max_levels(max_levels: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_undo_levels: max_levels,
        }
    }

    /// Record a new action (clears redo stack)
    pub fn record_action(&mut self, action: UndoableAction) {
        self.undo_stack.push(action);
        self.redo_stack.clear(); // New action invalidates redo

        // Limit stack size
        while self.undo_stack.len() > self.max_undo_levels {
            self.undo_stack.remove(0);
        }
    }

    /// Undo the last action
    ///
    /// Returns the action that was undone (with state to restore),
    /// or None if nothing to undo.
    pub fn undo(&mut self) -> Option<UndoResult> {
        let action = self.undo_stack.pop()?;
        let description = action.description.clone();
        let dsp_chain = action.dsp_chain_before.clone();
        let layer1_path = action.layer1_path_before.clone();

        self.redo_stack.push(action);

        Some(UndoResult {
            message: format!("Undone: {}", description),
            dsp_chain_state: dsp_chain,
            layer1_path,
        })
    }

    /// Redo the last undone action
    ///
    /// Returns the action that was redone (with state to restore),
    /// or None if nothing to redo.
    pub fn redo(&mut self) -> Option<UndoResult> {
        let action = self.redo_stack.pop()?;
        let description = action.description.clone();
        let dsp_chain = action.dsp_chain_after.clone();
        let layer1_path = action.layer1_path_after.clone();

        self.undo_stack.push(action);

        Some(UndoResult {
            message: format!("Redone: {}", description),
            dsp_chain_state: dsp_chain,
            layer1_path,
        })
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Get number of available undos
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Get number of available redos
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }

    /// Get the last action description (for display)
    pub fn last_action_description(&self) -> Option<&str> {
        self.undo_stack.last().map(|a| a.description.as_str())
    }

    /// Get undo history (most recent first)
    pub fn undo_history(&self) -> Vec<&str> {
        self.undo_stack
            .iter()
            .rev()
            .map(|a| a.description.as_str())
            .collect()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

impl Default for UndoManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of an undo/redo operation
#[derive(Debug, Clone)]
pub struct UndoResult {
    /// Message describing what was done
    pub message: String,

    /// DSP chain state to restore
    pub dsp_chain_state: Vec<EffectState>,

    /// Layer 1 path to restore (if neural action)
    pub layer1_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_effect_state(id: &str, effect_type: &str) -> EffectState {
        EffectState {
            id: id.to_string(),
            effect_type: effect_type.to_string(),
            enabled: true,
            params: HashMap::new(),
        }
    }

    #[test]
    fn test_record_and_undo() {
        let mut manager = UndoManager::new();

        let before = vec![];
        let after = vec![make_effect_state("eq-1", "eq")];

        let action = UndoableAction::new("Added EQ").with_dsp_states(before, after);

        manager.record_action(action);

        assert!(manager.can_undo());
        assert!(!manager.can_redo());

        let result = manager.undo().unwrap();
        assert!(result.message.contains("Undone"));
        assert!(result.dsp_chain_state.is_empty()); // Restored to before

        assert!(!manager.can_undo());
        assert!(manager.can_redo());
    }

    #[test]
    fn test_undo_redo_cycle() {
        let mut manager = UndoManager::new();

        let action = UndoableAction::new("Test action").with_dsp_states(
            vec![],
            vec![make_effect_state("eq-1", "eq")],
        );

        manager.record_action(action);

        // Undo
        manager.undo();
        assert!(manager.can_redo());

        // Redo
        let result = manager.redo().unwrap();
        assert!(result.message.contains("Redone"));
        assert_eq!(result.dsp_chain_state.len(), 1);

        assert!(manager.can_undo());
        assert!(!manager.can_redo());
    }

    #[test]
    fn test_new_action_clears_redo() {
        let mut manager = UndoManager::new();

        manager.record_action(UndoableAction::new("Action 1"));
        manager.undo();
        assert!(manager.can_redo());

        // New action should clear redo
        manager.record_action(UndoableAction::new("Action 2"));
        assert!(!manager.can_redo());
    }

    #[test]
    fn test_max_undo_levels() {
        let mut manager = UndoManager::with_max_levels(3);

        for i in 0..5 {
            manager.record_action(UndoableAction::new(&format!("Action {}", i)));
        }

        assert_eq!(manager.undo_count(), 3);
    }

    #[test]
    fn test_undo_history() {
        let mut manager = UndoManager::new();

        manager.record_action(UndoableAction::new("First"));
        manager.record_action(UndoableAction::new("Second"));
        manager.record_action(UndoableAction::new("Third"));

        let history = manager.undo_history();
        assert_eq!(history, vec!["Third", "Second", "First"]);
    }

    #[test]
    fn test_neural_action() {
        let mut manager = UndoManager::new();

        let action = UndoableAction::new("Applied style transfer").with_layer1_paths(
            Some("/path/to/original.wav".to_string()),
            Some("/path/to/processed.wav".to_string()),
        );

        assert!(action.is_neural_action());

        manager.record_action(action);

        let result = manager.undo().unwrap();
        assert_eq!(result.layer1_path, Some("/path/to/original.wav".to_string()));
    }
}
