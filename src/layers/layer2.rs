//! Layer 2 - DSP Chain (Real-time)
//!
//! Layer 2 stores DSP effect parameters, NOT rendered audio.
//! Effects are applied in real-time during playback and at
//! export time. The chain order matters for signal flow.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{NuevaError, Result};

/// State of a single DSP effect in the chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectState {
    /// Unique identifier for this effect instance (e.g., "eq-1", "compressor-2")
    pub id: String,
    /// Type of effect (e.g., "eq", "compressor", "reverb", "delay")
    pub effect_type: String,
    /// Whether this effect is active in the chain
    pub enabled: bool,
    /// Effect-specific parameters as JSON
    pub params: Value,
}

impl EffectState {
    /// Create a new effect state with default parameters
    pub fn new(id: impl Into<String>, effect_type: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            effect_type: effect_type.into(),
            enabled: true,
            params: Value::Object(serde_json::Map::new()),
        }
    }

    /// Create a new effect state with specific parameters
    pub fn with_params(
        id: impl Into<String>,
        effect_type: impl Into<String>,
        params: Value,
    ) -> Self {
        Self {
            id: id.into(),
            effect_type: effect_type.into(),
            enabled: true,
            params,
        }
    }

    /// Enable this effect
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable this effect (bypass)
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Toggle the enabled state
    pub fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        self.enabled
    }

    /// Update a specific parameter
    pub fn set_param(&mut self, key: &str, value: Value) {
        if let Value::Object(ref mut map) = self.params {
            map.insert(key.to_string(), value);
        }
    }

    /// Get a specific parameter value
    pub fn get_param(&self, key: &str) -> Option<&Value> {
        if let Value::Object(ref map) = self.params {
            map.get(key)
        } else {
            None
        }
    }
}

/// Layer 2: DSP Effect Chain
///
/// This layer manages an ordered chain of DSP effects.
/// Effects are stored as parameters only; actual processing
/// happens in real-time during playback or at export.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Layer2 {
    /// Ordered list of effects in the chain
    effects: Vec<EffectState>,
}

