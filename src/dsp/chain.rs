//! Effect Chain
//!
//! Manages ordered processing of multiple effects.

use super::{get_default_order_priority, Effect};
use crate::engine::AudioBuffer;
use crate::error::{NuevaError, Result};

/// Position hint for inserting effects into the chain
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainPosition {
    /// Insert at the beginning
    Start,
    /// Insert at the end
    End,
    /// Insert at a specific index
    Index(usize),
    /// Insert at recommended position based on effect type
    Recommended,
}

/// Ordered chain of effects for processing
#[derive(Default)]
pub struct EffectChain {
    effects: Vec<Box<dyn Effect>>,
    sample_rate: u32,
    max_block_size: usize,
}

impl EffectChain {
    /// Create a new empty effect chain
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
            sample_rate: 48000,
            max_block_size: 512,
        }
    }

    /// Prepare all effects for processing
    pub fn prepare(&mut self, sample_rate: u32, max_block_size: usize) {
        self.sample_rate = sample_rate;
        self.max_block_size = max_block_size;
        for effect in &mut self.effects {
            effect.prepare(sample_rate, max_block_size);
        }
    }

    /// Process audio through all enabled effects in order
    pub fn process(&mut self, buffer: &mut AudioBuffer) {
        for effect in &mut self.effects {
            if effect.is_enabled() {
                effect.process(buffer);
            }
        }
    }

    /// Reset all effects
    pub fn reset(&mut self) {
        for effect in &mut self.effects {
            effect.reset();
        }
    }

    /// Add an effect to the chain
    ///
    /// Returns the index where the effect was inserted.
    pub fn add(&mut self, mut effect: Box<dyn Effect>, position: ChainPosition) -> usize {
        effect.prepare(self.sample_rate, self.max_block_size);

        let index = match position {
            ChainPosition::Start => 0,
            ChainPosition::End => self.effects.len(),
            ChainPosition::Index(i) => i.min(self.effects.len()),
            ChainPosition::Recommended => self.find_recommended_position(&effect),
        };

        self.effects.insert(index, effect);
        index
    }

    /// Find recommended position for an effect based on type
    fn find_recommended_position(&self, effect: &Box<dyn Effect>) -> usize {
        let effect_priority = get_default_order_priority(effect.effect_type());

        for (i, existing) in self.effects.iter().enumerate() {
            let existing_priority = get_default_order_priority(existing.effect_type());
            if existing_priority > effect_priority {
                return i;
            }
        }

        self.effects.len()
    }

    /// Remove an effect by ID
    pub fn remove(&mut self, id: &str) -> Option<Box<dyn Effect>> {
        if let Some(index) = self.find_index(id) {
            Some(self.effects.remove(index))
        } else {
            None
        }
    }

    /// Get an effect by ID
    pub fn get(&self, id: &str) -> Option<&dyn Effect> {
        self.effects
            .iter()
            .find(|e| e.id() == id)
            .map(|e| e.as_ref())
    }

    /// Get a mutable reference to an effect by ID
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Box<dyn Effect>> {
        self.effects.iter_mut().find(|e| e.id() == id)
    }

    /// Get effect at index
    pub fn get_at(&self, index: usize) -> Option<&dyn Effect> {
        self.effects.get(index).map(|e| e.as_ref())
    }

    /// Get mutable effect at index
    pub fn get_at_mut(&mut self, index: usize) -> Option<&mut Box<dyn Effect>> {
        self.effects.get_mut(index)
    }

    /// Find index of effect by ID
    pub fn find_index(&self, id: &str) -> Option<usize> {
        self.effects.iter().position(|e| e.id() == id)
    }

    /// Move effect to a new position
    pub fn reorder(&mut self, id: &str, new_index: usize) -> Result<()> {
        let current_index = self
            .find_index(id)
            .ok_or_else(|| NuevaError::ProcessingError {
                reason: format!("Effect not found: {}", id),
            })?;

        let effect = self.effects.remove(current_index);
        let insert_index = new_index.min(self.effects.len());
        self.effects.insert(insert_index, effect);
        Ok(())
    }

    /// Number of effects in the chain
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// Check if chain is empty
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// Clear all effects
    pub fn clear(&mut self) {
        self.effects.clear();
    }

    /// Iterate over effects
    pub fn iter(&self) -> impl Iterator<Item = &dyn Effect> {
        self.effects.iter().map(|e| e.as_ref())
    }

    /// Iterate over enabled effects
    pub fn iter_enabled(&self) -> impl Iterator<Item = &dyn Effect> {
        self.effects
            .iter()
            .filter(|e| e.is_enabled())
            .map(|e| e.as_ref())
    }

    /// Get effect IDs in order
    pub fn effect_ids(&self) -> Vec<String> {
        self.effects.iter().map(|e| e.id().to_string()).collect()
    }

    /// Enable all effects
    pub fn enable_all(&mut self) {
        for effect in &mut self.effects {
            effect.set_enabled(true);
        }
    }

    /// Disable all effects
    pub fn disable_all(&mut self) {
        for effect in &mut self.effects {
            effect.set_enabled(false);
        }
    }

    /// Bypass processing (disable all) with state preservation
    pub fn bypass(&mut self) -> Vec<(String, bool)> {
        let states: Vec<_> = self
            .effects
            .iter()
            .map(|e| (e.id().to_string(), e.is_enabled()))
            .collect();
        self.disable_all();
        states
    }

    /// Restore enabled states from bypass
    pub fn restore(&mut self, states: &[(String, bool)]) {
        for (id, enabled) in states {
            if let Some(effect) = self.get_mut(id) {
                effect.set_enabled(*enabled);
            }
        }
    }
}

