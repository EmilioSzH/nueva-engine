//! Effect Chain management (spec §4.3)
//!
//! Effects are processed in chain order (index 0 first).
//! Recommended default order:
//! 1. Gate (clean up noise before processing)
//! 2. EQ (corrective) - remove problems
//! 3. Compression
//! 4. EQ (creative) - add color
//! 5. Saturation
//! 6. Delay
//! 7. Reverb (almost always last among time-based)
//! 8. Limiter (always last)

use super::{AudioBuffer, Effect, ProcessResult};
use crate::error::{NuevaError, Result};

/// Order priority constants (spec §4.3)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EffectPosition {
    Gate = 0,
    EqCorrective = 1,
    Compressor = 2,
    EqCreative = 3,
    Saturation = 4,
    Delay = 5,
    Reverb = 6,
    Limiter = 7,
}

impl EffectPosition {
    /// Get recommended position for an effect type
    pub fn for_effect_type(effect_type: &str) -> Self {
        match effect_type {
            "gate" => EffectPosition::Gate,
            "eq" | "parametric-eq" => EffectPosition::EqCorrective,
            "compressor" => EffectPosition::Compressor,
            "saturation" => EffectPosition::Saturation,
            "delay" => EffectPosition::Delay,
            "reverb" => EffectPosition::Reverb,
            "limiter" => EffectPosition::Limiter,
            _ => EffectPosition::Saturation, // Default to middle
        }
    }
}

/// Chain of effects for processing
pub struct EffectChain {
    effects: Vec<Box<dyn Effect>>,
    sample_rate: f64,
    samples_per_block: usize,
}

impl EffectChain {
    /// Create a new empty effect chain
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
            sample_rate: 44100.0,
            samples_per_block: 512,
        }
    }

    /// Prepare all effects for processing
    pub fn prepare(&mut self, sample_rate: f64, samples_per_block: usize) {
        self.sample_rate = sample_rate;
        self.samples_per_block = samples_per_block;
        for effect in &mut self.effects {
            effect.prepare(sample_rate, samples_per_block);
        }
    }

    /// Reset all effects
    pub fn reset(&mut self) {
        for effect in &mut self.effects {
            effect.reset();
        }
    }

    /// Add an effect at the recommended position (spec §4.3)
    pub fn add(&mut self, mut effect: Box<dyn Effect>) {
        effect.prepare(self.sample_rate, self.samples_per_block);
        let position = self.get_recommended_position(effect.effect_type());
        self.effects.insert(position, effect);
    }

    /// Add an effect at a specific index
    pub fn add_at(&mut self, mut effect: Box<dyn Effect>, index: usize) {
        effect.prepare(self.sample_rate, self.samples_per_block);
        let index = index.min(self.effects.len());
        self.effects.insert(index, effect);
    }

    /// Remove an effect by ID
    pub fn remove(&mut self, effect_id: &str) -> Result<Box<dyn Effect>> {
        let index = self
            .effects
            .iter()
            .position(|e| e.id() == effect_id)
            .ok_or_else(|| NuevaError::EffectNotFound {
                effect_id: effect_id.to_string(),
            })?;

        Ok(self.effects.remove(index))
    }

    /// Get a reference to an effect by ID
    pub fn get(&self, effect_id: &str) -> Option<&dyn Effect> {
        self.effects
            .iter()
            .find(|e| e.id() == effect_id)
            .map(|e| e.as_ref())
    }

    /// Get a mutable reference to an effect by ID
    pub fn get_mut(&mut self, effect_id: &str) -> Option<&mut (dyn Effect + 'static)> {
        for effect in &mut self.effects {
            if effect.id() == effect_id {
                return Some(effect.as_mut());
            }
        }
        None
    }

    /// Move an effect to a new position
    pub fn move_effect(&mut self, effect_id: &str, new_index: usize) -> Result<()> {
        let current_index = self
            .effects
            .iter()
            .position(|e| e.id() == effect_id)
            .ok_or_else(|| NuevaError::EffectNotFound {
                effect_id: effect_id.to_string(),
            })?;

        let effect = self.effects.remove(current_index);
        let new_index = new_index.min(self.effects.len());
        self.effects.insert(new_index, effect);
        Ok(())
    }

    /// Process the entire chain
    pub fn process(&mut self, buffer: &mut AudioBuffer) -> Vec<ProcessResult> {
        let mut results = Vec::with_capacity(self.effects.len());
        for effect in &mut self.effects {
            results.push(effect.process_safe(buffer));
        }
        results
    }

    /// Get the number of effects in the chain
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// Check if the chain is empty
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// Iterate over effects
    pub fn iter(&self) -> impl Iterator<Item = &dyn Effect> {
        self.effects.iter().map(|e| e.as_ref())
    }

    /// Get recommended position for inserting an effect type (spec §4.3)
    fn get_recommended_position(&self, effect_type: &str) -> usize {
        let priority = EffectPosition::for_effect_type(effect_type) as u32;

        for (i, effect) in self.effects.iter().enumerate() {
            let existing_priority = EffectPosition::for_effect_type(effect.effect_type()) as u32;
            if existing_priority > priority {
                return i;
            }
        }

        self.effects.len()
    }

    /// Serialize chain state to JSON
    pub fn to_json(&self) -> Result<serde_json::Value> {
        let effects: Result<Vec<serde_json::Value>> =
            self.effects.iter().map(|e| e.to_json()).collect();

        Ok(serde_json::json!({
            "effects": effects?,
            "sample_rate": self.sample_rate,
            "samples_per_block": self.samples_per_block,
        }))
    }
}

impl Default for EffectChain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effect_position_ordering() {
        assert!(EffectPosition::Gate < EffectPosition::Compressor);
        assert!(EffectPosition::Compressor < EffectPosition::Reverb);
        assert!(EffectPosition::Reverb < EffectPosition::Limiter);
    }

    #[test]
    fn test_chain_new() {
        let chain = EffectChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
    }
}
