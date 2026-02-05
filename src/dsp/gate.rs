//! Gate Effect
//!
//! Noise gate per spec 4.2.7.
//! Attenuates audio below a specified threshold level.

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
const MIN_THRESHOLD_DB: f32 = -80.0;
/// Maximum threshold in dB
const MAX_THRESHOLD_DB: f32 = 0.0;

/// Minimum attack time in ms
const MIN_ATTACK_MS: f32 = 0.1;
/// Maximum attack time in ms
const MAX_ATTACK_MS: f32 = 50.0;

/// Minimum release time in ms
const MIN_RELEASE_MS: f32 = 10.0;
/// Maximum release time in ms
const MAX_RELEASE_MS: f32 = 500.0;

/// Minimum hold time in ms
const MIN_HOLD_MS: f32 = 0.0;
/// Maximum hold time in ms
const MAX_HOLD_MS: f32 = 100.0;

/// Minimum range (attenuation) in dB
const MIN_RANGE_DB: f32 = -80.0;
/// Maximum range (attenuation) in dB
const MAX_RANGE_DB: f32 = 0.0;

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

/// Calculate envelope coefficient from time constant
#[inline]
fn time_to_coeff(time_ms: f32, sample_rate: f32) -> f32 {
    (-1.0 / (time_ms * sample_rate / 1000.0)).exp()
}

// ============================================================================
// Gate Effect
// ============================================================================

/// Noise gate effect
///
/// Attenuates audio when the input level falls below a threshold.
/// Useful for removing background noise, bleed, and unwanted low-level sounds.
///
/// # Parameters
/// - `threshold_db`: Level below which gating occurs (-80 to 0 dB)
/// - `attack_ms`: Time for gate to fully open (0.1 to 50 ms)
/// - `release_ms`: Time for gate to fully close (10 to 500 ms)
/// - `hold_ms`: Time to keep gate open after signal drops below threshold (0 to 100 ms)
/// - `range_db`: Amount of attenuation when gate is closed (-80 = full gate, 0 = no gate)
///
/// # Example
/// ```ignore
/// use nueva::dsp::Gate;
/// use nueva::engine::AudioBuffer;
/// use nueva::dsp::effect::Effect;
///
/// let mut gate = Gate::new(-40.0); // -40 dB threshold
/// gate.set_attack_ms(1.0);
/// gate.set_release_ms(50.0);
/// gate.set_range_db(-60.0); // Attenuate by 60 dB when closed
/// // process audio...
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gate {
    params: EffectParams,
    threshold_db: f32,
    attack_ms: f32,
    release_ms: f32,
    hold_ms: f32,
    range_db: f32,
    #[serde(skip)]
    envelope: f32,
    #[serde(skip)]
    hold_counter: usize,
    #[serde(skip)]
    sample_rate: f32,
    #[serde(skip)]
    threshold_linear: f32,
    #[serde(skip)]
    attack_coeff: f32,
    #[serde(skip)]
    release_coeff: f32,
    #[serde(skip)]
    hold_samples: usize,
    #[serde(skip)]
    range_linear: f32,
}

impl Gate {
    /// Create a new gate with specified threshold
    ///
    /// # Arguments
    /// * `threshold_db` - Threshold in dB (-80 to 0 dB)
    ///
    /// # Returns
    /// A new Gate with default attack/release/hold/range settings
    pub fn new(threshold_db: f32) -> Self {
        let clamped_threshold = threshold_db.clamp(MIN_THRESHOLD_DB, MAX_THRESHOLD_DB);
        let mut gate = Self {
            params: EffectParams::default(),
            threshold_db: clamped_threshold,
            attack_ms: 1.0,
            release_ms: 50.0,
            hold_ms: 10.0,
            range_db: -80.0,
            envelope: 0.0,
            hold_counter: 0,
            sample_rate: DEFAULT_SAMPLE_RATE,
            threshold_linear: db_to_linear(clamped_threshold),
            attack_coeff: 0.0,
            release_coeff: 0.0,
            hold_samples: 0,
            range_linear: db_to_linear(-80.0),
        };
        gate.update_coefficients();
        gate
    }

    /// Set threshold in dB
    pub fn set_threshold_db(&mut self, db: f32) {
        self.threshold_db = db.clamp(MIN_THRESHOLD_DB, MAX_THRESHOLD_DB);
        self.threshold_linear = db_to_linear(self.threshold_db);
    }