impl Clone for EffectChain {
    fn clone(&self) -> Self {
        Self {
            effects: self.effects.clone(),
            sample_rate: self.sample_rate,
            max_block_size: self.max_block_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::Gain;
    use crate::engine::buffer::ChannelLayout;

    #[test]
    fn test_chain_new() {
        let chain = EffectChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
    }

    #[test]
    fn test_chain_add_remove() {
        let mut chain = EffectChain::new();

        let gain = Gain::new(0.0);
        let id = gain.id().to_string();

        chain.add(Box::new(gain), ChainPosition::End);
        assert_eq!(chain.len(), 1);

        let removed = chain.remove(&id);
        assert!(removed.is_some());
        assert!(chain.is_empty());
    }

    #[test]
    fn test_chain_process() {
        let mut chain = EffectChain::new();
        chain.prepare(48000, 512);

        // Add -6dB gain
        chain.add(Box::new(Gain::new(-6.0)), ChainPosition::End);

        // Create test buffer with 1.0 samples
        let mut buffer = AudioBuffer::new(100, ChannelLayout::Mono);
        for i in 0..100 {
            buffer.set_sample(0, i, 1.0);
        }

        chain.process(&mut buffer);

        // Check gain was applied (approximately 0.5 for -6dB)
        let sample = buffer.get_sample(0, 0).unwrap();
        assert!((sample - 0.501187).abs() < 0.001);
    }

    #[test]
    fn test_chain_recommended_order() {
        let mut chain = EffectChain::new();

        // Add effects out of order
        let limiter = crate::dsp::Limiter::new(-1.0);
        let gate = crate::dsp::Gate::new(-40.0);
        let comp = crate::dsp::Compressor::new(-18.0, 4.0);

        chain.add(Box::new(limiter), ChainPosition::Recommended);
        chain.add(Box::new(gate), ChainPosition::Recommended);
        chain.add(Box::new(comp), ChainPosition::Recommended);

        // Verify order: gate (0) -> compressor (2) -> limiter (7)
        assert_eq!(chain.get_at(0).unwrap().effect_type(), "gate");
        assert_eq!(chain.get_at(1).unwrap().effect_type(), "compressor");
        assert_eq!(chain.get_at(2).unwrap().effect_type(), "limiter");
    }
}
