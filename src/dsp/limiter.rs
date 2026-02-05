//! Limiter Effect
//!
//! Brickwall limiter per spec 4.2.8.
//! Prevents output from exceeding a specified ceiling level.

use crate::dsp::effect::{Effect, EffectParams};
use crate::engine::AudioBuffer;
use crate::error::{NuevaError, Result};
use crate::impl_effect_common;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ============================================================================
// Constants
// ============================================================================

/// Minimum ceiling in dB
const MIN_CEILING_DB: f32 = -12.0;
/// Maximum ceiling in dB
const MAX_CEILING_DB: f32 = 0.0;

/// Minimum release time in ms
const MIN_RELEASE_MS: f32 = 10.0;
/// Maximum release time in ms
const MAX_RELEASE_MS: f32 = 1000.0;

/// Default sample rate
const DEFAULT_SAMPLE_RATE: f32 = 48000.0;

/// Very fast attack time for brickwall limiting (0.1ms)
const ATTACK_MS: f32 = 0.1;

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert decibels to linear amplitude
#[inline]
fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Convert linear amplitude to decibels
#[inline]
fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * linear.log10()
    }
}

/// Calculate envelope coefficient from time constant
#[inline]
fn time_to_coeff(time_ms: f32, sample_rate: f32) -> f32 {
    (-1.0 / (time_ms * sample_rate / 1000.0)).exp()
}

// ============================================================================
// Limiter Effect
// ============================================================================

/// Brickwall limiter effect
///
/// Prevents audio from exceeding a specified ceiling level.
/// Uses a very fast attack and configurable release for transparent limiting.
///
/// # Parameters
/// - `ceiling_db`: Maximum output level (-12 to 0 dB)
/// - `release_ms`: Release time for gain recovery (10 to 1000 ms)
///
/// # Example
/// ```ignore
/// use nueva::dsp::Limiter;
/// use nueva::engine::AudioBuffer;
/// use nueva::dsp::effect::Effect;
///
/// let mut limiter = Limiter::new(-1.0); // -1 dBFS ceiling
/// limiter.set_release_ms(100.0);
/// // process audio...
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Limiter {
    params: EffectParams,
    ceiling_db: f32,
    release_ms: f32,
    #[serde(skip)]
    envelope: f32,
    #[serde(skip)]
    sample_rate: f32,
    #[serde(skip)]
    ceiling_linear: f32,
    #[serde(skip)]
    attack_coeff: f32,
    #[serde(skip)]
    release_coeff: f32,
}

impl Limiter {
    /// Create a new limiter with specified ceiling
    ///
    /// # Arguments
    /// * `ceiling_db` - Maximum output level (-12 to 0 dB)
    ///
    /// # Returns
    /// A new Limiter with default release settings
    pub fn new(ceiling_db: f32) -> Self {
        let clamped_ceiling = ceiling_db.clamp(MIN_CEILING_DB, MAX_CEILING_DB);
        let mut limiter = Self {
            params: EffectParams::default(),
            ceiling_db: clamped_ceiling,
            release_ms: 100.0,
            envelope: 0.0,
            sample_rate: DEFAULT_SAMPLE_RATE,
            ceiling_linear: db_to_linear(clamped_ceiling),
            attack_coeff: 0.0,
            release_coeff: 0.0,
        };
        limiter.update_coefficients();
        limiter
    }

    /// Set ceiling in dB
    pub fn set_ceiling_db(&mut self, db: f32) {
        self.ceiling_db = db.clamp(MIN_CEILING_DB, MAX_CEILING_DB);
        self.ceiling_linear = db_to_linear(self.ceiling_db);
    }

    /// Get ceiling in dB
    pub fn ceiling_db(&self) -> f32 {
        self.ceiling_db
    }

    /// Get ceiling in linear
    pub fn ceiling_linear(&self) -> f32 {
        self.ceiling_linear
    }

    /// Set release time in milliseconds
    pub fn set_release_ms(&mut self, ms: f32) {
        self.release_ms = ms.clamp(MIN_RELEASE_MS, MAX_RELEASE_MS);
        self.release_coeff = time_to_coeff(self.release_ms, self.sample_rate);
    }

