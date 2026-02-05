//! Gain Effect
//!
//! Simple gain adjustment effect per spec 4.2.1.
//! Provides volume control with dB-based interface.

use crate::dsp::effect::{Effect, EffectParams};
use crate::engine::AudioBuffer;
use crate::error::{NuevaError, Result};
use crate::impl_effect_common;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ============================================================================
// Constants
// ============================================================================

/// Minimum gain in dB (-96 dB = effectively silent)
const MIN_GAIN_DB: f32 = -96.0;

/// Maximum gain in dB (+24 dB)
const MAX_GAIN_DB: f32 = 24.0;

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert decibels to linear amplitude
///
/// # Arguments
/// * `db` - Value in decibels
///
/// # Returns
/// Linear amplitude value
#[inline]
fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

// ============================================================================
// Gain Effect
// ============================================================================

/// Simple gain adjustment effect
///
/// Provides volume control in dB with linear interpolation for smooth changes.
///
/// # Parameters
/// - `gain_db`: Gain in decibels (-96 to +24 dB)
///
/// # Example
/// ```ignore
/// use nueva::dsp::Gain;
/// use nueva::engine::AudioBuffer;
/// use nueva::dsp::effect::Effect;
///
/// let mut gain = Gain::new(-6.0); // -6 dB attenuation
/// let mut buffer = AudioBuffer::new(1024, ChannelLayout::Stereo);
/// // ... fill buffer with audio ...
/// gain.process(&mut buffer);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gain {
    params: EffectParams,
    gain_db: f32,
    #[serde(skip)]
    gain_linear: f32,
}

impl Gain {
    /// Create a new gain effect
    ///
    /// # Arguments
    /// * `gain_db` - Gain in decibels (-96 to +24 dB), clamped to valid range
    ///
    /// # Returns
    /// A new Gain effect instance
    pub fn new(gain_db: f32) -> Self {
        let clamped = gain_db.clamp(MIN_GAIN_DB, MAX_GAIN_DB);
        Self {
            params: EffectParams::default(),
            gain_db: clamped,
            gain_linear: db_to_linear(clamped),
        }
    }

    /// Set the gain in decibels
    ///
    /// # Arguments
    /// * `db` - New gain value (-96 to +24 dB), clamped to valid range
    pub fn set_gain_db(&mut self, db: f32) {
        self.gain_db = db.clamp(MIN_GAIN_DB, MAX_GAIN_DB);
        self.gain_linear = db_to_linear(self.gain_db);
    }

    /// Get the current gain in decibels
    ///
    /// # Returns
    /// Current gain value in dB
    pub fn gain_db(&self) -> f32 {
        self.gain_db
    }

    /// Get the current linear gain multiplier
    ///
    /// # Returns
    /// Linear gain value (for debugging/display)
    pub fn gain_linear(&self) -> f32 {
        self.gain_linear
    }
}

impl Default for Gain {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl Effect for Gain {
    impl_effect_common!(Gain, "gain", "Gain");

    fn process(&mut self, buffer: &mut AudioBuffer) {
        if !self.params.enabled {
            return;
        }

        // Unity gain optimization
        if (self.gain_linear - 1.0).abs() < f32::EPSILON {
            return;
        }

        // Apply gain to all channels
        for channel in 0..buffer.num_channels() {
            let samples = buffer.channel_mut(channel);
            for sample in samples.iter_mut() {
                *sample *= self.gain_linear;
            }
        }
    }

    fn prepare(&mut self, _sample_rate: u32, _max_block_size: usize) {
        // Update linear cache in case gain_db was deserialized
        self.gain_linear = db_to_linear(self.gain_db);
    }

    fn reset(&mut self) {
        // Gain has no internal state to reset
    }

    fn to_json(&self) -> Result<Value> {
        serde_json::to_value(self).map_err(|e| NuevaError::Serialization(e))
    }

