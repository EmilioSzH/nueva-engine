//! Noise Gate effect (spec §4.2.7)
//!
//! A gate attenuates audio below a threshold, useful for removing noise
//! during silent passages. Features envelope follower with hysteresis
//! to prevent chattering.

use super::effect::{Effect, EffectMetadata};
use super::AudioBuffer;
use crate::error::{NuevaError, Result};
use serde::{Deserialize, Serialize};

/// Gate state for the envelope follower
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateState {
    /// Gate is closed (attenuating)
    Closed,
    /// Gate is in attack phase (opening)
    Attack,
    /// Gate is open (passing signal)
    Open,
    /// Gate is in hold phase (waiting before release)
    Hold,
    /// Gate is in release phase (closing)
    Release,
}

/// Gate parameters (spec §4.2.7)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateParams {
    /// Threshold in dB (-80 to 0)
    pub threshold_db: f32,
    /// Attack time in ms (0.1 to 50)
    pub attack_ms: f32,
    /// Release time in ms (10 to 500)
    pub release_ms: f32,
    /// Hold time in ms (0 to 100)
    pub hold_ms: f32,
    /// Range/attenuation in dB (-80 = full gate, 0 = no effect)
    pub range_db: f32,
}

impl Default for GateParams {
    fn default() -> Self {
        Self {
            threshold_db: -40.0,
            attack_ms: 1.0,
            release_ms: 50.0,
            hold_ms: 10.0,
            range_db: -80.0,
        }
    }
}

impl GateParams {
    /// Validate parameters are within spec ranges
    pub fn validate(&self) -> Result<()> {
        if !(-80.0..=0.0).contains(&self.threshold_db) {
            return Err(NuevaError::InvalidParameter {
                param: "threshold_db".to_string(),
                value: self.threshold_db.to_string(),
                expected: "-80 to 0 dB".to_string(),
            });
        }
        if !(0.1..=50.0).contains(&self.attack_ms) {
            return Err(NuevaError::InvalidParameter {
                param: "attack_ms".to_string(),
                value: self.attack_ms.to_string(),
                expected: "0.1 to 50 ms".to_string(),
            });
        }
        if !(10.0..=500.0).contains(&self.release_ms) {
            return Err(NuevaError::InvalidParameter {
                param: "release_ms".to_string(),
                value: self.release_ms.to_string(),
                expected: "10 to 500 ms".to_string(),
            });
        }
        if !(0.0..=100.0).contains(&self.hold_ms) {
            return Err(NuevaError::InvalidParameter {
                param: "hold_ms".to_string(),
                value: self.hold_ms.to_string(),
                expected: "0 to 100 ms".to_string(),
            });
        }
        if !(-80.0..=0.0).contains(&self.range_db) {
            return Err(NuevaError::InvalidParameter {
                param: "range_db".to_string(),
                value: self.range_db.to_string(),
                expected: "-80 to 0 dB".to_string(),
            });
        }
        Ok(())
    }

    /// Clamp parameters to valid ranges
    pub fn clamp(&mut self) {
        self.threshold_db = self.threshold_db.clamp(-80.0, 0.0);
        self.attack_ms = self.attack_ms.clamp(0.1, 50.0);
        self.release_ms = self.release_ms.clamp(10.0, 500.0);
        self.hold_ms = self.hold_ms.clamp(0.0, 100.0);
        self.range_db = self.range_db.clamp(-80.0, 0.0);
    }
}

