//! DSP Effect Chain
//!
//! Manages ordered sequence of effects for Layer 2 processing.

use crate::audio::AudioBuffer;
use crate::dsp::effects::Effect;
use crate::error::Result;

/// Ordered chain of DSP effects
pub struct DspChain {
    effects: Vec<Box<dyn Effect>>,
}

impl DspChain {
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
        }
    }

    /// Add an effect to the end of the chain
    pub fn add(&mut self, effect: Box<dyn Effect>) {
        self.effects.push(effect);
    }

    /// Insert an effect at a specific position
    pub fn insert(&mut self, index: usize, effect: Box<dyn Effect>) {
        if index <= self.effects.len() {
            self.effects.insert(index, effect);
        }
    }

    /// Remove an effect by ID
    pub fn remove(&mut self, id: &str) -> Option<Box<dyn Effect>> {
        if let Some(pos) = self.effects.iter().position(|e| e.id() == id) {
            Some(self.effects.remove(pos))
        } else {
            None
        }
    }

    /// Get an effect by ID
    pub fn get(&self, id: &str) -> Option<&dyn Effect> {
        self.effects.iter().find(|e| e.id() == id).map(|e| e.as_ref())
    }

    /// Get a mutable effect by ID
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Box<dyn Effect>> {
        self.effects.iter_mut().find(|e| e.id() == id)
    }

    /// Process audio through all effects in order
    pub fn process(&mut self, buffer: &mut AudioBuffer) -> Result<()> {
        for effect in &mut self.effects {
            effect.process(buffer)?;
        }
        Ok(())
    }

    /// Reset all effects
    pub fn reset(&mut self) {
        for effect in &mut self.effects {
            effect.reset();
        }
    }

    /// Get number of effects in chain
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// Check if chain is empty
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// Get effect IDs in order
    pub fn effect_ids(&self) -> Vec<&str> {
        self.effects.iter().map(|e| e.id()).collect()
    }
}

impl Default for DspChain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::effects::GainEffect;
    use crate::audio::verification::calculate_rms_db;

    #[test]
    fn test_empty_chain_passthrough() {
        let mut buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);
        let original = buffer.clone();

        let mut chain = DspChain::new();
        chain.process(&mut buffer).unwrap();

        assert!(buffer.is_identical_to(&original));
    }

    #[test]
    fn test_chain_processing() {
        let mut buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);
        let original_rms = calculate_rms_db(buffer.samples());

        let mut chain = DspChain::new();
        chain.add(Box::new(GainEffect::with_gain("gain-1", -6.0)));
        chain.add(Box::new(GainEffect::with_gain("gain-2", -6.0)));
        chain.process(&mut buffer).unwrap();

        let new_rms = calculate_rms_db(buffer.samples());
        // Two -6dB gains = -12dB total
        assert!((new_rms - (original_rms - 12.0)).abs() < 0.2);
    }

    #[test]
    fn test_chain_effect_management() {
        let mut chain = DspChain::new();
        chain.add(Box::new(GainEffect::new("gain-1")));
        chain.add(Box::new(GainEffect::new("gain-2")));

        assert_eq!(chain.len(), 2);
        assert_eq!(chain.effect_ids(), vec!["gain-1", "gain-2"]);

        chain.remove("gain-1");
        assert_eq!(chain.len(), 1);
        assert_eq!(chain.effect_ids(), vec!["gain-2"]);
    }
}