    fn from_json(&mut self, json: &Value) -> Result<()> {
        if let Some(gain_db) = json.get("gain_db").and_then(|v| v.as_f64()) {
            self.set_gain_db(gain_db as f32);
        }
        if let Some(enabled) = json
            .get("params")
            .and_then(|p| p.get("enabled"))
            .and_then(|v| v.as_bool())
        {
            self.params.enabled = enabled;
        }
        if let Some(id) = json
            .get("params")
            .and_then(|p| p.get("id"))
            .and_then(|v| v.as_str())
        {
            self.params.id = id.to_string();
        }
        Ok(())
    }

    fn get_params(&self) -> Value {
        json!({
            "gain_db": self.gain_db,
            "gain_linear": self.gain_linear,
            "enabled": self.params.enabled
        })
    }

    fn set_param(&mut self, name: &str, value: &Value) -> Result<()> {
        match name {
            "gain_db" | "gain" => {
                if let Some(v) = value.as_f64() {
                    self.set_gain_db(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!(
                            "Invalid value for gain_db: expected number, got {:?}",
                            value
                        ),
                    })
                }
            }
            "enabled" => {
                if let Some(v) = value.as_bool() {
                    self.params.enabled = v;
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!(
                            "Invalid value for enabled: expected bool, got {:?}",
                            value
                        ),
                    })
                }
            }
            _ => Err(NuevaError::ProcessingError {
                reason: format!("Unknown parameter: {}", name),
            }),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::buffer::ChannelLayout;

    /// Helper to create a test buffer with known values
    fn create_test_buffer(value: f32, num_samples: usize) -> AudioBuffer {
        let mut buffer = AudioBuffer::new(num_samples, ChannelLayout::Stereo);
        for ch in 0..buffer.num_channels() {
            for i in 0..num_samples {
                buffer.set_sample(ch, i, value);
            }
        }
        buffer
    }

    #[test]
    fn test_gain_new() {
        let gain = Gain::new(-6.0);
        assert!((gain.gain_db() - (-6.0)).abs() < f32::EPSILON);
        // -6 dB ~= 0.501187
        assert!((gain.gain_linear() - 0.501187).abs() < 0.001);
    }