/// Noise Gate effect (spec §4.2.7)
///
/// Implementation features:
/// - Envelope follower for level detection
/// - Hysteresis to prevent chattering (2 dB default)
/// - Hold timer to prevent rapid on/off switching
/// - Smooth attack/release to avoid clicks
#[derive(Debug, Clone)]
pub struct Gate {
    /// Effect parameters
    params: GateParams,
    /// Unique instance ID
    id: String,
    /// Whether the effect is enabled
    enabled: bool,
    /// Current sample rate
    sample_rate: f64,
    /// Current gate state
    state: GateState,
    /// Envelope follower value (linear)
    envelope: f32,
    /// Current gain (linear, 0 to 1)
    current_gain: f32,
    /// Hold counter in samples
    hold_counter: usize,
    /// Hysteresis in dB (prevents chattering)
    hysteresis_db: f32,
    /// Attack coefficient for envelope smoothing
    attack_coeff: f32,
    /// Release coefficient for envelope smoothing
    release_coeff: f32,
    /// Gate attack coefficient (for gain smoothing)
    gate_attack_coeff: f32,
    /// Gate release coefficient (for gain smoothing)
    gate_release_coeff: f32,
    /// Hold time in samples
    hold_samples: usize,
    /// Range as linear multiplier
    range_linear: f32,
    /// Threshold as linear value
    threshold_linear: f32,
    /// Hysteresis threshold (lower) as linear value
    threshold_low_linear: f32,
}

impl Gate {
    /// Create a new Gate with default parameters
    pub fn new() -> Self {
        Self::with_params(GateParams::default())
    }

    /// Create a new Gate with specified parameters
    pub fn with_params(params: GateParams) -> Self {
        let mut gate = Self {
            params,
            id: String::new(),
            enabled: true,
            sample_rate: 44100.0,
            state: GateState::Closed,
            envelope: 0.0,
            current_gain: 0.0,
            hold_counter: 0,
            hysteresis_db: 2.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            gate_attack_coeff: 0.0,
            gate_release_coeff: 0.0,
            hold_samples: 0,
            range_linear: 0.0,
            threshold_linear: 0.0,
            threshold_low_linear: 0.0,
        };
        gate.update_coefficients();
        gate
    }

    /// Get current parameters
    pub fn params(&self) -> &GateParams {
        &self.params
    }

    /// Set parameters (validates and updates coefficients)
    pub fn set_params(&mut self, params: GateParams) -> Result<()> {
        params.validate()?;
        self.params = params;
        self.update_coefficients();
        Ok(())
    }

    /// Set threshold in dB
    pub fn set_threshold_db(&mut self, threshold_db: f32) -> Result<()> {
        if !(-80.0..=0.0).contains(&threshold_db) {
            return Err(NuevaError::InvalidParameter {
                param: "threshold_db".to_string(),
                value: threshold_db.to_string(),
                expected: "-80 to 0 dB".to_string(),
            });
        }
        self.params.threshold_db = threshold_db;
        self.update_coefficients();
        Ok(())
    }

    /// Set attack time in ms
    pub fn set_attack_ms(&mut self, attack_ms: f32) -> Result<()> {
        if !(0.1..=50.0).contains(&attack_ms) {
            return Err(NuevaError::InvalidParameter {
                param: "attack_ms".to_string(),
                value: attack_ms.to_string(),
                expected: "0.1 to 50 ms".to_string(),
            });
        }
        self.params.attack_ms = attack_ms;
        self.update_coefficients();
        Ok(())
    }

    /// Set release time in ms
    pub fn set_release_ms(&mut self, release_ms: f32) -> Result<()> {
        if !(10.0..=500.0).contains(&release_ms) {
            return Err(NuevaError::InvalidParameter {
                param: "release_ms".to_string(),
                value: release_ms.to_string(),
                expected: "10 to 500 ms".to_string(),
            });
        }
        self.params.release_ms = release_ms;
        self.update_coefficients();
        Ok(())
    }

    /// Set hold time in ms
    pub fn set_hold_ms(&mut self, hold_ms: f32) -> Result<()> {
        if !(0.0..=100.0).contains(&hold_ms) {
            return Err(NuevaError::InvalidParameter {
                param: "hold_ms".to_string(),
                value: hold_ms.to_string(),
                expected: "0 to 100 ms".to_string(),
            });
        }
        self.params.hold_ms = hold_ms;
        self.update_coefficients();
        Ok(())
    }

    /// Set range/attenuation in dB
    pub fn set_range_db(&mut self, range_db: f32) -> Result<()> {
        if !(-80.0..=0.0).contains(&range_db) {
            return Err(NuevaError::InvalidParameter {
                param: "range_db".to_string(),
                value: range_db.to_string(),
                expected: "-80 to 0 dB".to_string(),
            });
        }
        self.params.range_db = range_db;
        self.update_coefficients();
        Ok(())
    }