    /// Get release time in milliseconds
    pub fn release_ms(&self) -> f32 {
        self.release_ms
    }

    /// Update internal coefficients
    fn update_coefficients(&mut self) {
        self.attack_coeff = time_to_coeff(ATTACK_MS, self.sample_rate);
        self.release_coeff = time_to_coeff(self.release_ms, self.sample_rate);
        self.ceiling_linear = db_to_linear(self.ceiling_db);
    }
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new(-1.0)
    }
}

impl Effect for Limiter {
    impl_effect_common!(Limiter, "limiter", "Limiter");

    fn process(&mut self, buffer: &mut AudioBuffer) {
        if !self.params.enabled {
            return;
        }

        let num_channels = buffer.num_channels();
        let num_samples = buffer.num_samples();

        if num_channels == 0 || num_samples == 0 {
            return;
        }

        // Process sample by sample
        for i in 0..num_samples {
            // Calculate peak level across all channels for this sample
            let mut peak = 0.0_f32;
            for ch in 0..num_channels {
                let sample = buffer.get_sample(ch, i).unwrap_or(0.0);
                peak = peak.max(sample.abs());
            }

            // Calculate required gain reduction
            let target_gain_reduction = if peak > self.ceiling_linear {
                // Need to reduce gain so peak becomes ceiling
                let required_db = linear_to_db(peak) - self.ceiling_db;
                required_db.max(0.0)
            } else {
                0.0
            };

            // Smooth envelope with very fast attack, smooth release
            if target_gain_reduction > self.envelope {
                // Attack - very fast (nearly instant for brickwall limiting)
                self.envelope = self.attack_coeff * self.envelope
                    + (1.0 - self.attack_coeff) * target_gain_reduction;
            } else {
                // Release - configurable
                self.envelope = self.release_coeff * self.envelope
                    + (1.0 - self.release_coeff) * target_gain_reduction;
            }

            // Convert envelope (gain reduction in dB) to linear gain
            let gain_linear = db_to_linear(-self.envelope);

            // Apply gain to all channels
            for ch in 0..num_channels {
                let samples = buffer.channel_mut(ch);
                samples[i] *= gain_linear;

                // Final hard clip to ensure ceiling is never exceeded
                // (belt-and-suspenders approach for true brickwall limiting)
                if samples[i].abs() > self.ceiling_linear {
                    samples[i] = samples[i].signum() * self.ceiling_linear;
                }
            }
        }
    }

    fn prepare(&mut self, sample_rate: u32, _max_block_size: usize) {
        self.sample_rate = sample_rate as f32;
        self.update_coefficients();
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
    }

    fn to_json(&self) -> Result<Value> {
        serde_json::to_value(self).map_err(|e| NuevaError::Serialization(e))
    }

