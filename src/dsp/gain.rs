//! Gain effect implementation (spec 4.2.1)
//!
//! Simple gain control with dB-based parameter.
//! Range: -96 to +24 dB

use super::{AudioBuffer, Effect, EffectMetadata};
use crate::error::{NuevaError, Result};
use serde::{Deserialize, Serialize};

/// Minimum gain in dB
const GAIN_MIN_DB: f32 = -96.0;
/// Maximum gain in dB
const GAIN_MAX_DB: f32 = 24.0;

/// Gain effect (spec 4.2.1)
///
/// Applies simple multiplication to adjust audio level.
/// Gain is specified in decibels for intuitive control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GainEffect {
    /// Unique instance identifier
    id: String,
    /// Whether the effect is enabled
    enabled: bool,
    /// Gain in decibels (-96 to +24)
    gain_db: f32,
    /// Cached linear gain value (10^(gain_db/20))
    #[serde(skip)]
    gain_linear: f32,
    /// Sample rate (stored from prepare)
    #[serde(skip)]
    sample_rate: f64,
    /// Samples per block (stored from prepare)
    #[serde(skip)]
    samples_per_block: usize,
}

impl GainEffect {
    /// Create a new gain effect with 0 dB (unity gain)
    pub fn new() -> Self {
        Self {
            id: String::new(),
            enabled: true,
            gain_db: 0.0,
            gain_linear: 1.0,
            sample_rate: 44100.0,
            samples_per_block: 512,
        }
    }

    /// Create a new gain effect with the specified gain in dB
    ///
    /// Returns an error if gain_db is outside the valid range.
    pub fn with_gain(gain_db: f32) -> Result<Self> {
        let mut effect = Self::new();
        effect.set_gain_db(gain_db)?;
        Ok(effect)
    }

    /// Get the current gain in dB
    pub fn gain_db(&self) -> f32 {
        self.gain_db
    }

    /// Set the gain in dB
    ///
    /// Valid range: -96 to +24 dB
    pub fn set_gain_db(&mut self, gain_db: f32) -> Result<()> {
        if !(GAIN_MIN_DB..=GAIN_MAX_DB).contains(&gain_db) {
            return Err(NuevaError::InvalidParameter {
                param: "gain_db".to_string(),
                value: gain_db.to_string(),
                expected: format!("{} to {} dB", GAIN_MIN_DB, GAIN_MAX_DB),
            });
        }
        self.gain_db = gain_db;
        self.update_linear_gain();
        Ok(())
    }

    /// Get the current linear gain multiplier
    pub fn gain_linear(&self) -> f32 {
        self.gain_linear
    }

    /// Convert dB to linear gain: 10^(dB/20)
    fn db_to_linear(db: f32) -> f32 {
        10.0_f32.powf(db / 20.0)
    }

    /// Update the cached linear gain value from gain_db
    fn update_linear_gain(&mut self) {
        self.gain_linear = Self::db_to_linear(self.gain_db);
    }
}

impl Default for GainEffect {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for GainEffect {
    fn process(&mut self, buffer: &mut AudioBuffer) {
        if !self.enabled {
            return;
        }

        let gain = self.gain_linear;
        for sample in buffer.samples_mut() {
            *sample *= gain;
        }
    }

    fn prepare(&mut self, sample_rate: f64, samples_per_block: usize) {
        self.sample_rate = sample_rate;
        self.samples_per_block = samples_per_block;
        // Ensure linear gain is up to date
        self.update_linear_gain();
    }

    fn reset(&mut self) {
        // Gain effect has no internal state to reset
        // (no delay lines, envelope followers, etc.)
    }

    fn to_json(&self) -> Result<serde_json::Value> {
        serde_json::to_value(self).map_err(|e| NuevaError::SerializationError {
            details: e.to_string(),
        })
    }

    fn from_json(&mut self, json: &serde_json::Value) -> Result<()> {
        let loaded: GainEffect =
            serde_json::from_value(json.clone()).map_err(|e| NuevaError::SerializationError {
                details: e.to_string(),
            })?;

        // Validate the loaded gain value
        if !(GAIN_MIN_DB..=GAIN_MAX_DB).contains(&loaded.gain_db) {
            return Err(NuevaError::InvalidParameter {
                param: "gain_db".to_string(),
                value: loaded.gain_db.to_string(),
                expected: format!("{} to {} dB", GAIN_MIN_DB, GAIN_MAX_DB),
            });
        }

        self.id = loaded.id;
        self.enabled = loaded.enabled;
        self.gain_db = loaded.gain_db;
        self.update_linear_gain();
        Ok(())
    }