    /// Set hysteresis in dB (default is 2 dB)
    pub fn set_hysteresis_db(&mut self, hysteresis_db: f32) {
        self.hysteresis_db = hysteresis_db.max(0.0);
        self.update_coefficients();
    }

    /// Update internal coefficients after parameter changes
    fn update_coefficients(&mut self) {
        // Convert threshold to linear
        self.threshold_linear = db_to_linear(self.params.threshold_db);

        // Calculate hysteresis threshold (lower threshold for closing)
        self.threshold_low_linear = db_to_linear(self.params.threshold_db - self.hysteresis_db);

        // Convert range to linear
        self.range_linear = db_to_linear(self.params.range_db);

        // Calculate envelope follower coefficients
        // Using a simple one-pole filter: coeff = exp(-1 / (time_constant * sample_rate))
        // For attack, we want fast response (level follower)
        let envelope_attack_ms = 0.1; // Fast attack for envelope detection
        let envelope_release_ms = 50.0; // Slower release for envelope detection

        self.attack_coeff = calculate_coefficient(envelope_attack_ms, self.sample_rate);
        self.release_coeff = calculate_coefficient(envelope_release_ms, self.sample_rate);

        // Calculate gate smoothing coefficients (for the actual gain)
        self.gate_attack_coeff = calculate_coefficient(self.params.attack_ms, self.sample_rate);
        self.gate_release_coeff = calculate_coefficient(self.params.release_ms, self.sample_rate);

        // Convert hold time to samples
        self.hold_samples = (self.params.hold_ms * self.sample_rate as f32 / 1000.0) as usize;
    }

    /// Process a single sample and return the gain to apply
    fn process_sample(&mut self, input_level: f32) -> f32 {
        // Update envelope follower (peak detection)
        if input_level > self.envelope {
            // Attack - fast response to increasing level
            self.envelope =
                self.attack_coeff * self.envelope + (1.0 - self.attack_coeff) * input_level;
        } else {
            // Release - slower response to decreasing level
            self.envelope =
                self.release_coeff * self.envelope + (1.0 - self.release_coeff) * input_level;
        }

        // State machine for gate
        let target_gain = match self.state {
            GateState::Closed => {
                // Check if we should open
                if self.envelope > self.threshold_linear {
                    self.state = GateState::Attack;
                }
                self.range_linear
            }
            GateState::Attack => {
                // Smoothly open the gate
                if self.current_gain >= 0.99 {
                    self.state = GateState::Open;
                }
                1.0
            }
            GateState::Open => {
                // Check if we should start closing (use hysteresis threshold)
                if self.envelope < self.threshold_low_linear {
                    self.state = GateState::Hold;
                    self.hold_counter = self.hold_samples;
                }
                1.0
            }
            GateState::Hold => {
                // Wait for hold time before releasing
                if self.envelope > self.threshold_linear {
                    // Signal came back up, stay open
                    self.state = GateState::Open;
                } else if self.hold_counter > 0 {
                    self.hold_counter -= 1;
                } else {
                    self.state = GateState::Release;
                }
                1.0
            }
            GateState::Release => {
                // Smoothly close the gate
                if self.envelope > self.threshold_linear {
                    // Signal came back up
                    self.state = GateState::Attack;
                    1.0
                } else if self.current_gain <= self.range_linear + 0.001 {
                    self.state = GateState::Closed;
                    self.range_linear
                } else {
                    self.range_linear
                }
            }
        };

        // Smooth the gain transition
        if target_gain > self.current_gain {
            // Opening - use attack coefficient
            self.current_gain = self.gate_attack_coeff * self.current_gain
                + (1.0 - self.gate_attack_coeff) * target_gain;
        } else {
            // Closing - use release coefficient
            self.current_gain = self.gate_release_coeff * self.current_gain
                + (1.0 - self.gate_release_coeff) * target_gain;
        }

        self.current_gain
    }
}