impl Layer2 {
    /// Create a new empty effect chain
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
        }
    }

    /// Add an effect to the end of the chain
    ///
    /// # Returns
    /// The index of the newly added effect
    pub fn add_effect(&mut self, effect: EffectState) -> usize {
        let index = self.effects.len();
        self.effects.push(effect);
        index
    }

    /// Insert an effect at a specific position
    ///
    /// # Arguments
    /// * `index` - Position to insert at (0 = start of chain)
    /// * `effect` - The effect to insert
    ///
    /// # Returns
    /// The actual index where the effect was inserted (clamped to valid range)
    pub fn insert_effect(&mut self, index: usize, effect: EffectState) -> usize {
        let actual_index = index.min(self.effects.len());
        self.effects.insert(actual_index, effect);
        actual_index
    }

    /// Remove an effect by its ID
    ///
    /// # Returns
    /// The removed effect, or None if not found
    pub fn remove_effect(&mut self, id: &str) -> Option<EffectState> {
        if let Some(index) = self.effects.iter().position(|e| e.id == id) {
            Some(self.effects.remove(index))
        } else {
            None
        }
    }

    /// Get an effect by its ID (immutable)
    pub fn get_effect(&self, id: &str) -> Option<&EffectState> {
        self.effects.iter().find(|e| e.id == id)
    }

    /// Get an effect by its ID (mutable)
    pub fn get_effect_mut(&mut self, id: &str) -> Option<&mut EffectState> {
        self.effects.iter_mut().find(|e| e.id == id)
    }

    /// Get an effect by its index (immutable)
    pub fn get_effect_at(&self, index: usize) -> Option<&EffectState> {
        self.effects.get(index)
    }

    /// Get an effect by its index (mutable)
    pub fn get_effect_at_mut(&mut self, index: usize) -> Option<&mut EffectState> {
        self.effects.get_mut(index)
    }

    /// Reorder an effect to a new position in the chain
    ///
    /// # Arguments
    /// * `id` - ID of the effect to move
    /// * `new_index` - New position (0 = start of chain)
    ///
    /// # Errors
    /// Returns error if the effect is not found
    pub fn reorder(&mut self, id: &str, new_index: usize) -> Result<()> {
        let current_index = self
            .effects
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| NuevaError::LayerError {
                reason: format!("Effect '{}' not found in chain", id),
            })?;

        if current_index == new_index {
            return Ok(());
        }

        let effect = self.effects.remove(current_index);
        let target_index = new_index.min(self.effects.len());
        self.effects.insert(target_index, effect);

        Ok(())
    }

    /// Clear all effects from the chain
    pub fn clear(&mut self) {
        self.effects.clear();
    }

    /// Get the number of effects in the chain
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// Check if the chain is empty
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// Get the number of enabled effects
    pub fn enabled_count(&self) -> usize {
        self.effects.iter().filter(|e| e.enabled).count()
    }

    /// Iterate over all effects in order
    pub fn iter(&self) -> impl Iterator<Item = &EffectState> {
        self.effects.iter()
    }

    /// Iterate over all effects in order (mutable)
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut EffectState> {
        self.effects.iter_mut()
    }

    /// Iterate over enabled effects only
    pub fn iter_enabled(&self) -> impl Iterator<Item = &EffectState> {
        self.effects.iter().filter(|e| e.enabled)
    }

    /// Get the index of an effect by ID
    pub fn get_index(&self, id: &str) -> Option<usize> {
        self.effects.iter().position(|e| e.id == id)
    }

    /// Enable all effects in the chain
    pub fn enable_all(&mut self) {
        for effect in &mut self.effects {
            effect.enabled = true;
        }
    }

    /// Disable all effects in the chain (bypass all)
    pub fn disable_all(&mut self) {
        for effect in &mut self.effects {
            effect.enabled = false;
        }
    }

    /// Generate a unique ID for a new effect of the given type
    pub fn generate_id(&self, effect_type: &str) -> String {
        let mut counter = 1;
        loop {
            let id = format!("{}-{}", effect_type, counter);
            if self.get_effect(&id).is_none() {
                return id;
            }
            counter += 1;
        }
    }

    /// Duplicate an effect and add it after the original
    ///
    /// # Returns
    /// The ID of the new effect, or None if the original wasn't found
    pub fn duplicate_effect(&mut self, id: &str) -> Option<String> {
        let (index, effect_clone) = {
            let index = self.effects.iter().position(|e| e.id == id)?;
            let effect = &self.effects[index];
            let new_id = self.generate_id(&effect.effect_type);
            let mut clone = effect.clone();
            clone.id = new_id;
            (index, clone)
        };

        let new_id = effect_clone.id.clone();
        self.effects.insert(index + 1, effect_clone);
        Some(new_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer2_new() {
        let layer2 = Layer2::new();
        assert!(layer2.is_empty());
        assert_eq!(layer2.len(), 0);
    }

    #[test]
    fn test_add_effect() {
        let mut layer2 = Layer2::new();

        let eq = EffectState::new("eq-1", "eq");
        let index = layer2.add_effect(eq);

        assert_eq!(index, 0);
        assert_eq!(layer2.len(), 1);
        assert!(layer2.get_effect("eq-1").is_some());
    }

    #[test]
    fn test_remove_effect() {
        let mut layer2 = Layer2::new();

        layer2.add_effect(EffectState::new("eq-1", "eq"));
        layer2.add_effect(EffectState::new("comp-1", "compressor"));

        let removed = layer2.remove_effect("eq-1");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, "eq-1");
        assert_eq!(layer2.len(), 1);
        assert!(layer2.get_effect("eq-1").is_none());
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut layer2 = Layer2::new();
        layer2.add_effect(EffectState::new("eq-1", "eq"));

        let removed = layer2.remove_effect("nonexistent");
        assert!(removed.is_none());
        assert_eq!(layer2.len(), 1);
    }

    #[test]
    fn test_reorder() {
        let mut layer2 = Layer2::new();

        layer2.add_effect(EffectState::new("eq-1", "eq"));
        layer2.add_effect(EffectState::new("comp-1", "compressor"));
        layer2.add_effect(EffectState::new("reverb-1", "reverb"));

        // Move reverb to the front
        layer2.reorder("reverb-1", 0).unwrap();

        assert_eq!(layer2.get_effect_at(0).unwrap().id, "reverb-1");
        assert_eq!(layer2.get_effect_at(1).unwrap().id, "eq-1");
        assert_eq!(layer2.get_effect_at(2).unwrap().id, "comp-1");
    }

    #[test]
    fn test_reorder_not_found() {
        let mut layer2 = Layer2::new();
        layer2.add_effect(EffectState::new("eq-1", "eq"));

        let result = layer2.reorder("nonexistent", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_effect_enable_disable() {
        let mut layer2 = Layer2::new();
        layer2.add_effect(EffectState::new("eq-1", "eq"));

        {
            let effect = layer2.get_effect_mut("eq-1").unwrap();
            assert!(effect.enabled);
            effect.disable();
            assert!(!effect.enabled);
            effect.enable();
            assert!(effect.enabled);
        }
    }

    #[test]
    fn test_effect_toggle() {
        let mut effect = EffectState::new("eq-1", "eq");
        assert!(effect.enabled);

        let new_state = effect.toggle();
        assert!(!new_state);
        assert!(!effect.enabled);

        let new_state = effect.toggle();
        assert!(new_state);
        assert!(effect.enabled);
    }

    #[test]
    fn test_effect_params() {
        let mut effect = EffectState::with_params(
            "eq-1",
            "eq",
            serde_json::json!({
                "frequency": 1000,
                "gain": 3.5
            }),
        );

        assert_eq!(
            effect.get_param("frequency"),
            Some(&serde_json::json!(1000))
        );

        effect.set_param("frequency", serde_json::json!(2000));
        assert_eq!(
            effect.get_param("frequency"),
            Some(&serde_json::json!(2000))
        );
    }

    #[test]
    fn test_iter_enabled() {
        let mut layer2 = Layer2::new();

        layer2.add_effect(EffectState::new("eq-1", "eq"));
        layer2.add_effect(EffectState::new("comp-1", "compressor"));
        layer2.add_effect(EffectState::new("reverb-1", "reverb"));

        // Disable the compressor
        layer2.get_effect_mut("comp-1").unwrap().disable();

        let enabled: Vec<_> = layer2.iter_enabled().collect();
        assert_eq!(enabled.len(), 2);
        assert!(enabled.iter().any(|e| e.id == "eq-1"));
        assert!(enabled.iter().any(|e| e.id == "reverb-1"));
    }

    #[test]
    fn test_clear() {
        let mut layer2 = Layer2::new();

        layer2.add_effect(EffectState::new("eq-1", "eq"));
        layer2.add_effect(EffectState::new("comp-1", "compressor"));

        layer2.clear();

        assert!(layer2.is_empty());
        assert_eq!(layer2.len(), 0);
    }

    #[test]
    fn test_generate_id() {
        let mut layer2 = Layer2::new();

        layer2.add_effect(EffectState::new("eq-1", "eq"));
        layer2.add_effect(EffectState::new("eq-2", "eq"));

        let new_id = layer2.generate_id("eq");
        assert_eq!(new_id, "eq-3");

        let comp_id = layer2.generate_id("compressor");
        assert_eq!(comp_id, "compressor-1");
    }

    #[test]
    fn test_duplicate_effect() {
        let mut layer2 = Layer2::new();

        layer2.add_effect(EffectState::with_params(
            "eq-1",
            "eq",
            serde_json::json!({"frequency": 1000}),
        ));

        let new_id = layer2.duplicate_effect("eq-1");
        assert_eq!(new_id, Some("eq-2".to_string()));
        assert_eq!(layer2.len(), 2);

        // Check that the duplicate has the same params
        let dup = layer2.get_effect("eq-2").unwrap();
        assert_eq!(dup.get_param("frequency"), Some(&serde_json::json!(1000)));
    }

    #[test]
    fn test_insert_effect() {
        let mut layer2 = Layer2::new();

        layer2.add_effect(EffectState::new("eq-1", "eq"));
        layer2.add_effect(EffectState::new("reverb-1", "reverb"));

        // Insert compressor in the middle
        layer2.insert_effect(1, EffectState::new("comp-1", "compressor"));

        assert_eq!(layer2.get_effect_at(0).unwrap().id, "eq-1");
        assert_eq!(layer2.get_effect_at(1).unwrap().id, "comp-1");
        assert_eq!(layer2.get_effect_at(2).unwrap().id, "reverb-1");
    }

    #[test]
    fn test_enable_disable_all() {
        let mut layer2 = Layer2::new();

        layer2.add_effect(EffectState::new("eq-1", "eq"));
        layer2.add_effect(EffectState::new("comp-1", "compressor"));

        layer2.disable_all();
        assert_eq!(layer2.enabled_count(), 0);

        layer2.enable_all();
        assert_eq!(layer2.enabled_count(), 2);
    }
}
