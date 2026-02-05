//! Compressor Effect
//!
//! Dynamic range compressor per spec 4.2.3.
//! Provides threshold-based gain reduction with soft knee support.

use crate::dsp::effect::{Effect, EffectParams};
use crate::engine::AudioBuffer;
use crate::error::{NuevaError, Result};
use crate::impl_effect_common;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ============================================================================
// Constants
// ============================================================================

/// Minimum threshold in dB
const MIN_THRESHOLD_DB: f32 = -60.0;
/// Maximum threshold in dB
const MAX_THRESHOLD_DB: f32 = 0.0;

/// Minimum compression ratio
const MIN_RATIO: f32 = 1.0;
/// Maximum compression ratio
const MAX_RATIO: f32 = 20.0;

/// Minimum attack time in ms
const MIN_ATTACK_MS: f32 = 0.1;
/// Maximum attack time in ms
const MAX_ATTACK_MS: f32 = 100.0;

/// Minimum release time in ms
const MIN_RELEASE_MS: f32 = 10.0;
/// Maximum release time in ms
const MAX_RELEASE_MS: f32 = 1000.0;

/// Minimum knee width in dB
const MIN_KNEE_DB: f32 = 0.0;
/// Maximum knee width in dB
const MAX_KNEE_DB: f32 = 12.0;

/// Minimum makeup gain in dB
const MIN_MAKEUP_GAIN_DB: f32 = 0.0;
/// Maximum makeup gain in dB
const MAX_MAKEUP_GAIN_DB: f32 = 24.0;

/// Default sample rate
const DEFAULT_SAMPLE_RATE: f32 = 48000.0;

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
// Compressor Effect
// ============================================================================

/// Dynamic range compressor effect
///
/// Reduces the dynamic range of audio by attenuating signals above a threshold.
/// Supports soft knee compression and auto makeup gain.
///
/// # Parameters
/// - `threshold_db`: Level above which compression begins (-60 to 0 dB)
/// - `ratio`: Compression ratio (1:1 to 20:1)
/// - `attack_ms`: Attack time (0.1 to 100 ms)
/// - `release_ms`: Release time (10 to 1000 ms)
/// - `knee_db`: Knee width for soft knee compression (0 to 12 dB)
/// - `makeup_gain_db`: Output gain compensation (0 to 24 dB)
/// - `auto_makeup`: Automatically calculate makeup gain
///
/// # Example
/// ```ignore
/// use nueva::dsp::Compressor;
/// use nueva::engine::AudioBuffer;
/// use nueva::dsp::effect::Effect;
///
/// let mut comp = Compressor::new(-18.0, 4.0);
/// comp.set_attack_ms(10.0);
/// comp.set_release_ms(100.0);
/// // process audio...
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Compressor {
    params: EffectParams,
    threshold_db: f32,
    ratio: f32,
    attack_ms: f32,
    release_ms: f32,
    knee_db: f32,
    makeup_gain_db: f32,
    auto_makeup: bool,
    #[serde(skip)]
    envelope: f32,
    #[serde(skip)]
    sample_rate: f32,
    #[serde(skip)]
    attack_coeff: f32,
    #[serde(skip)]
    release_coeff: f32,
}

impl Compressor {
    /// Create a new compressor with threshold and ratio
    ///
    /// # Arguments
    /// * `threshold_db` - Threshold in dB (-60 to 0 dB)
    /// * `ratio` - Compression ratio (1.0 to 20.0)
    ///
    /// # Returns
    /// A new Compressor with default attack/release/knee settings
    pub fn new(threshold_db: f32, ratio: f32) -> Self {
        let mut comp = Self {
            params: EffectParams::default(),
            threshold_db: threshold_db.clamp(MIN_THRESHOLD_DB, MAX_THRESHOLD_DB),
            ratio: ratio.clamp(MIN_RATIO, MAX_RATIO),
            attack_ms: 10.0,
            release_ms: 100.0,
            knee_db: 0.0,
            makeup_gain_db: 0.0,
            auto_makeup: false,
            envelope: 0.0,
            sample_rate: DEFAULT_SAMPLE_RATE,
            attack_coeff: 0.0,
            release_coeff: 0.0,
        };
        comp.update_coefficients();
        comp
    }