    /// Get threshold in dB
    pub fn threshold_db(&self) -> f32 {
        self.threshold_db
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

    /// Set hold time in milliseconds
    pub fn set_hold_ms(&mut self, ms: f32) {
        self.hold_ms = ms.clamp(MIN_HOLD_MS, MAX_HOLD_MS);
        self.hold_samples = (self.hold_ms * self.sample_rate / 1000.0) as usize;
    }

    /// Get hold time in milliseconds
    pub fn hold_ms(&self) -> f32 {
        self.hold_ms
    }

    /// Set range (attenuation) in dB
    ///
    /// -80 dB = full gate (complete silence when closed)
    /// 0 dB = no attenuation (gate has no effect)
    pub fn set_range_db(&mut self, db: f32) {
        self.range_db = db.clamp(MIN_RANGE_DB, MAX_RANGE_DB);
        self.range_linear = db_to_linear(self.range_db);
    }

    /// Get range (attenuation) in dB
    pub fn range_db(&self) -> f32 {
        self.range_db
    }

    /// Update internal coefficients
    fn update_coefficients(&mut self) {
        self.attack_coeff = time_to_coeff(self.attack_ms, self.sample_rate);
        self.release_coeff = time_to_coeff(self.release_ms, self.sample_rate);
        self.hold_samples = (self.hold_ms * self.sample_rate / 1000.0) as usize;
        self.threshold_linear = db_to_linear(self.threshold_db);
        self.range_linear = db_to_linear(self.range_db);
    }
}

impl Default for Gate {
    fn default() -> Self {
        Self::new(-40.0)
    }
}

impl Effect for Gate {
    impl_effect_common!(Gate, "gate", "Gate");

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

            // Determine if gate should be open or closed
            let gate_open = if peak >= self.threshold_linear {
                // Signal above threshold - open gate and reset hold counter
                self.hold_counter = self.hold_samples;
                true
            } else if self.hold_counter > 0 {
                // Signal below threshold but in hold phase
                self.hold_counter -= 1;
                true
            } else {
                // Signal below threshold and hold expired - close gate
                false
            };

            // Target envelope: 1.0 = fully open, range_linear = fully closed
            let target_envelope = if gate_open { 1.0 } else { self.range_linear };

            // Smooth envelope with attack/release
            if target_envelope > self.envelope {
                // Opening gate (attack)
                self.envelope =
                    self.attack_coeff * self.envelope + (1.0 - self.attack_coeff) * target_envelope;
            } else {
                // Closing gate (release)
                self.envelope = self.release_coeff * self.envelope
                    + (1.0 - self.release_coeff) * target_envelope;
            }

