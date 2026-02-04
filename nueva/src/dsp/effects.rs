//! Effect trait and implementations
//!
//! All DSP effects implement the Effect trait.

use crate::audio::AudioBuffer;
use crate::error::Result;

/// Base trait for all DSP effects
pub trait Effect: Send + Sync {
    /// Unique identifier for this effect instance
    fn id(&self) -> &str;

    /// Effect type name (e.g., "parametric-eq", "compressor")
    fn effect_type(&self) -> &str;

    /// Process audio in-place
    fn process(&mut self, buffer: &mut AudioBuffer) -> Result<()>;

    /// Reset effect state (clear delay lines, etc.)
    fn reset(&mut self);

    /// Get current parameters as JSON
    fn get_params(&self) -> serde_json::Value;

    /// Set parameters from JSON
    fn set_params(&mut self, params: &serde_json::Value) -> Result<()>;
}

/// Simple gain effect for testing
#[derive(Debug)]
pub struct GainEffect {
    id: String,
    gain_db: f32,
}

impl GainEffect {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            gain_db: 0.0,
        }
    }

    pub fn with_gain(id: &str, gain_db: f32) -> Self {
        Self {
            id: id.to_string(),
            gain_db,
        }
    }

    pub fn set_gain_db(&mut self, gain_db: f32) {
        self.gain_db = gain_db;
    }

    pub fn gain_db(&self) -> f32 {
        self.gain_db
    }
}

impl Effect for GainEffect {
    fn id(&self) -> &str {
        &self.id
    }

    fn effect_type(&self) -> &str {
        "gain"
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> Result<()> {
        buffer.apply_gain_db(self.gain_db);
        Ok(())
    }

    fn reset(&mut self) {
        // Gain has no state to reset
    }

    fn get_params(&self) -> serde_json::Value {
        serde_json::json!({
            "gain_db": self.gain_db
        })
    }

    fn set_params(&mut self, params: &serde_json::Value) -> Result<()> {
        if let Some(gain) = params.get("gain_db").and_then(|v| v.as_f64()) {
            self.gain_db = gain as f32;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::verification::calculate_rms_db;

    #[test]
    fn test_gain_effect() {
        let mut buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);
        let original_rms = calculate_rms_db(buffer.samples());

        let mut gain = GainEffect::with_gain("gain-1", -6.0);
        gain.process(&mut buffer).unwrap();

        let new_rms = calculate_rms_db(buffer.samples());
        assert!((new_rms - (original_rms - 6.0)).abs() < 0.1);
    }

    #[test]
    fn test_gain_params() {
        let mut gain = GainEffect::new("gain-1");
        gain.set_params(&serde_json::json!({"gain_db": -12.0})).unwrap();
        assert!((gain.gain_db() - (-12.0)).abs() < 0.001);
    }
}