    #[test]
    fn test_gain_default() {
        let gain = Gain::default();
        assert!((gain.gain_db() - 0.0).abs() < f32::EPSILON);
        assert!((gain.gain_linear() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_gain_clamping() {
        // Test min clamping
        let gain_low = Gain::new(-200.0);
        assert!((gain_low.gain_db() - MIN_GAIN_DB).abs() < f32::EPSILON);

        // Test max clamping
        let gain_high = Gain::new(100.0);
        assert!((gain_high.gain_db() - MAX_GAIN_DB).abs() < f32::EPSILON);
    }

    #[test]
    fn test_gain_set() {
        let mut gain = Gain::new(0.0);
        gain.set_gain_db(-12.0);
        assert!((gain.gain_db() - (-12.0)).abs() < f32::EPSILON);
        // -12 dB ~= 0.251189
        assert!((gain.gain_linear() - 0.251189).abs() < 0.001);
    }

    #[test]
    fn test_gain_process() {
        let mut gain = Gain::new(-6.0);
        let mut buffer = create_test_buffer(1.0, 100);

        gain.process(&mut buffer);

        // All samples should be approximately 0.501187
        for ch in 0..buffer.num_channels() {
            for i in 0..buffer.num_samples() {
                let sample = buffer.get_sample(ch, i).unwrap();
                assert!((sample - 0.501187).abs() < 0.001);
            }
        }
    }

    #[test]
    fn test_gain_process_unity() {
        let mut gain = Gain::new(0.0);
        let mut buffer = create_test_buffer(0.5, 100);

        gain.process(&mut buffer);

        // Unity gain should leave samples unchanged
        for ch in 0..buffer.num_channels() {
            for i in 0..buffer.num_samples() {
                let sample = buffer.get_sample(ch, i).unwrap();
                assert!((sample - 0.5).abs() < f32::EPSILON);
            }
        }
    }

    #[test]
    fn test_gain_process_disabled() {
        let mut gain = Gain::new(-12.0);
        gain.set_enabled(false);
        let mut buffer = create_test_buffer(1.0, 100);

        gain.process(&mut buffer);

        // Disabled effect should not modify buffer
        for ch in 0..buffer.num_channels() {
            for i in 0..buffer.num_samples() {
                let sample = buffer.get_sample(ch, i).unwrap();
                assert!((sample - 1.0).abs() < f32::EPSILON);
            }
        }
    }

    #[test]
    fn test_gain_positive() {
        let mut gain = Gain::new(6.0);
        let mut buffer = create_test_buffer(0.5, 100);

        gain.process(&mut buffer);

        // +6 dB ~= 1.995262
        // 0.5 * 1.995262 ~= 0.997631
        for ch in 0..buffer.num_channels() {
            for i in 0..buffer.num_samples() {
                let sample = buffer.get_sample(ch, i).unwrap();
                assert!((sample - 0.997631).abs() < 0.01);
            }
        }
    }

    #[test]
    fn test_gain_effect_type() {
        let gain = Gain::new(0.0);
        assert_eq!(gain.effect_type(), "gain");
        assert_eq!(gain.display_name(), "Gain");
    }

    #[test]
    fn test_gain_get_params() {
        let gain = Gain::new(-3.0);
        let params = gain.get_params();

        assert!((params["gain_db"].as_f64().unwrap() - (-3.0)).abs() < 0.001);
        assert!(params["enabled"].as_bool().unwrap());
    }

    #[test]
    fn test_gain_set_param() {
        let mut gain = Gain::new(0.0);

        gain.set_param("gain_db", &json!(-6.0)).unwrap();
        assert!((gain.gain_db() - (-6.0)).abs() < f32::EPSILON);

        gain.set_param("enabled", &json!(false)).unwrap();
        assert!(!gain.is_enabled());
    }

    #[test]
    fn test_gain_set_param_invalid() {
        let mut gain = Gain::new(0.0);

        // Invalid type
        let result = gain.set_param("gain_db", &json!("not a number"));
        assert!(result.is_err());

        // Unknown parameter
        let result = gain.set_param("unknown", &json!(1.0));
        assert!(result.is_err());
    }

    #[test]
    fn test_gain_to_from_json() {
        let original = Gain::new(-9.0);
        let json = original.to_json().unwrap();

        let mut restored = Gain::new(0.0);
        restored.from_json(&json).unwrap();

        assert!((restored.gain_db() - (-9.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_gain_box_clone() {
        let gain = Gain::new(-6.0);
        let boxed: Box<dyn Effect> = Box::new(gain);
        let cloned = boxed.box_clone();

        assert_eq!(cloned.effect_type(), "gain");
    }

    #[test]
    fn test_gain_id() {
        let mut gain = Gain::new(0.0);
        let original_id = gain.id().to_string();

        // ID should be a UUID
        assert!(!original_id.is_empty());

        // Can set new ID
        gain.set_id("custom-gain-id".to_string());
        assert_eq!(gain.id(), "custom-gain-id");
    }

    #[test]
    fn test_gain_enabled() {
        let mut gain = Gain::new(0.0);
        assert!(gain.is_enabled());

        gain.set_enabled(false);
        assert!(!gain.is_enabled());

        gain.set_enabled(true);
        assert!(gain.is_enabled());
    }

    #[test]
    fn test_gain_prepare() {
        let mut gain = Gain::new(-6.0);
        // Modify gain_linear manually (simulating deserialization)
        gain.gain_linear = 0.0;

        gain.prepare(48000, 512);

        // prepare() should recalculate gain_linear
        assert!((gain.gain_linear() - 0.501187).abs() < 0.001);
    }
}