    /// Set threshold in dB
    pub fn set_threshold_db(&mut self, db: f32) {
        self.threshold_db = db.clamp(MIN_THRESHOLD_DB, MAX_THRESHOLD_DB);
    }

    /// Get threshold in dB
    pub fn threshold_db(&self) -> f32 {
        self.threshold_db
    }

    /// Set compression ratio
    pub fn set_ratio(&mut self, ratio: f32) {
        self.ratio = ratio.clamp(MIN_RATIO, MAX_RATIO);
    }

    /// Get compression ratio
    pub fn ratio(&self) -> f32 {
        self.ratio
    }

    /// Set attack time in milliseconds
    pub fn set_attack_ms(&mut self, ms: f32) {
        self.attack_ms = ms.clamp(MIN_ATTACK_MS, MAX_ATTACK_MS);
        self.attack_coeff = time_to_coeff(self.attack_ms, self.sample_rate);
    }

    /// Get attack time in milliseconds
    pub fn attack_ms(&self) -> f32 {
        self.attack_ms
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

    /// Set knee width in dB
    pub fn set_knee_db(&mut self, db: f32) {
        self.knee_db = db.clamp(MIN_KNEE_DB, MAX_KNEE_DB);
    }

    /// Get knee width in dB
    pub fn knee_db(&self) -> f32 {
        self.knee_db
    }

    /// Set makeup gain in dB
    pub fn set_makeup_gain_db(&mut self, db: f32) {
        self.makeup_gain_db = db.clamp(MIN_MAKEUP_GAIN_DB, MAX_MAKEUP_GAIN_DB);
    }

    /// Get makeup gain in dB
    pub fn makeup_gain_db(&self) -> f32 {
        self.makeup_gain_db
    }

    /// Enable or disable auto makeup gain
    pub fn set_auto_makeup(&mut self, enabled: bool) {
        self.auto_makeup = enabled;
    }

    /// Check if auto makeup gain is enabled
    pub fn auto_makeup(&self) -> bool {
        self.auto_makeup
    }

    /// Calculate auto makeup gain based on threshold and ratio
    ///
    /// Estimates the gain reduction at threshold and compensates.
    pub fn calculate_auto_makeup(&self) -> f32 {
        // Estimate average gain reduction assuming signal at threshold
        // For a signal at threshold with given ratio, output would be threshold
        // With 4:1 ratio and -18dB threshold, 18dB above threshold becomes 4.5dB above
        // So we lose about (ratio-1)/ratio * |threshold| dB on average
        let reduction_estimate = (self.ratio - 1.0) / self.ratio * self.threshold_db.abs();
        // Return a portion of this as makeup (typically half works well)
        (reduction_estimate * 0.5).clamp(MIN_MAKEUP_GAIN_DB, MAX_MAKEUP_GAIN_DB)
    }

    /// Update internal coefficients (called after sample rate or time changes)
    fn update_coefficients(&mut self) {
        self.attack_coeff = time_to_coeff(self.attack_ms, self.sample_rate);
        self.release_coeff = time_to_coeff(self.release_ms, self.sample_rate);
    }

    /// Apply soft knee compression curve
    ///
    /// Returns gain reduction in dB for given input level in dB.
    fn compute_gain_reduction(&self, input_db: f32) -> f32 {
        let threshold = self.threshold_db;
        let knee = self.knee_db;
        let ratio = self.ratio;

        if knee <= 0.0 {
            // Hard knee
            if input_db <= threshold {
                0.0 // No gain reduction below threshold
            } else {
                // Gain reduction = input - threshold - (input - threshold) / ratio
                //                = (input - threshold) * (1 - 1/ratio)
                let over = input_db - threshold;
                over * (1.0 - 1.0 / ratio)
            }
        } else {
            // Soft knee
            let knee_start = threshold - knee / 2.0;
            let knee_end = threshold + knee / 2.0;

            if input_db < knee_start {
                // Below knee region - no compression
                0.0
            } else if input_db > knee_end {
                // Above knee region - full compression
                let over = input_db - threshold;
                over * (1.0 - 1.0 / ratio)
            } else {
                // In knee region - quadratic interpolation
                // Position in knee (0 to 1)
                let knee_pos = (input_db - knee_start) / knee;
                // Smooth transition from 0 to full compression
                let compression_amount = knee_pos * knee_pos;
                let over = input_db - knee_start;
                // Gradually apply compression ratio
                over * compression_amount * (1.0 - 1.0 / ratio) / 2.0
            }
        }
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new(-18.0, 4.0)
    }
}

impl Effect for Compressor {
    impl_effect_common!(Compressor, "compressor", "Compressor");