impl Default for Gate {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Gate {
    fn process(&mut self, buffer: &mut AudioBuffer) {
        let num_channels = buffer.num_channels();
        let num_samples = buffer.num_samples();

        for frame in 0..num_samples {
            // Calculate peak level across all channels for this frame
            let mut peak: f32 = 0.0;
            for channel in 0..num_channels {
                if let Some(sample) = buffer.get(frame, channel) {
                    peak = peak.max(sample.abs());
                }
            }

            // Get gain for this sample
            let gain = self.process_sample(peak);

            // Apply gain to all channels
            for channel in 0..num_channels {
                if let Some(sample) = buffer.get(frame, channel) {
                    buffer.set(frame, channel, sample * gain);
                }
            }
        }
    }

    fn prepare(&mut self, sample_rate: f64, _samples_per_block: usize) {
        self.sample_rate = sample_rate;
        self.update_coefficients();
    }

    fn reset(&mut self) {
        self.state = GateState::Closed;
        self.envelope = 0.0;
        self.current_gain = self.range_linear;
        self.hold_counter = 0;
    }

    fn to_json(&self) -> Result<serde_json::Value> {
        serde_json::to_value(&self.params).map_err(|e| NuevaError::SerializationError {
            details: e.to_string(),
        })
    }

    fn from_json(&mut self, json: &serde_json::Value) -> Result<()> {
        let params: GateParams =
            serde_json::from_value(json.clone()).map_err(|e| NuevaError::SerializationError {
                details: e.to_string(),
            })?;
        self.set_params(params)
    }

    fn effect_type(&self) -> &'static str {
        "gate"
    }