    fn effect_type(&self) -> &'static str {
        "gain"
    }

    fn display_name(&self) -> &'static str {
        "Gain"
    }

    fn metadata(&self) -> EffectMetadata {
        EffectMetadata {
            effect_type: "gain".to_string(),
            display_name: "Gain".to_string(),
            category: "utility".to_string(),
            order_priority: 0, // Gain can go anywhere, defaults to early
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn set_id(&mut self, id: String) {
        self.id = id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_gain() {
        let effect = GainEffect::new();
        assert_eq!(effect.gain_db(), 0.0);
        assert!((effect.gain_linear() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_gain_with_value() {
        let effect = GainEffect::with_gain(-6.0).unwrap();
        assert_eq!(effect.gain_db(), -6.0);
        // -6 dB should be approximately 0.5
        assert!((effect.gain_linear() - 0.5011872).abs() < 1e-4);
    }

    #[test]
    fn test_gain_6db_boost() {
        let effect = GainEffect::with_gain(6.0).unwrap();
        // +6 dB should be approximately 2.0
        assert!((effect.gain_linear() - 1.9952623).abs() < 1e-4);
    }

    #[test]
    fn test_gain_range_validation() {
        // Valid range
        assert!(GainEffect::with_gain(-96.0).is_ok());
        assert!(GainEffect::with_gain(24.0).is_ok());
        assert!(GainEffect::with_gain(0.0).is_ok());

        // Invalid range
        assert!(GainEffect::with_gain(-97.0).is_err());
        assert!(GainEffect::with_gain(25.0).is_err());
    }

    #[test]
    fn test_set_gain_db() {
        let mut effect = GainEffect::new();

        assert!(effect.set_gain_db(-12.0).is_ok());
        assert_eq!(effect.gain_db(), -12.0);

        assert!(effect.set_gain_db(-100.0).is_err());
        assert_eq!(effect.gain_db(), -12.0); // Should not have changed
    }

    #[test]
    fn test_process_unity_gain() {
        let mut effect = GainEffect::new();
        effect.prepare(44100.0, 512);

        let mut buffer =
            AudioBuffer::from_interleaved(vec![0.5, -0.5, 0.25, -0.25], 2, 44100.0).unwrap();

        effect.process(&mut buffer);

        let samples = buffer.samples();
        assert!((samples[0] - 0.5).abs() < 1e-6);
        assert!((samples[1] - (-0.5)).abs() < 1e-6);
        assert!((samples[2] - 0.25).abs() < 1e-6);
        assert!((samples[3] - (-0.25)).abs() < 1e-6);
    }

    #[test]
    fn test_process_6db_boost() {
        let mut effect = GainEffect::with_gain(6.0).unwrap();
        effect.prepare(44100.0, 512);

        let mut buffer =
            AudioBuffer::from_interleaved(vec![0.5, -0.5, 0.25, -0.25], 2, 44100.0).unwrap();

        effect.process(&mut buffer);

        let samples = buffer.samples();
        let gain = effect.gain_linear();
        assert!((samples[0] - 0.5 * gain).abs() < 1e-6);
        assert!((samples[1] - (-0.5) * gain).abs() < 1e-6);
    }

    #[test]
    fn test_process_6db_cut() {
        let mut effect = GainEffect::with_gain(-6.0).unwrap();
        effect.prepare(44100.0, 512);

        let mut buffer =
            AudioBuffer::from_interleaved(vec![1.0, -1.0, 0.5, -0.5], 2, 44100.0).unwrap();

        effect.process(&mut buffer);

        let samples = buffer.samples();
        // -6 dB ~= 0.5 multiplier
        assert!((samples[0] - 0.5011872).abs() < 1e-4);
        assert!((samples[1] - (-0.5011872)).abs() < 1e-4);
    }

    #[test]
    fn test_process_disabled() {
        let mut effect = GainEffect::with_gain(12.0).unwrap();
        effect.set_enabled(false);
        effect.prepare(44100.0, 512);

        let mut buffer = AudioBuffer::from_interleaved(vec![0.5, -0.5], 2, 44100.0).unwrap();

        effect.process(&mut buffer);

        // Should not be modified when disabled
        let samples = buffer.samples();
        assert!((samples[0] - 0.5).abs() < 1e-6);
        assert!((samples[1] - (-0.5)).abs() < 1e-6);
    }

    #[test]
    fn test_effect_type() {
        let effect = GainEffect::new();
        assert_eq!(effect.effect_type(), "gain");
    }

    #[test]
    fn test_display_name() {
        let effect = GainEffect::new();
        assert_eq!(effect.display_name(), "Gain");
    }

    #[test]
    fn test_metadata() {
        let effect = GainEffect::new();
        let meta = effect.metadata();
        assert_eq!(meta.effect_type, "gain");
        assert_eq!(meta.display_name, "Gain");
        assert_eq!(meta.category, "utility");
    }

    #[test]
    fn test_id() {
        let mut effect = GainEffect::new();
        assert!(effect.id().is_empty());

        effect.set_id("gain-1".to_string());
        assert_eq!(effect.id(), "gain-1");
    }

    #[test]
    fn test_enabled() {
        let mut effect = GainEffect::new();
        assert!(effect.is_enabled());

        effect.set_enabled(false);
        assert!(!effect.is_enabled());

        effect.set_enabled(true);
        assert!(effect.is_enabled());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let mut effect = GainEffect::new();
        effect.set_id("gain-test".to_string());
        effect.set_gain_db(-12.0).unwrap();
        effect.set_enabled(false);

        let json = effect.to_json().unwrap();

        let mut restored = GainEffect::new();
        restored.from_json(&json).unwrap();

        assert_eq!(restored.id(), "gain-test");
        assert_eq!(restored.gain_db(), -12.0);
        assert!(!restored.is_enabled());
        // Linear gain should be recalculated
        assert!((restored.gain_linear() - effect.gain_linear()).abs() < 1e-6);
    }

    #[test]
    fn test_from_json_validation() {
        let mut effect = GainEffect::new();

        // Invalid gain value should be rejected
        let invalid_json = serde_json::json!({
            "id": "test",
            "enabled": true,
            "gain_db": 100.0  // Out of range
        });

        assert!(effect.from_json(&invalid_json).is_err());
    }

    #[test]
    fn test_extreme_gain_values() {
        // Test minimum gain (-96 dB)
        let effect_min = GainEffect::with_gain(-96.0).unwrap();
        // -96 dB should be very small (approximately 1.58e-5)
        assert!(effect_min.gain_linear() < 0.0001);
        assert!(effect_min.gain_linear() > 0.0);

        // Test maximum gain (+24 dB)
        let effect_max = GainEffect::with_gain(24.0).unwrap();
        // +24 dB should be approximately 15.85
        assert!((effect_max.gain_linear() - 15.848932).abs() < 1e-3);
    }

    #[test]
    fn test_db_to_linear_conversion() {
        // Test known conversions
        assert!((GainEffect::db_to_linear(0.0) - 1.0).abs() < 1e-6);
        assert!((GainEffect::db_to_linear(20.0) - 10.0).abs() < 1e-4);
        assert!((GainEffect::db_to_linear(-20.0) - 0.1).abs() < 1e-4);
        assert!((GainEffect::db_to_linear(6.0) - 1.9952623).abs() < 1e-4);
        assert!((GainEffect::db_to_linear(-6.0) - 0.5011872).abs() < 1e-4);
    }

    #[test]
    fn test_reset() {
        let mut effect = GainEffect::with_gain(-12.0).unwrap();
        effect.prepare(48000.0, 1024);

        // Reset should not change gain settings
        effect.reset();

        assert_eq!(effect.gain_db(), -12.0);
    }

    #[test]
    fn test_prepare() {
        let mut effect = GainEffect::with_gain(-6.0).unwrap();

        effect.prepare(48000.0, 1024);

        // Gain should still be valid after prepare
        assert_eq!(effect.gain_db(), -6.0);
        assert!((effect.gain_linear() - 0.5011872).abs() < 1e-4);
    }
}