    fn process(&mut self, buffer: &mut AudioBuffer) {
        if !self.params.enabled {
            return;
        }

        let num_channels = buffer.num_channels();
        let num_samples = buffer.num_samples();

        if num_channels == 0 || num_samples == 0 {
            return;
        }

        // Get makeup gain (auto or manual)
        let makeup_gain = if self.auto_makeup {
            self.calculate_auto_makeup()
        } else {
            self.makeup_gain_db
        };
        let makeup_linear = db_to_linear(makeup_gain);

        // Process sample by sample
        for i in 0..num_samples {
            // Calculate peak level across all channels for this sample
            let mut peak = 0.0_f32;
            for ch in 0..num_channels {
                let sample = buffer.get_sample(ch, i).unwrap_or(0.0);
                peak = peak.max(sample.abs());
            }

            // Convert to dB
            let input_db = linear_to_db(peak);

            // Calculate target gain reduction
            let gain_reduction_db = self.compute_gain_reduction(input_db);

            // Smooth envelope with attack/release
            let target_envelope = gain_reduction_db;
            if target_envelope > self.envelope {
                // Attack (increasing gain reduction)
                self.envelope =
                    self.attack_coeff * self.envelope + (1.0 - self.attack_coeff) * target_envelope;
            } else {
                // Release (decreasing gain reduction)
                self.envelope = self.release_coeff * self.envelope
                    + (1.0 - self.release_coeff) * target_envelope;
            }

            // Convert envelope (gain reduction in dB) to linear gain
            let gain_linear = db_to_linear(-self.envelope) * makeup_linear;

            // Apply gain to all channels
            for ch in 0..num_channels {
                let samples = buffer.channel_mut(ch);
                samples[i] *= gain_linear;
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
        if let Some(v) = json.get("threshold_db").and_then(|v| v.as_f64()) {
            self.set_threshold_db(v as f32);
        }
        if let Some(v) = json.get("ratio").and_then(|v| v.as_f64()) {
            self.set_ratio(v as f32);
        }
        if let Some(v) = json.get("attack_ms").and_then(|v| v.as_f64()) {
            self.set_attack_ms(v as f32);
        }
        if let Some(v) = json.get("release_ms").and_then(|v| v.as_f64()) {
            self.set_release_ms(v as f32);
        }
        if let Some(v) = json.get("knee_db").and_then(|v| v.as_f64()) {
            self.set_knee_db(v as f32);
        }
        if let Some(v) = json.get("makeup_gain_db").and_then(|v| v.as_f64()) {
            self.set_makeup_gain_db(v as f32);
        }
        if let Some(v) = json.get("auto_makeup").and_then(|v| v.as_bool()) {
            self.set_auto_makeup(v);
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
            "threshold_db": self.threshold_db,
            "ratio": self.ratio,
            "attack_ms": self.attack_ms,
            "release_ms": self.release_ms,
            "knee_db": self.knee_db,
            "makeup_gain_db": self.makeup_gain_db,
            "auto_makeup": self.auto_makeup,
            "enabled": self.params.enabled
        })
    }

    fn set_param(&mut self, name: &str, value: &Value) -> Result<()> {
        match name {
            "threshold_db" | "threshold" => {
                if let Some(v) = value.as_f64() {
                    self.set_threshold_db(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!(
                            "Invalid value for threshold_db: expected number, got {:?}",
                            value
                        ),
                    })
                }
            }
            "ratio" => {
                if let Some(v) = value.as_f64() {
                    self.set_ratio(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!(
                            "Invalid value for ratio: expected number, got {:?}",
                            value
                        ),
                    })
                }
            }
            "attack_ms" | "attack" => {
                if let Some(v) = value.as_f64() {
                    self.set_attack_ms(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!(
                            "Invalid value for attack_ms: expected number, got {:?}",
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
            "knee_db" | "knee" => {
                if let Some(v) = value.as_f64() {
                    self.set_knee_db(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!(
                            "Invalid value for knee_db: expected number, got {:?}",
                            value
                        ),
                    })
                }
            }
            "makeup_gain_db" | "makeup_gain" | "makeup" => {
                if let Some(v) = value.as_f64() {
                    self.set_makeup_gain_db(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!(
                            "Invalid value for makeup_gain_db: expected number, got {:?}",
                            value
                        ),
                    })
                }
            }
            "auto_makeup" => {
                if let Some(v) = value.as_bool() {
                    self.set_auto_makeup(v);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!(
                            "Invalid value for auto_makeup: expected bool, got {:?}",
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
    fn test_compressor_new() {
        let comp = Compressor::new(-18.0, 4.0);
        assert!((comp.threshold_db() - (-18.0)).abs() < f32::EPSILON);
        assert!((comp.ratio() - 4.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compressor_default() {
        let comp = Compressor::default();
        assert!((comp.threshold_db() - (-18.0)).abs() < f32::EPSILON);
        assert!((comp.ratio() - 4.0).abs() < f32::EPSILON);
        assert!((comp.attack_ms() - 10.0).abs() < f32::EPSILON);
        assert!((comp.release_ms() - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compressor_clamping() {
        // Threshold clamping
        let comp = Compressor::new(-100.0, 4.0);
        assert!((comp.threshold_db() - MIN_THRESHOLD_DB).abs() < f32::EPSILON);

        let comp = Compressor::new(10.0, 4.0);
        assert!((comp.threshold_db() - MAX_THRESHOLD_DB).abs() < f32::EPSILON);

        // Ratio clamping
        let comp = Compressor::new(-18.0, 0.5);
        assert!((comp.ratio() - MIN_RATIO).abs() < f32::EPSILON);

        let comp = Compressor::new(-18.0, 100.0);
        assert!((comp.ratio() - MAX_RATIO).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compressor_setters() {
        let mut comp = Compressor::new(-18.0, 4.0);

        comp.set_threshold_db(-24.0);
        assert!((comp.threshold_db() - (-24.0)).abs() < f32::EPSILON);

        comp.set_ratio(8.0);
        assert!((comp.ratio() - 8.0).abs() < f32::EPSILON);

        comp.set_attack_ms(5.0);
        assert!((comp.attack_ms() - 5.0).abs() < f32::EPSILON);

        comp.set_release_ms(200.0);
        assert!((comp.release_ms() - 200.0).abs() < f32::EPSILON);

        comp.set_knee_db(6.0);
        assert!((comp.knee_db() - 6.0).abs() < f32::EPSILON);

        comp.set_makeup_gain_db(3.0);
        assert!((comp.makeup_gain_db() - 3.0).abs() < f32::EPSILON);

        comp.set_auto_makeup(true);
        assert!(comp.auto_makeup());
    }

    #[test]
    fn test_compressor_process_below_threshold() {
        let mut comp = Compressor::new(-6.0, 4.0);
        comp.prepare(48000, 512);

        // Signal at -12 dB (below threshold of -6 dB)
        let mut buffer = create_test_buffer(0.25, 1000);
        let original_sample = buffer.get_sample(0, 500).unwrap();

        comp.process(&mut buffer);

        // Signal below threshold should pass through mostly unchanged
        let processed_sample = buffer.get_sample(0, 500).unwrap();
        // Allow some tolerance for envelope smoothing at start
        assert!((processed_sample - original_sample).abs() < 0.1);
    }

    #[test]
    fn test_compressor_process_above_threshold() {
        let mut comp = Compressor::new(-12.0, 4.0);
        comp.set_attack_ms(0.1); // Very fast attack
        comp.prepare(48000, 512);

        // Signal at 0 dB (well above threshold of -12 dB)
        let mut buffer = create_test_buffer(1.0, 10000);

        comp.process(&mut buffer);

        // Signal should be compressed - peak should be reduced
        let processed_sample = buffer.get_sample(0, 9000).unwrap();
        // With 4:1 ratio, 12dB over threshold becomes 3dB over, so output ~-9dB
        // Should be significantly less than 1.0
        assert!(processed_sample < 0.8);
    }

    #[test]
    fn test_compressor_process_disabled() {
        let mut comp = Compressor::new(-18.0, 4.0);
        comp.set_enabled(false);
        comp.prepare(48000, 512);

        let mut buffer = create_test_buffer(1.0, 100);

        comp.process(&mut buffer);

        // Disabled effect should not modify buffer
        let sample = buffer.get_sample(0, 50).unwrap();
        assert!((sample - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compressor_soft_knee() {
        let mut comp = Compressor::new(-18.0, 4.0);
        comp.set_knee_db(6.0);
        comp.prepare(48000, 512);

        // Test gain reduction calculation in knee region
        let gr_in_knee = comp.compute_gain_reduction(-18.0);
        let gr_above_knee = comp.compute_gain_reduction(-12.0);

        // Gain reduction in knee should be less than full compression
        assert!(gr_in_knee < gr_above_knee);
    }

    #[test]
    fn test_compressor_hard_knee() {
        let comp = Compressor::new(-18.0, 4.0);

        // Below threshold
        let gr_below = comp.compute_gain_reduction(-24.0);
        assert!((gr_below - 0.0).abs() < f32::EPSILON);

        // At threshold
        let gr_at = comp.compute_gain_reduction(-18.0);
        assert!((gr_at - 0.0).abs() < f32::EPSILON);

        // Above threshold (6dB over, 4:1 ratio = 4.5dB reduction)
        let gr_above = comp.compute_gain_reduction(-12.0);
        assert!((gr_above - 4.5).abs() < 0.1);
    }

    #[test]
    fn test_compressor_auto_makeup() {
        let comp = Compressor::new(-18.0, 4.0);
        let auto_makeup = comp.calculate_auto_makeup();

        // Auto makeup should be positive
        assert!(auto_makeup > 0.0);
        assert!(auto_makeup <= MAX_MAKEUP_GAIN_DB);
    }

    #[test]
    fn test_compressor_effect_type() {
        let comp = Compressor::new(-18.0, 4.0);
        assert_eq!(comp.effect_type(), "compressor");
        assert_eq!(comp.display_name(), "Compressor");
    }

    #[test]
    fn test_compressor_get_params() {
        let mut comp = Compressor::new(-24.0, 8.0);
        comp.set_attack_ms(5.0);
        comp.set_release_ms(150.0);
        comp.set_knee_db(3.0);
        comp.set_makeup_gain_db(6.0);
        comp.set_auto_makeup(true);

        let params = comp.get_params();

        assert!((params["threshold_db"].as_f64().unwrap() - (-24.0)).abs() < 0.001);
        assert!((params["ratio"].as_f64().unwrap() - 8.0).abs() < 0.001);
        assert!((params["attack_ms"].as_f64().unwrap() - 5.0).abs() < 0.001);
        assert!((params["release_ms"].as_f64().unwrap() - 150.0).abs() < 0.001);
        assert!((params["knee_db"].as_f64().unwrap() - 3.0).abs() < 0.001);
        assert!((params["makeup_gain_db"].as_f64().unwrap() - 6.0).abs() < 0.001);
        assert!(params["auto_makeup"].as_bool().unwrap());
    }

    #[test]
    fn test_compressor_set_param() {
        let mut comp = Compressor::new(-18.0, 4.0);

        comp.set_param("threshold_db", &json!(-24.0)).unwrap();
        assert!((comp.threshold_db() - (-24.0)).abs() < f32::EPSILON);

        comp.set_param("ratio", &json!(8.0)).unwrap();
        assert!((comp.ratio() - 8.0).abs() < f32::EPSILON);

        comp.set_param("attack_ms", &json!(5.0)).unwrap();
        assert!((comp.attack_ms() - 5.0).abs() < f32::EPSILON);

        comp.set_param("release_ms", &json!(200.0)).unwrap();
        assert!((comp.release_ms() - 200.0).abs() < f32::EPSILON);

        comp.set_param("knee_db", &json!(6.0)).unwrap();
        assert!((comp.knee_db() - 6.0).abs() < f32::EPSILON);

        comp.set_param("makeup_gain_db", &json!(3.0)).unwrap();
        assert!((comp.makeup_gain_db() - 3.0).abs() < f32::EPSILON);

        comp.set_param("auto_makeup", &json!(true)).unwrap();
        assert!(comp.auto_makeup());

        comp.set_param("enabled", &json!(false)).unwrap();
        assert!(!comp.is_enabled());
    }

    #[test]
    fn test_compressor_set_param_invalid() {
        let mut comp = Compressor::new(-18.0, 4.0);

        let result = comp.set_param("threshold_db", &json!("not a number"));
        assert!(result.is_err());

        let result = comp.set_param("unknown", &json!(1.0));
        assert!(result.is_err());
    }

    #[test]
    fn test_compressor_to_from_json() {
        let mut original = Compressor::new(-24.0, 8.0);
        original.set_attack_ms(5.0);
        original.set_release_ms(200.0);
        original.set_knee_db(6.0);
        original.set_makeup_gain_db(3.0);

        let json = original.to_json().unwrap();

        let mut restored = Compressor::new(-18.0, 4.0);
        restored.from_json(&json).unwrap();

        assert!((restored.threshold_db() - (-24.0)).abs() < f32::EPSILON);
        assert!((restored.ratio() - 8.0).abs() < f32::EPSILON);
        assert!((restored.attack_ms() - 5.0).abs() < f32::EPSILON);
        assert!((restored.release_ms() - 200.0).abs() < f32::EPSILON);
        assert!((restored.knee_db() - 6.0).abs() < f32::EPSILON);
        assert!((restored.makeup_gain_db() - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compressor_reset() {
        let mut comp = Compressor::new(-18.0, 4.0);
        comp.prepare(48000, 512);

        // Process some audio to build up envelope
        let mut buffer = create_test_buffer(1.0, 1000);
        comp.process(&mut buffer);

        // Envelope should be non-zero
        assert!(comp.envelope > 0.0);

        // Reset should clear envelope
        comp.reset();
        assert!((comp.envelope - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compressor_box_clone() {
        let comp = Compressor::new(-18.0, 4.0);
        let boxed: Box<dyn Effect> = Box::new(comp);
        let cloned = boxed.box_clone();

        assert_eq!(cloned.effect_type(), "compressor");
    }

    #[test]
    fn test_compressor_id() {
        let mut comp = Compressor::new(-18.0, 4.0);
        let original_id = comp.id().to_string();

        assert!(!original_id.is_empty());

        comp.set_id("custom-compressor-id".to_string());
        assert_eq!(comp.id(), "custom-compressor-id");
    }

    #[test]
    fn test_compressor_prepare() {
        let mut comp = Compressor::new(-18.0, 4.0);
        comp.set_attack_ms(10.0);
        comp.set_release_ms(100.0);

        // Call prepare with different sample rate
        comp.prepare(96000, 1024);

        // Coefficients should be updated (we can't directly test them,
        // but we can verify prepare doesn't panic)
        assert!((comp.sample_rate - 96000.0).abs() < f32::EPSILON);
    }
}