    fn display_name(&self) -> &'static str {
        "Gate"
    }

    fn metadata(&self) -> EffectMetadata {
        EffectMetadata {
            effect_type: "gate".to_string(),
            display_name: "Gate".to_string(),
            category: "dynamics".to_string(),
            order_priority: 0, // Gate should be first in chain (spec §4.3)
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

/// Convert decibels to linear amplitude
#[inline]
fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Convert linear amplitude to decibels
#[allow(dead_code)]
#[inline]
fn linear_to_db(linear: f32) -> f32 {
    if linear > 0.0 {
        20.0 * linear.log10()
    } else {
        -80.0 // Floor at -80 dB
    }
}

/// Calculate one-pole filter coefficient from time constant
#[inline]
fn calculate_coefficient(time_ms: f32, sample_rate: f64) -> f32 {
    if time_ms <= 0.0 {
        return 0.0;
    }
    let time_seconds = time_ms / 1000.0;
    (-1.0 / (time_seconds * sample_rate as f32)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gate_default_params() {
        let gate = Gate::new();
        assert_eq!(gate.params.threshold_db, -40.0);
        assert_eq!(gate.params.attack_ms, 1.0);
        assert_eq!(gate.params.release_ms, 50.0);
        assert_eq!(gate.params.hold_ms, 10.0);
        assert_eq!(gate.params.range_db, -80.0);
    }

    #[test]
    fn test_gate_param_validation() {
        let mut params = GateParams::default();
        assert!(params.validate().is_ok());

        params.threshold_db = -100.0; // Out of range
        assert!(params.validate().is_err());

        params.threshold_db = -40.0;
        params.attack_ms = 0.0; // Out of range
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_gate_param_clamping() {
        let mut params = GateParams {
            threshold_db: -100.0,
            attack_ms: 0.0,
            release_ms: 1000.0,
            hold_ms: -10.0,
            range_db: -100.0,
        };
        params.clamp();

        assert_eq!(params.threshold_db, -80.0);
        assert_eq!(params.attack_ms, 0.1);
        assert_eq!(params.release_ms, 500.0);
        assert_eq!(params.hold_ms, 0.0);
        assert_eq!(params.range_db, -80.0);
    }

    #[test]
    fn test_gate_silence_passes() {
        let mut gate = Gate::new();
        gate.prepare(44100.0, 512);

        // Create silent buffer
        let mut buffer = AudioBuffer::new(2, 1000, 44100.0);

        // Process
        gate.process(&mut buffer);

        // Output should still be silent (gate closed, but silence * attenuation = silence)
        for i in 0..buffer.num_samples() {
            for ch in 0..buffer.num_channels() {
                assert_eq!(buffer.get(i, ch).unwrap(), 0.0);
            }
        }
    }

    #[test]
    fn test_gate_loud_signal_passes() {
        let mut gate = Gate::new();
        gate.set_threshold_db(-40.0).unwrap();
        gate.prepare(44100.0, 512);

        // Create loud signal buffer (well above threshold)
        let mut buffer = AudioBuffer::new(1, 1000, 44100.0);
        for i in 0..1000 {
            buffer.set(i, 0, 0.5); // -6 dB, well above -40 dB threshold
        }

        // Process
        gate.process(&mut buffer);

        // After attack time, signal should pass through mostly unchanged
        // Check samples near the end where gate should be fully open
        let last_sample = buffer.get(999, 0).unwrap();
        assert!(
            last_sample > 0.45,
            "Gate should pass loud signals, got {}",
            last_sample
        );
    }

    #[test]
    fn test_gate_attenuates_quiet_signal() {
        let mut gate = Gate::new();
        gate.set_threshold_db(-20.0).unwrap(); // Set threshold high
        gate.set_range_db(-60.0).unwrap();
        gate.prepare(44100.0, 512);

        // Create quiet signal buffer (below threshold)
        let mut buffer = AudioBuffer::new(1, 1000, 44100.0);
        for i in 0..1000 {
            buffer.set(i, 0, 0.01); // About -40 dB
        }

        // Process
        gate.process(&mut buffer);

        // Signal should be attenuated
        let last_sample = buffer.get(999, 0).unwrap();
        // With -60 dB range, the attenuation factor is 0.001
        // So 0.01 * 0.001 = 0.00001
        assert!(
            last_sample < 0.001,
            "Gate should attenuate quiet signals, got {}",
            last_sample
        );
    }

    #[test]
    fn test_gate_hysteresis_prevents_chattering() {
        let mut gate = Gate::new();
        gate.set_threshold_db(-30.0).unwrap();
        gate.set_hysteresis_db(3.0);
        gate.prepare(44100.0, 512);

        // The gate should only close when signal drops 3 dB below threshold
        // This test verifies hysteresis is being applied
        assert!(gate.threshold_linear > gate.threshold_low_linear);

        // The ratio between thresholds should match hysteresis
        let ratio = gate.threshold_linear / gate.threshold_low_linear;
        let expected_ratio = db_to_linear(3.0);
        assert!((ratio - expected_ratio).abs() < 0.001);
    }

    #[test]
    fn test_gate_serialization() {
        let mut gate = Gate::new();
        gate.set_threshold_db(-35.0).unwrap();
        gate.set_attack_ms(5.0).unwrap();
        gate.set_release_ms(100.0).unwrap();

        // Serialize
        let json = gate.to_json().unwrap();

        // Create new gate and deserialize
        let mut gate2 = Gate::new();
        gate2.from_json(&json).unwrap();

        assert_eq!(gate2.params.threshold_db, -35.0);
        assert_eq!(gate2.params.attack_ms, 5.0);
        assert_eq!(gate2.params.release_ms, 100.0);
    }

    #[test]
    fn test_gate_effect_trait() {
        let gate = Gate::new();

        assert_eq!(gate.effect_type(), "gate");
        assert_eq!(gate.display_name(), "Gate");
        assert!(gate.is_enabled());

        let metadata = gate.metadata();
        assert_eq!(metadata.effect_type, "gate");
        assert_eq!(metadata.category, "dynamics");
        assert_eq!(metadata.order_priority, 0);
    }

    #[test]
    fn test_gate_enable_disable() {
        let mut gate = Gate::new();

        assert!(gate.is_enabled());
        gate.set_enabled(false);
        assert!(!gate.is_enabled());
        gate.set_enabled(true);
        assert!(gate.is_enabled());
    }

    #[test]
    fn test_gate_id() {
        let mut gate = Gate::new();

        assert_eq!(gate.id(), "");
        gate.set_id("gate-1".to_string());
        assert_eq!(gate.id(), "gate-1");
    }

    #[test]
    fn test_gate_reset() {
        let mut gate = Gate::new();
        gate.prepare(44100.0, 512);

        // Process some audio to change internal state
        let mut buffer = AudioBuffer::new(1, 100, 44100.0);
        for i in 0..100 {
            buffer.set(i, 0, 0.5);
        }
        gate.process(&mut buffer);

        // Reset
        gate.reset();

        // Internal state should be reset
        assert_eq!(gate.state, GateState::Closed);
        assert_eq!(gate.envelope, 0.0);
        assert_eq!(gate.hold_counter, 0);
    }

    #[test]
    fn test_gate_stereo_processing() {
        let mut gate = Gate::new();
        gate.set_threshold_db(-20.0).unwrap();
        gate.prepare(44100.0, 512);

        // Create stereo buffer with loud signal
        let mut buffer = AudioBuffer::new(2, 500, 44100.0);
        for i in 0..500 {
            buffer.set(i, 0, 0.3); // Left
            buffer.set(i, 1, -0.3); // Right
        }

        // Process
        gate.process(&mut buffer);

        // Both channels should be processed with the same gain
        // (gate uses peak across all channels)
        let left = buffer.get(499, 0).unwrap();
        let right = buffer.get(499, 1).unwrap();

        // Samples should have similar magnitude (opposite sign)
        assert!((left.abs() - right.abs()).abs() < 0.001);
    }

    #[test]
    fn test_db_to_linear_conversion() {
        assert!((db_to_linear(0.0) - 1.0).abs() < 0.001);
        assert!((db_to_linear(-6.0) - 0.5012).abs() < 0.01);
        assert!((db_to_linear(-20.0) - 0.1).abs() < 0.001);
        assert!((db_to_linear(-40.0) - 0.01).abs() < 0.001);
    }

    #[test]
    fn test_linear_to_db_conversion() {
        assert!((linear_to_db(1.0) - 0.0).abs() < 0.001);
        assert!((linear_to_db(0.5) - (-6.02)).abs() < 0.1);
        assert!((linear_to_db(0.1) - (-20.0)).abs() < 0.001);
        assert_eq!(linear_to_db(0.0), -80.0);
    }

    #[test]
    fn test_coefficient_calculation() {
        let sample_rate = 44100.0;

        // Longer time = higher coefficient (slower change)
        let fast_coeff = calculate_coefficient(1.0, sample_rate);
        let slow_coeff = calculate_coefficient(100.0, sample_rate);

        assert!(slow_coeff > fast_coeff);

        // Zero time should give zero coefficient
        assert_eq!(calculate_coefficient(0.0, sample_rate), 0.0);
    }

    #[test]
    fn test_gate_hold_timer() {
        let mut gate = Gate::new();
        gate.set_threshold_db(-30.0).unwrap();
        gate.set_hold_ms(50.0).unwrap(); // 50ms hold
        gate.prepare(44100.0, 512);

        // Hold samples should be calculated correctly
        let expected_hold_samples = (50.0 * 44100.0 / 1000.0) as usize;
        assert_eq!(gate.hold_samples, expected_hold_samples);
    }

    #[test]
    fn test_gate_parameter_setters_validation() {
        let mut gate = Gate::new();

        // Valid values should succeed
        assert!(gate.set_threshold_db(-30.0).is_ok());
        assert!(gate.set_attack_ms(10.0).is_ok());
        assert!(gate.set_release_ms(100.0).is_ok());
        assert!(gate.set_hold_ms(50.0).is_ok());
        assert!(gate.set_range_db(-60.0).is_ok());

        // Invalid values should fail
        assert!(gate.set_threshold_db(-100.0).is_err());
        assert!(gate.set_attack_ms(0.0).is_err());
        assert!(gate.set_release_ms(5.0).is_err());
        assert!(gate.set_hold_ms(-10.0).is_err());
        assert!(gate.set_range_db(10.0).is_err());
    }
}