    fn from_json(&mut self, json: &Value) -> Result<()> {
        if let Some(v) = json.get("ceiling_db").and_then(|v| v.as_f64()) {
            self.set_ceiling_db(v as f32);
        }
        if let Some(v) = json.get("release_ms").and_then(|v| v.as_f64()) {
            self.set_release_ms(v as f32);
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
            "ceiling_db": self.ceiling_db,
            "release_ms": self.release_ms,
            "enabled": self.params.enabled
        })
    }

    fn set_param(&mut self, name: &str, value: &Value) -> Result<()> {
        match name {
            "ceiling_db" | "ceiling" => {
                if let Some(v) = value.as_f64() {
                    self.set_ceiling_db(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!(
                            "Invalid value for ceiling_db: expected number, got {:?}",
                            value
                        ),
                    })
                }
            }
            "release_ms" | "release" => {
                if let Some(v) = value.as_f64() {
                    self.set_release_ms(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!(
                            "Invalid value for release_ms: expected number, got {:?}",
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
    fn test_limiter_new() {
        let limiter = Limiter::new(-1.0);
        assert!((limiter.ceiling_db() - (-1.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_limiter_default() {
        let limiter = Limiter::default();
        assert!((limiter.ceiling_db() - (-1.0)).abs() < f32::EPSILON);
        assert!((limiter.release_ms() - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_limiter_clamping() {
        // Ceiling clamping
        let limiter = Limiter::new(-20.0);
        assert!((limiter.ceiling_db() - MIN_CEILING_DB).abs() < f32::EPSILON);

        let limiter = Limiter::new(10.0);
        assert!((limiter.ceiling_db() - MAX_CEILING_DB).abs() < f32::EPSILON);
    }

    #[test]
    fn test_limiter_setters() {
        let mut limiter = Limiter::new(-1.0);

        limiter.set_ceiling_db(-3.0);
        assert!((limiter.ceiling_db() - (-3.0)).abs() < f32::EPSILON);

        limiter.set_release_ms(200.0);
        assert!((limiter.release_ms() - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_limiter_release_clamping() {
        let mut limiter = Limiter::new(-1.0);

        limiter.set_release_ms(5.0);
        assert!((limiter.release_ms() - MIN_RELEASE_MS).abs() < f32::EPSILON);

        limiter.set_release_ms(2000.0);
        assert!((limiter.release_ms() - MAX_RELEASE_MS).abs() < f32::EPSILON);
    }

    #[test]
    fn test_limiter_process_below_ceiling() {
        let mut limiter = Limiter::new(-1.0);
        limiter.prepare(48000, 512);

        // Signal at -6 dB (below ceiling of -1 dB)
        let level = db_to_linear(-6.0);
        let mut buffer = create_test_buffer(level, 1000);
        let original_sample = buffer.get_sample(0, 500).unwrap();

        limiter.process(&mut buffer);

        // Signal below ceiling should pass through unchanged
        let processed_sample = buffer.get_sample(0, 500).unwrap();
        assert!((processed_sample - original_sample).abs() < 0.01);
    }

    #[test]
    fn test_limiter_process_above_ceiling() {
        let mut limiter = Limiter::new(-6.0);
        limiter.prepare(48000, 512);

        // Signal at 0 dB (above ceiling of -6 dB)
        let mut buffer = create_test_buffer(1.0, 10000);

        limiter.process(&mut buffer);

        // All samples should be at or below ceiling
        let ceiling_linear = limiter.ceiling_linear();
        for ch in 0..buffer.num_channels() {
            for i in 0..buffer.num_samples() {
                let sample = buffer.get_sample(ch, i).unwrap();
                assert!(
                    sample.abs() <= ceiling_linear + 0.001,
                    "Sample {} at index {} exceeds ceiling {}",
                    sample,
                    i,
                    ceiling_linear
                );
            }
        }
    }

    #[test]
    fn test_limiter_brickwall() {
        let mut limiter = Limiter::new(-3.0);
        limiter.prepare(48000, 512);

        // Create buffer with samples exceeding ceiling
        let mut buffer = AudioBuffer::new(100, ChannelLayout::Stereo);
        for i in 0..100 {
            buffer.set_sample(0, i, 2.0); // Way over 0 dB
            buffer.set_sample(1, i, 2.0);
        }

        limiter.process(&mut buffer);

        // Verify absolute brickwall - no sample should exceed ceiling
        let ceiling_linear = limiter.ceiling_linear();
        for ch in 0..buffer.num_channels() {
            for i in 0..buffer.num_samples() {
                let sample = buffer.get_sample(ch, i).unwrap();
                assert!(
                    sample.abs() <= ceiling_linear,
                    "Brickwall failed: sample {} exceeds ceiling {}",
                    sample,
                    ceiling_linear
                );
            }
        }
    }

    #[test]
    fn test_limiter_process_disabled() {
        let mut limiter = Limiter::new(-6.0);
        limiter.set_enabled(false);
        limiter.prepare(48000, 512);

        let mut buffer = create_test_buffer(1.0, 100);

        limiter.process(&mut buffer);

        // Disabled effect should not modify buffer
        let sample = buffer.get_sample(0, 50).unwrap();
        assert!((sample - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_limiter_effect_type() {
        let limiter = Limiter::new(-1.0);
        assert_eq!(limiter.effect_type(), "limiter");
        assert_eq!(limiter.display_name(), "Limiter");
    }

    #[test]
    fn test_limiter_get_params() {
        let mut limiter = Limiter::new(-3.0);
        limiter.set_release_ms(200.0);

        let params = limiter.get_params();

        assert!((params["ceiling_db"].as_f64().unwrap() - (-3.0)).abs() < 0.001);
        assert!((params["release_ms"].as_f64().unwrap() - 200.0).abs() < 0.001);
        assert!(params["enabled"].as_bool().unwrap());
    }

    #[test]
    fn test_limiter_set_param() {
        let mut limiter = Limiter::new(-1.0);

        limiter.set_param("ceiling_db", &json!(-6.0)).unwrap();
        assert!((limiter.ceiling_db() - (-6.0)).abs() < f32::EPSILON);

        limiter.set_param("release_ms", &json!(200.0)).unwrap();
        assert!((limiter.release_ms() - 200.0).abs() < f32::EPSILON);

        limiter.set_param("enabled", &json!(false)).unwrap();
        assert!(!limiter.is_enabled());
    }

    #[test]
    fn test_limiter_set_param_invalid() {
        let mut limiter = Limiter::new(-1.0);

        let result = limiter.set_param("ceiling_db", &json!("not a number"));
        assert!(result.is_err());

        let result = limiter.set_param("unknown", &json!(1.0));
        assert!(result.is_err());
    }

    #[test]
    fn test_limiter_to_from_json() {
        let mut original = Limiter::new(-3.0);
        original.set_release_ms(200.0);

        let json = original.to_json().unwrap();

        let mut restored = Limiter::new(-1.0);
        restored.from_json(&json).unwrap();

        assert!((restored.ceiling_db() - (-3.0)).abs() < f32::EPSILON);
        assert!((restored.release_ms() - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_limiter_reset() {
        let mut limiter = Limiter::new(-6.0);
        limiter.prepare(48000, 512);

        // Process some audio to build up envelope
        let mut buffer = create_test_buffer(1.0, 1000);
        limiter.process(&mut buffer);

        // Envelope should be non-zero
        assert!(limiter.envelope > 0.0);

        // Reset should clear envelope
        limiter.reset();
        assert!((limiter.envelope - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_limiter_box_clone() {
        let limiter = Limiter::new(-1.0);
        let boxed: Box<dyn Effect> = Box::new(limiter);
        let cloned = boxed.box_clone();

        assert_eq!(cloned.effect_type(), "limiter");
    }

    #[test]
    fn test_limiter_id() {
        let mut limiter = Limiter::new(-1.0);
        let original_id = limiter.id().to_string();

        assert!(!original_id.is_empty());

        limiter.set_id("custom-limiter-id".to_string());
        assert_eq!(limiter.id(), "custom-limiter-id");
    }

    #[test]
    fn test_limiter_prepare() {
        let mut limiter = Limiter::new(-1.0);
        limiter.set_release_ms(100.0);

        limiter.prepare(96000, 1024);

        assert!((limiter.sample_rate - 96000.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_limiter_ceiling_linear() {
        let limiter = Limiter::new(-6.0);
        let ceiling_linear = limiter.ceiling_linear();
        // -6 dB ~= 0.501187
        assert!((ceiling_linear - 0.501187).abs() < 0.001);
    }

    #[test]
    fn test_limiter_preserves_stereo_relationship() {
        let mut limiter = Limiter::new(-6.0);
        limiter.prepare(48000, 512);

        // Create stereo buffer with different levels but both above ceiling
        let mut buffer = AudioBuffer::new(1000, ChannelLayout::Stereo);
        for i in 0..1000 {
            buffer.set_sample(0, i, 0.8); // Left channel
            buffer.set_sample(1, i, 0.4); // Right channel (half of left)
        }

        limiter.process(&mut buffer);

        // After processing, check that stereo relationship is maintained
        // (both channels should be reduced by the same ratio)
        let left = buffer.get_sample(0, 900).unwrap();
        let right = buffer.get_sample(1, 900).unwrap();

        // Right should still be approximately half of left
        let ratio = right / left;
        assert!(
            (ratio - 0.5).abs() < 0.1,
            "Stereo relationship not preserved: ratio = {}",
            ratio
        );
    }
}