            // Apply envelope to all channels
            for ch in 0..num_channels {
                let samples = buffer.channel_mut(ch);
                samples[i] *= self.envelope;
            }
        }
    }

    fn prepare(&mut self, sample_rate: u32, _max_block_size: usize) {
        self.sample_rate = sample_rate as f32;
        self.update_coefficients();
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
        self.hold_counter = 0;
    }

    fn to_json(&self) -> Result<Value> {
        serde_json::to_value(self).map_err(|e| NuevaError::Serialization(e))
    }

    fn from_json(&mut self, json: &Value) -> Result<()> {
        if let Some(v) = json.get("threshold_db").and_then(|v| v.as_f64()) {
            self.set_threshold_db(v as f32);
        }
        if let Some(v) = json.get("attack_ms").and_then(|v| v.as_f64()) {
            self.set_attack_ms(v as f32);
        }
        if let Some(v) = json.get("release_ms").and_then(|v| v.as_f64()) {
            self.set_release_ms(v as f32);
        }
        if let Some(v) = json.get("hold_ms").and_then(|v| v.as_f64()) {
            self.set_hold_ms(v as f32);
        }
        if let Some(v) = json.get("range_db").and_then(|v| v.as_f64()) {
            self.set_range_db(v as f32);
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
            "attack_ms": self.attack_ms,
            "release_ms": self.release_ms,
            "hold_ms": self.hold_ms,
            "range_db": self.range_db,
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
            "hold_ms" | "hold" => {
                if let Some(v) = value.as_f64() {
                    self.set_hold_ms(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!(
                            "Invalid value for hold_ms: expected number, got {:?}",
                            value
                        ),
                    })
                }
            }
            "range_db" | "range" => {
                if let Some(v) = value.as_f64() {
                    self.set_range_db(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!(
                            "Invalid value for range_db: expected number, got {:?}",
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
    fn test_gate_new() {
        let gate = Gate::new(-40.0);
        assert!((gate.threshold_db() - (-40.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_gate_default() {
        let gate = Gate::default();
        assert!((gate.threshold_db() - (-40.0)).abs() < f32::EPSILON);
        assert!((gate.attack_ms() - 1.0).abs() < f32::EPSILON);
        assert!((gate.release_ms() - 50.0).abs() < f32::EPSILON);
        assert!((gate.hold_ms() - 10.0).abs() < f32::EPSILON);
        assert!((gate.range_db() - (-80.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_gate_clamping() {
        // Threshold clamping
        let gate = Gate::new(-100.0);
        assert!((gate.threshold_db() - MIN_THRESHOLD_DB).abs() < f32::EPSILON);

        let gate = Gate::new(10.0);
        assert!((gate.threshold_db() - MAX_THRESHOLD_DB).abs() < f32::EPSILON);
    }

    #[test]
    fn test_gate_setters() {
        let mut gate = Gate::new(-40.0);

        gate.set_threshold_db(-50.0);
        assert!((gate.threshold_db() - (-50.0)).abs() < f32::EPSILON);

        gate.set_attack_ms(5.0);
        assert!((gate.attack_ms() - 5.0).abs() < f32::EPSILON);

        gate.set_release_ms(100.0);
        assert!((gate.release_ms() - 100.0).abs() < f32::EPSILON);

        gate.set_hold_ms(20.0);
        assert!((gate.hold_ms() - 20.0).abs() < f32::EPSILON);

        gate.set_range_db(-60.0);
        assert!((gate.range_db() - (-60.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_gate_setter_clamping() {
        let mut gate = Gate::new(-40.0);

        gate.set_attack_ms(0.01);
        assert!((gate.attack_ms() - MIN_ATTACK_MS).abs() < f32::EPSILON);

        gate.set_attack_ms(100.0);
        assert!((gate.attack_ms() - MAX_ATTACK_MS).abs() < f32::EPSILON);

        gate.set_release_ms(5.0);
        assert!((gate.release_ms() - MIN_RELEASE_MS).abs() < f32::EPSILON);

        gate.set_release_ms(1000.0);
        assert!((gate.release_ms() - MAX_RELEASE_MS).abs() < f32::EPSILON);

        gate.set_hold_ms(-10.0);
        assert!((gate.hold_ms() - MIN_HOLD_MS).abs() < f32::EPSILON);

        gate.set_hold_ms(200.0);
        assert!((gate.hold_ms() - MAX_HOLD_MS).abs() < f32::EPSILON);

        gate.set_range_db(-100.0);
        assert!((gate.range_db() - MIN_RANGE_DB).abs() < f32::EPSILON);

        gate.set_range_db(10.0);
        assert!((gate.range_db() - MAX_RANGE_DB).abs() < f32::EPSILON);
    }

    #[test]
    fn test_gate_process_above_threshold() {
        let mut gate = Gate::new(-20.0);
        gate.set_range_db(-80.0);
        gate.prepare(48000, 512);

        // Signal at -6 dB (well above threshold of -20 dB)
        let level = db_to_linear(-6.0);
        let mut buffer = create_test_buffer(level, 10000);

        gate.process(&mut buffer);

        // Signal above threshold should pass through with envelope close to 1.0
        // Check sample near the end where envelope has settled
        let processed_sample = buffer.get_sample(0, 9000).unwrap();
        // Should be very close to original since gate is open
        assert!(
            (processed_sample - level).abs() < 0.1,
            "Expected ~{}, got {}",
            level,
            processed_sample
        );
    }

    #[test]
    fn test_gate_process_below_threshold() {
        let mut gate = Gate::new(-20.0);
        gate.set_range_db(-80.0);
        gate.set_attack_ms(0.1);
        gate.set_release_ms(10.0);
        gate.set_hold_ms(0.0);
        gate.prepare(48000, 512);

        // Signal at -40 dB (below threshold of -20 dB)
        let level = db_to_linear(-40.0);
        let mut buffer = create_test_buffer(level, 10000);

        gate.process(&mut buffer);

        // Signal below threshold should be attenuated by range
        let processed_sample = buffer.get_sample(0, 9000).unwrap();
        // With -80 dB range, output should be very small
        assert!(
            processed_sample.abs() < 0.01,
            "Expected near silence, got {}",
            processed_sample
        );
    }

    #[test]
    fn test_gate_hold_time() {
        let mut gate = Gate::new(-20.0);
        gate.set_hold_ms(50.0); // 50ms hold
        gate.set_attack_ms(0.1);
        gate.set_release_ms(10.0);
        gate.set_range_db(-80.0);
        gate.prepare(48000, 512);

        // Create buffer: loud signal then quiet signal
        let loud_level = db_to_linear(-6.0);
        let quiet_level = db_to_linear(-40.0);
        let num_samples = 10000;
        let mut buffer = AudioBuffer::new(num_samples, ChannelLayout::Stereo);

        // First half: loud (opens gate)
        for i in 0..num_samples / 2 {
            buffer.set_sample(0, i, loud_level);
            buffer.set_sample(1, i, loud_level);
        }
        // Second half: quiet (gate should stay open during hold, then close)
        for i in num_samples / 2..num_samples {
            buffer.set_sample(0, i, quiet_level);
            buffer.set_sample(1, i, quiet_level);
        }

        gate.process(&mut buffer);

        // Shortly after transition (within hold time), gate should still be mostly open
        // 50ms at 48kHz = 2400 samples, so at sample 5000 + 1000 = 6000, still in hold
        let sample_in_hold = buffer.get_sample(0, 6000).unwrap();
        // Should still have significant signal due to hold
        assert!(
            sample_in_hold.abs() > quiet_level * 0.1,
            "Hold time not working: expected gate open, got {}",
            sample_in_hold
        );

        // Well after hold + release, gate should be closed
        let sample_after_hold = buffer.get_sample(0, 9500).unwrap();
        assert!(
            sample_after_hold.abs() < 0.01,
            "Gate should be closed, got {}",
            sample_after_hold
        );
    }

    #[test]
    fn test_gate_range() {
        let mut gate = Gate::new(-20.0);
        gate.set_range_db(-20.0); // Only 20 dB attenuation when closed
        gate.set_hold_ms(0.0);
        gate.set_release_ms(10.0);
        gate.prepare(48000, 512);

        // Signal well below threshold
        let level = db_to_linear(-40.0);
        let mut buffer = create_test_buffer(level, 10000);

        gate.process(&mut buffer);

        // With -20 dB range, signal should be attenuated to about level * 0.1
        let processed_sample = buffer.get_sample(0, 9000).unwrap();
        let expected = level * db_to_linear(-20.0);
        // Allow for envelope smoothing
        assert!(
            (processed_sample.abs() - expected).abs() < expected + 0.001,
            "Expected ~{}, got {}",
            expected,
            processed_sample
        );
    }

    #[test]
    fn test_gate_process_disabled() {
        let mut gate = Gate::new(-40.0);
        gate.set_enabled(false);
        gate.prepare(48000, 512);

        let level = db_to_linear(-60.0);
        let mut buffer = create_test_buffer(level, 100);

        gate.process(&mut buffer);

        // Disabled effect should not modify buffer
        let sample = buffer.get_sample(0, 50).unwrap();
        assert!((sample - level).abs() < f32::EPSILON);
    }

    #[test]
    fn test_gate_effect_type() {
        let gate = Gate::new(-40.0);
        assert_eq!(gate.effect_type(), "gate");
        assert_eq!(gate.display_name(), "Gate");
    }

    #[test]
    fn test_gate_get_params() {
        let mut gate = Gate::new(-50.0);
        gate.set_attack_ms(2.0);
        gate.set_release_ms(100.0);
        gate.set_hold_ms(25.0);
        gate.set_range_db(-60.0);

        let params = gate.get_params();

        assert!((params["threshold_db"].as_f64().unwrap() - (-50.0)).abs() < 0.001);
        assert!((params["attack_ms"].as_f64().unwrap() - 2.0).abs() < 0.001);
        assert!((params["release_ms"].as_f64().unwrap() - 100.0).abs() < 0.001);
        assert!((params["hold_ms"].as_f64().unwrap() - 25.0).abs() < 0.001);
        assert!((params["range_db"].as_f64().unwrap() - (-60.0)).abs() < 0.001);
        assert!(params["enabled"].as_bool().unwrap());
    }

    #[test]
    fn test_gate_set_param() {
        let mut gate = Gate::new(-40.0);

        gate.set_param("threshold_db", &json!(-50.0)).unwrap();
        assert!((gate.threshold_db() - (-50.0)).abs() < f32::EPSILON);

        gate.set_param("attack_ms", &json!(5.0)).unwrap();
        assert!((gate.attack_ms() - 5.0).abs() < f32::EPSILON);

        gate.set_param("release_ms", &json!(100.0)).unwrap();
        assert!((gate.release_ms() - 100.0).abs() < f32::EPSILON);

        gate.set_param("hold_ms", &json!(30.0)).unwrap();
        assert!((gate.hold_ms() - 30.0).abs() < f32::EPSILON);

        gate.set_param("range_db", &json!(-60.0)).unwrap();
        assert!((gate.range_db() - (-60.0)).abs() < f32::EPSILON);

        gate.set_param("enabled", &json!(false)).unwrap();
        assert!(!gate.is_enabled());
    }

    #[test]
    fn test_gate_set_param_invalid() {
        let mut gate = Gate::new(-40.0);

        let result = gate.set_param("threshold_db", &json!("not a number"));
        assert!(result.is_err());

        let result = gate.set_param("unknown", &json!(1.0));
        assert!(result.is_err());
    }

    #[test]
    fn test_gate_to_from_json() {
        let mut original = Gate::new(-50.0);
        original.set_attack_ms(3.0);
        original.set_release_ms(80.0);
        original.set_hold_ms(15.0);
        original.set_range_db(-60.0);

        let json = original.to_json().unwrap();

        let mut restored = Gate::new(-40.0);
        restored.from_json(&json).unwrap();

        assert!((restored.threshold_db() - (-50.0)).abs() < f32::EPSILON);
        assert!((restored.attack_ms() - 3.0).abs() < f32::EPSILON);
        assert!((restored.release_ms() - 80.0).abs() < f32::EPSILON);
        assert!((restored.hold_ms() - 15.0).abs() < f32::EPSILON);
        assert!((restored.range_db() - (-60.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_gate_reset() {
        let mut gate = Gate::new(-40.0);
        gate.prepare(48000, 512);

        // Process some audio to build up state
        let level = db_to_linear(-6.0);
        let mut buffer = create_test_buffer(level, 1000);
        gate.process(&mut buffer);

        // Reset should clear state
        gate.reset();
        assert!((gate.envelope - 0.0).abs() < f32::EPSILON);
        assert_eq!(gate.hold_counter, 0);
    }

    #[test]
    fn test_gate_box_clone() {
        let gate = Gate::new(-40.0);
        let boxed: Box<dyn Effect> = Box::new(gate);
        let cloned = boxed.box_clone();

        assert_eq!(cloned.effect_type(), "gate");
    }

    #[test]
    fn test_gate_id() {
        let mut gate = Gate::new(-40.0);
        let original_id = gate.id().to_string();

        assert!(!original_id.is_empty());

        gate.set_id("custom-gate-id".to_string());
        assert_eq!(gate.id(), "custom-gate-id");
    }

    #[test]
    fn test_gate_prepare() {
        let mut gate = Gate::new(-40.0);
        gate.set_hold_ms(10.0);

        gate.prepare(96000, 1024);

        assert!((gate.sample_rate - 96000.0).abs() < f32::EPSILON);
        // Hold samples should be updated: 10ms * 96000 / 1000 = 960
        assert_eq!(gate.hold_samples, 960);
    }

    #[test]
    fn test_gate_preserves_stereo_relationship() {
        let mut gate = Gate::new(-20.0);
        gate.set_range_db(-80.0);
        gate.prepare(48000, 512);

        // Create stereo buffer with different levels, both above threshold
        let mut buffer = AudioBuffer::new(1000, ChannelLayout::Stereo);
        for i in 0..1000 {
            buffer.set_sample(0, i, 0.8); // Left channel
            buffer.set_sample(1, i, 0.4); // Right channel (half of left)
        }

        gate.process(&mut buffer);

        // After processing, check that stereo relationship is maintained
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
