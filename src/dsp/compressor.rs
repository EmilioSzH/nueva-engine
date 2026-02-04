//! Compressor effect (spec section 4.2.3)
//!
//! A dynamics processor that reduces the dynamic range of audio signals.
//! Features envelope follower, gain computer with soft knee, attack/release
//! smoothing, and optional auto makeup gain.

use super::{AudioBuffer, Effect, EffectMetadata};
use crate::error::{NuevaError, Result};
use serde::{Deserialize, Serialize};

/// Compressor parameters with validation ranges from spec section 4.2.3
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressorParams {
    /// Threshold level in dB (-60 to 0 dB)
    pub threshold_db: f32,
    /// Compression ratio (1.0 to 20.0, representing 1:1 to 20:1)
    pub ratio: f32,
    /// Attack time in milliseconds (0.1 to 100 ms)
    pub attack_ms: f32,
    /// Release time in milliseconds (10 to 1000 ms)
    pub release_ms: f32,
    /// Knee width in dB (0 = hard knee, up to 12 dB for soft knee)
    pub knee_db: f32,
    /// Makeup gain in dB (0 to 24 dB)
    pub makeup_gain_db: f32,
    /// Enable automatic makeup gain calculation
    pub auto_makeup: bool,
}

impl Default for CompressorParams {
    fn default() -> Self {
        Self {
            threshold_db: -18.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 100.0,
            knee_db: 0.0,
            makeup_gain_db: 0.0,
            auto_makeup: false,
        }
    }
}

impl CompressorParams {
    /// Validate parameters against spec ranges
    pub fn validate(&self) -> Result<()> {
        if self.threshold_db < -60.0 || self.threshold_db > 0.0 {
            return Err(NuevaError::InvalidParameter {
                param: "threshold_db".to_string(),
                value: self.threshold_db.to_string(),
                expected: "-60 to 0 dB".to_string(),
            });
        }
        if self.ratio < 1.0 || self.ratio > 20.0 {
            return Err(NuevaError::InvalidParameter {
                param: "ratio".to_string(),
                value: self.ratio.to_string(),
                expected: "1.0 to 20.0".to_string(),
            });
        }
        if self.attack_ms < 0.1 || self.attack_ms > 100.0 {
            return Err(NuevaError::InvalidParameter {
                param: "attack_ms".to_string(),
                value: self.attack_ms.to_string(),
                expected: "0.1 to 100 ms".to_string(),
            });
        }
        if self.release_ms < 10.0 || self.release_ms > 1000.0 {
            return Err(NuevaError::InvalidParameter {
                param: "release_ms".to_string(),
                value: self.release_ms.to_string(),
                expected: "10 to 1000 ms".to_string(),
            });
        }
        if self.knee_db < 0.0 || self.knee_db > 12.0 {
            return Err(NuevaError::InvalidParameter {
                param: "knee_db".to_string(),
                value: self.knee_db.to_string(),
                expected: "0 to 12 dB".to_string(),
            });
        }
        if self.makeup_gain_db < 0.0 || self.makeup_gain_db > 24.0 {
            return Err(NuevaError::InvalidParameter {
                param: "makeup_gain_db".to_string(),
                value: self.makeup_gain_db.to_string(),
                expected: "0 to 24 dB".to_string(),
            });
        }
        Ok(())
    }

    /// Clamp parameters to valid ranges
    pub fn clamp(&mut self) {
        self.threshold_db = self.threshold_db.clamp(-60.0, 0.0);
        self.ratio = self.ratio.clamp(1.0, 20.0);
        self.attack_ms = self.attack_ms.clamp(0.1, 100.0);
        self.release_ms = self.release_ms.clamp(10.0, 1000.0);
        self.knee_db = self.knee_db.clamp(0.0, 12.0);
        self.makeup_gain_db = self.makeup_gain_db.clamp(0.0, 24.0);
    }
}

/// Compressor dynamics processor
///
/// Implements a feed-forward compressor with:
/// - Peak envelope detection
/// - Soft knee support
/// - Smooth attack/release
/// - Auto makeup gain option
#[derive(Debug, Clone)]
pub struct Compressor {
    /// Unique instance identifier
    id: String,
    /// Whether the effect is enabled
    enabled: bool,
    /// Compressor parameters
    params: CompressorParams,
    /// Sample rate in Hz
    sample_rate: f64,
    /// Samples per processing block
    samples_per_block: usize,
    /// Attack coefficient for envelope smoothing
    attack_coeff: f32,
    /// Release coefficient for envelope smoothing
    release_coeff: f32,
    /// Current envelope level per channel (linear)
    envelope: Vec<f32>,
    /// Current gain reduction per channel (linear)
    gain_reduction: Vec<f32>,
}

impl Compressor {
    /// Create a new compressor with default parameters
    pub fn new() -> Self {
        Self {
            id: String::new(),
            enabled: true,
            params: CompressorParams::default(),
            sample_rate: 44100.0,
            samples_per_block: 512,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            envelope: vec![0.0; 2],
            gain_reduction: vec![1.0; 2],
        }
    }

    /// Create a new compressor with custom parameters
    pub fn with_params(params: CompressorParams) -> Self {
        let mut comp = Self::new();
        comp.params = params;
        comp.params.clamp();
        comp
    }

    /// Get the current parameters
    pub fn params(&self) -> &CompressorParams {
        &self.params
    }

    /// Set the parameters (validates and clamps)
    pub fn set_params(&mut self, params: CompressorParams) {
        self.params = params;
        self.params.clamp();
        self.update_coefficients();
    }

    /// Set threshold in dB
    pub fn set_threshold_db(&mut self, threshold_db: f32) {
        self.params.threshold_db = threshold_db.clamp(-60.0, 0.0);
    }

    /// Set compression ratio (1:1 to 20:1)
    pub fn set_ratio(&mut self, ratio: f32) {
        self.params.ratio = ratio.clamp(1.0, 20.0);
    }

    /// Set attack time in milliseconds
    pub fn set_attack_ms(&mut self, attack_ms: f32) {
        self.params.attack_ms = attack_ms.clamp(0.1, 100.0);
        self.update_coefficients();
    }

    /// Set release time in milliseconds
    pub fn set_release_ms(&mut self, release_ms: f32) {
        self.params.release_ms = release_ms.clamp(10.0, 1000.0);
        self.update_coefficients();
    }

    /// Set knee width in dB (0 = hard knee)
    pub fn set_knee_db(&mut self, knee_db: f32) {
        self.params.knee_db = knee_db.clamp(0.0, 12.0);
    }

    /// Set makeup gain in dB
    pub fn set_makeup_gain_db(&mut self, makeup_gain_db: f32) {
        self.params.makeup_gain_db = makeup_gain_db.clamp(0.0, 24.0);
    }

    /// Enable or disable auto makeup gain
    pub fn set_auto_makeup(&mut self, auto_makeup: bool) {
        self.params.auto_makeup = auto_makeup;
    }

    /// Get the current gain reduction in dB for metering
    pub fn gain_reduction_db(&self) -> f32 {
        // Return the average gain reduction across channels
        if self.gain_reduction.is_empty() {
            return 0.0;
        }
        let avg_linear: f32 =
            self.gain_reduction.iter().sum::<f32>() / self.gain_reduction.len() as f32;
        if avg_linear > 0.0 {
            20.0 * avg_linear.log10()
        } else {
            -96.0
        }
    }

    /// Update attack/release coefficients based on sample rate and time constants
    fn update_coefficients(&mut self) {
        // Calculate one-pole filter coefficients
        // Time constant: time for signal to reach ~63% of target
        // coeff = exp(-1 / (time_in_samples))

        let attack_samples = (self.params.attack_ms / 1000.0) * self.sample_rate as f32;
        let release_samples = (self.params.release_ms / 1000.0) * self.sample_rate as f32;

        // Prevent division by zero
        self.attack_coeff = if attack_samples > 0.0 {
            (-1.0 / attack_samples).exp()
        } else {
            0.0
        };

        self.release_coeff = if release_samples > 0.0 {
            (-1.0 / release_samples).exp()
        } else {
            0.0
        };
    }

    /// Calculate auto makeup gain based on threshold and ratio
    fn calculate_auto_makeup(&self) -> f32 {
        // Estimate makeup gain based on how much compression occurs at threshold
        // A simple approximation: compensate for half the gain reduction at threshold
        // This gives a reasonable starting point that doesn't over-compensate

        if self.params.ratio <= 1.0 {
            return 0.0;
        }

        // Gain reduction at threshold = threshold * (1 - 1/ratio)
        // We use half of this as makeup (conservative approach)
        let gr_at_threshold = self.params.threshold_db.abs() * (1.0 - 1.0 / self.params.ratio);
        (gr_at_threshold * 0.5).min(24.0)
    }

    /// Compute gain reduction for a given input level in dB
    /// Returns the gain reduction in dB (negative value)
    fn compute_gain_reduction_db(&self, input_db: f32) -> f32 {
        let threshold = self.params.threshold_db;
        let ratio = self.params.ratio;
        let knee = self.params.knee_db;

        if knee > 0.0 {
            // Soft knee implementation
            // Knee range: threshold - knee/2 to threshold + knee/2
            let knee_start = threshold - knee / 2.0;
            let knee_end = threshold + knee / 2.0;

            if input_db <= knee_start {
                // Below knee: no compression
                0.0
            } else if input_db >= knee_end {
                // Above knee: full compression
                (threshold + (input_db - threshold) / ratio) - input_db
            } else {
                // In knee region: gradual transition
                // Quadratic interpolation for smooth knee
                let knee_factor = (input_db - knee_start) / knee;
                let knee_factor_sq = knee_factor * knee_factor;

                // Interpolate ratio from 1:1 at knee_start to full ratio at knee_end
                let effective_ratio = 1.0 + (ratio - 1.0) * knee_factor_sq;
                let over_threshold = input_db - knee_start;

                (knee_start + over_threshold / effective_ratio) - input_db
            }
        } else {
            // Hard knee
            if input_db <= threshold {
                0.0
            } else {
                // output = threshold + (input - threshold) / ratio
                // gain_reduction = output - input
                (threshold + (input_db - threshold) / ratio) - input_db
            }
        }
    }

    /// Convert linear amplitude to dB
    fn linear_to_db(linear: f32) -> f32 {
        if linear > 0.0 {
            20.0 * linear.log10()
        } else {
            -96.0 // Floor at -96 dB
        }
    }

    /// Convert dB to linear amplitude
    fn db_to_linear(db: f32) -> f32 {
        10.0_f32.powf(db / 20.0)
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Compressor {
    fn process(&mut self, buffer: &mut AudioBuffer) {
        let num_channels = buffer.num_channels();
        let num_samples = buffer.num_samples();

        // Ensure we have envelope state for each channel
        if self.envelope.len() < num_channels {
            self.envelope.resize(num_channels, 0.0);
            self.gain_reduction.resize(num_channels, 1.0);
        }

        // Calculate makeup gain
        let makeup_db = if self.params.auto_makeup {
            self.calculate_auto_makeup()
        } else {
            self.params.makeup_gain_db
        };
        let makeup_linear = Self::db_to_linear(makeup_db);

        // Process each sample
        for frame in 0..num_samples {
            // For stereo, use the max level across channels for linked detection
            let mut max_input_level: f32 = 0.0;
            for ch in 0..num_channels {
                if let Some(sample) = buffer.get(frame, ch) {
                    max_input_level = max_input_level.max(sample.abs());
                }
            }

            // Convert to dB for gain computation
            let input_db = Self::linear_to_db(max_input_level);

            // Compute desired gain reduction
            let target_gr_db = self.compute_gain_reduction_db(input_db);
            let target_gr_linear = Self::db_to_linear(target_gr_db);

            // Apply envelope smoothing (attack/release)
            // We use the first channel's envelope for linked stereo operation
            let current_gr = self.gain_reduction[0];

            let smoothed_gr = if target_gr_linear < current_gr {
                // Attacking (gain going down, GR increasing)
                self.attack_coeff * current_gr + (1.0 - self.attack_coeff) * target_gr_linear
            } else {
                // Releasing (gain going up, GR decreasing)
                self.release_coeff * current_gr + (1.0 - self.release_coeff) * target_gr_linear
            };

            // Store for metering
            for ch in 0..num_channels.min(self.gain_reduction.len()) {
                self.gain_reduction[ch] = smoothed_gr;
            }

            // Apply gain reduction and makeup to all channels
            let total_gain = smoothed_gr * makeup_linear;
            for ch in 0..num_channels {
                if let Some(sample) = buffer.get(frame, ch) {
                    buffer.set(frame, ch, sample * total_gain);
                }
            }
        }
    }

    fn prepare(&mut self, sample_rate: f64, samples_per_block: usize) {
        self.sample_rate = sample_rate;
        self.samples_per_block = samples_per_block;
        self.update_coefficients();
    }

    fn reset(&mut self) {
        // Reset envelope followers and gain reduction state
        for env in &mut self.envelope {
            *env = 0.0;
        }
        for gr in &mut self.gain_reduction {
            *gr = 1.0;
        }
    }

    fn to_json(&self) -> Result<serde_json::Value> {
        serde_json::to_value(&CompressorState {
            id: self.id.clone(),
            enabled: self.enabled,
            params: self.params.clone(),
        })
        .map_err(|e| NuevaError::SerializationError {
            details: e.to_string(),
        })
    }

    fn from_json(&mut self, json: &serde_json::Value) -> Result<()> {
        let state: CompressorState =
            serde_json::from_value(json.clone()).map_err(|e| NuevaError::SerializationError {
                details: e.to_string(),
            })?;

        self.id = state.id;
        self.enabled = state.enabled;
        self.params = state.params;
        self.params.clamp();
        self.update_coefficients();
        Ok(())
    }

    fn effect_type(&self) -> &'static str {
        "compressor"
    }

    fn display_name(&self) -> &'static str {
        "Compressor"
    }

    fn metadata(&self) -> EffectMetadata {
        EffectMetadata {
            effect_type: "compressor".to_string(),
            display_name: "Compressor".to_string(),
            category: "dynamics".to_string(),
            order_priority: 2, // After gate and corrective EQ
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

/// Serializable state for the compressor
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompressorState {
    id: String,
    enabled: bool,
    params: CompressorParams,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compressor_default_params() {
        let comp = Compressor::new();
        let params = comp.params();

        assert_eq!(params.threshold_db, -18.0);
        assert_eq!(params.ratio, 4.0);
        assert_eq!(params.attack_ms, 10.0);
        assert_eq!(params.release_ms, 100.0);
        assert_eq!(params.knee_db, 0.0);
        assert_eq!(params.makeup_gain_db, 0.0);
        assert!(!params.auto_makeup);
    }

    #[test]
    fn test_parameter_validation() {
        let mut params = CompressorParams::default();
        assert!(params.validate().is_ok());

        // Test invalid threshold
        params.threshold_db = -70.0;
        assert!(params.validate().is_err());
        params.threshold_db = -18.0;

        // Test invalid ratio
        params.ratio = 0.5;
        assert!(params.validate().is_err());
        params.ratio = 25.0;
        assert!(params.validate().is_err());
        params.ratio = 4.0;

        // Test invalid attack
        params.attack_ms = 0.01;
        assert!(params.validate().is_err());
        params.attack_ms = 10.0;

        // Test invalid release
        params.release_ms = 5.0;
        assert!(params.validate().is_err());
        params.release_ms = 100.0;

        // Test invalid knee
        params.knee_db = 15.0;
        assert!(params.validate().is_err());
        params.knee_db = 0.0;

        // Test invalid makeup
        params.makeup_gain_db = 30.0;
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_parameter_clamping() {
        let mut params = CompressorParams {
            threshold_db: -100.0,
            ratio: 50.0,
            attack_ms: 0.001,
            release_ms: 5000.0,
            knee_db: 20.0,
            makeup_gain_db: 50.0,
            auto_makeup: false,
        };

        params.clamp();

        assert_eq!(params.threshold_db, -60.0);
        assert_eq!(params.ratio, 20.0);
        assert_eq!(params.attack_ms, 0.1);
        assert_eq!(params.release_ms, 1000.0);
        assert_eq!(params.knee_db, 12.0);
        assert_eq!(params.makeup_gain_db, 24.0);
    }

    #[test]
    fn test_gain_computer_hard_knee() {
        let comp = Compressor::with_params(CompressorParams {
            threshold_db: -20.0,
            ratio: 4.0,
            knee_db: 0.0, // Hard knee
            ..Default::default()
        });

        // Below threshold: no gain reduction
        let gr = comp.compute_gain_reduction_db(-30.0);
        assert!(
            (gr - 0.0).abs() < 0.01,
            "Expected 0 dB GR below threshold, got {}",
            gr
        );

        // At threshold: no gain reduction
        let gr = comp.compute_gain_reduction_db(-20.0);
        assert!(
            (gr - 0.0).abs() < 0.01,
            "Expected 0 dB GR at threshold, got {}",
            gr
        );

        // 8 dB above threshold with 4:1 ratio
        // Output = -20 + 8/4 = -20 + 2 = -18 dB
        // GR = output - input = -18 - (-12) = -6 dB
        let gr = comp.compute_gain_reduction_db(-12.0);
        assert!((gr - (-6.0)).abs() < 0.01, "Expected -6 dB GR, got {}", gr);
    }

    #[test]
    fn test_gain_computer_soft_knee() {
        let comp = Compressor::with_params(CompressorParams {
            threshold_db: -20.0,
            ratio: 4.0,
            knee_db: 6.0, // 6 dB soft knee
            ..Default::default()
        });

        // Well below knee: no compression
        let gr = comp.compute_gain_reduction_db(-30.0);
        assert!(
            (gr - 0.0).abs() < 0.01,
            "Expected 0 dB GR below knee, got {}",
            gr
        );

        // At knee start (-23 dB): should be transitioning
        let gr = comp.compute_gain_reduction_db(-23.0);
        assert!(
            gr <= 0.0 && gr >= -1.0,
            "Expected small GR at knee start, got {}",
            gr
        );

        // Above knee end (-17 dB): full compression
        let gr = comp.compute_gain_reduction_db(-8.0);
        // 12 dB above threshold, ratio 4:1 -> output = -20 + 12/4 = -17
        // GR = -17 - (-8) = -9 dB
        assert!(
            (gr - (-9.0)).abs() < 0.5,
            "Expected ~-9 dB GR above knee, got {}",
            gr
        );
    }

    #[test]
    fn test_auto_makeup_gain() {
        let mut comp = Compressor::with_params(CompressorParams {
            threshold_db: -20.0,
            ratio: 4.0,
            auto_makeup: true,
            ..Default::default()
        });
        comp.prepare(44100.0, 512);

        let makeup = comp.calculate_auto_makeup();
        // With -20 dB threshold and 4:1 ratio
        // GR at threshold = 20 * (1 - 1/4) = 20 * 0.75 = 15 dB
        // Auto makeup = 15 * 0.5 = 7.5 dB
        assert!(makeup > 0.0, "Auto makeup should be positive");
        assert!(makeup < 15.0, "Auto makeup should be conservative");
    }

    #[test]
    fn test_process_below_threshold() {
        let mut comp = Compressor::with_params(CompressorParams {
            threshold_db: -10.0,
            ratio: 4.0,
            ..Default::default()
        });
        comp.prepare(44100.0, 512);

        // Create a buffer with -20 dB signal (well below -10 dB threshold)
        let mut buffer = AudioBuffer::new(2, 1000, 44100.0);
        let amplitude = 0.1; // About -20 dB
        for i in 0..1000 {
            let t = i as f32 / 44100.0;
            let sample = amplitude * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
            buffer.set(i, 0, sample);
            buffer.set(i, 1, sample);
        }

        let rms_before = buffer.rms_db(0);
        comp.process(&mut buffer);
        let rms_after = buffer.rms_db(0);

        // Signal below threshold should not be significantly affected
        assert!(
            (rms_after - rms_before).abs() < 1.0,
            "Signal below threshold should not change much: before={}, after={}",
            rms_before,
            rms_after
        );
    }

    #[test]
    fn test_process_above_threshold() {
        let mut comp = Compressor::with_params(CompressorParams {
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 0.1, // Very fast attack for testing
            release_ms: 10.0,
            ..Default::default()
        });
        comp.prepare(44100.0, 512);

        // Create a buffer with -6 dB signal (above -20 dB threshold)
        let mut buffer = AudioBuffer::new(2, 4410, 44100.0); // 100ms
        let amplitude = 0.5; // About -6 dB
        for i in 0..4410 {
            let t = i as f32 / 44100.0;
            let sample = amplitude * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
            buffer.set(i, 0, sample);
            buffer.set(i, 1, sample);
        }

        let peak_before = buffer.peak_db(0);
        comp.process(&mut buffer);
        let peak_after = buffer.peak_db(0);

        // Signal above threshold should be compressed (reduced)
        assert!(
            peak_after < peak_before,
            "Signal above threshold should be compressed: before={}, after={}",
            peak_before,
            peak_after
        );
    }

    #[test]
    fn test_reset() {
        let mut comp = Compressor::new();
        comp.prepare(44100.0, 512);

        // Process some audio to change internal state
        let mut buffer = AudioBuffer::new(2, 100, 44100.0);
        for i in 0..100 {
            buffer.set(i, 0, 0.9);
            buffer.set(i, 1, 0.9);
        }
        comp.process(&mut buffer);

        // Gain reduction should be non-unity after compression
        let gr_before_reset = comp.gain_reduction[0];

        // Reset
        comp.reset();

        // After reset, gain reduction should be back to 1.0
        assert_eq!(comp.gain_reduction[0], 1.0);
        assert!(
            gr_before_reset < 1.0,
            "GR should have been applied before reset"
        );
    }

    #[test]
    fn test_serialization() {
        let mut comp = Compressor::with_params(CompressorParams {
            threshold_db: -24.0,
            ratio: 6.0,
            attack_ms: 5.0,
            release_ms: 200.0,
            knee_db: 3.0,
            makeup_gain_db: 4.0,
            auto_makeup: false,
        });
        comp.set_id("test-compressor-1".to_string());
        comp.set_enabled(false);

        // Serialize
        let json = comp.to_json().expect("Serialization should succeed");

        // Create new compressor and deserialize
        let mut comp2 = Compressor::new();
        comp2
            .from_json(&json)
            .expect("Deserialization should succeed");

        // Verify
        assert_eq!(comp2.id(), "test-compressor-1");
        assert!(!comp2.is_enabled());
        assert_eq!(comp2.params().threshold_db, -24.0);
        assert_eq!(comp2.params().ratio, 6.0);
        assert_eq!(comp2.params().attack_ms, 5.0);
        assert_eq!(comp2.params().release_ms, 200.0);
        assert_eq!(comp2.params().knee_db, 3.0);
        assert_eq!(comp2.params().makeup_gain_db, 4.0);
    }

    #[test]
    fn test_effect_trait_metadata() {
        let comp = Compressor::new();

        assert_eq!(comp.effect_type(), "compressor");
        assert_eq!(comp.display_name(), "Compressor");

        let meta = comp.metadata();
        assert_eq!(meta.effect_type, "compressor");
        assert_eq!(meta.display_name, "Compressor");
        assert_eq!(meta.category, "dynamics");
        assert_eq!(meta.order_priority, 2);
    }

    #[test]
    fn test_enable_disable() {
        let mut comp = Compressor::new();

        assert!(comp.is_enabled());
        comp.set_enabled(false);
        assert!(!comp.is_enabled());
        comp.set_enabled(true);
        assert!(comp.is_enabled());
    }

    #[test]
    fn test_id_management() {
        let mut comp = Compressor::new();

        assert_eq!(comp.id(), "");
        comp.set_id("my-compressor".to_string());
        assert_eq!(comp.id(), "my-compressor");
    }

    #[test]
    fn test_linear_to_db_conversion() {
        // Test common values
        assert!((Compressor::linear_to_db(1.0) - 0.0).abs() < 0.01);
        assert!((Compressor::linear_to_db(0.5) - (-6.02)).abs() < 0.1);
        assert!((Compressor::linear_to_db(0.1) - (-20.0)).abs() < 0.1);
        assert!((Compressor::linear_to_db(0.0)).abs() > 90.0); // Should be very negative
    }

    #[test]
    fn test_db_to_linear_conversion() {
        // Test common values
        assert!((Compressor::db_to_linear(0.0) - 1.0).abs() < 0.01);
        assert!((Compressor::db_to_linear(-6.0) - 0.501).abs() < 0.01);
        assert!((Compressor::db_to_linear(-20.0) - 0.1).abs() < 0.01);
    }

    #[test]
    fn test_setters() {
        let mut comp = Compressor::new();
        comp.prepare(44100.0, 512);

        comp.set_threshold_db(-30.0);
        assert_eq!(comp.params().threshold_db, -30.0);

        comp.set_ratio(8.0);
        assert_eq!(comp.params().ratio, 8.0);

        comp.set_attack_ms(20.0);
        assert_eq!(comp.params().attack_ms, 20.0);

        comp.set_release_ms(300.0);
        assert_eq!(comp.params().release_ms, 300.0);

        comp.set_knee_db(6.0);
        assert_eq!(comp.params().knee_db, 6.0);

        comp.set_makeup_gain_db(10.0);
        assert_eq!(comp.params().makeup_gain_db, 10.0);

        comp.set_auto_makeup(true);
        assert!(comp.params().auto_makeup);
    }

    #[test]
    fn test_setters_clamp_values() {
        let mut comp = Compressor::new();

        // Values should be clamped
        comp.set_threshold_db(-100.0);
        assert_eq!(comp.params().threshold_db, -60.0);

        comp.set_ratio(100.0);
        assert_eq!(comp.params().ratio, 20.0);

        comp.set_attack_ms(0.001);
        assert_eq!(comp.params().attack_ms, 0.1);

        comp.set_release_ms(5000.0);
        assert_eq!(comp.params().release_ms, 1000.0);

        comp.set_knee_db(50.0);
        assert_eq!(comp.params().knee_db, 12.0);

        comp.set_makeup_gain_db(100.0);
        assert_eq!(comp.params().makeup_gain_db, 24.0);
    }

    #[test]
    fn test_gain_reduction_metering() {
        let mut comp = Compressor::with_params(CompressorParams {
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 0.1,
            ..Default::default()
        });
        comp.prepare(44100.0, 512);

        // Initially no gain reduction
        assert!((comp.gain_reduction_db() - 0.0).abs() < 0.1);

        // Process loud signal
        let mut buffer = AudioBuffer::new(2, 1000, 44100.0);
        for i in 0..1000 {
            buffer.set(i, 0, 0.9);
            buffer.set(i, 1, 0.9);
        }
        comp.process(&mut buffer);

        // Should show gain reduction (negative dB value)
        let gr = comp.gain_reduction_db();
        assert!(
            gr < 0.0,
            "Should show gain reduction after compression: {}",
            gr
        );
    }

    #[test]
    fn test_stereo_linked_detection() {
        let mut comp = Compressor::with_params(CompressorParams {
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 0.1,
            ..Default::default()
        });
        comp.prepare(44100.0, 512);

        // Create buffer with loud signal only in left channel
        let mut buffer = AudioBuffer::new(2, 1000, 44100.0);
        for i in 0..1000 {
            buffer.set(i, 0, 0.9); // Loud left
            buffer.set(i, 1, 0.1); // Quiet right
        }

        comp.process(&mut buffer);

        // Both channels should be compressed equally (linked stereo)
        // The quiet channel should be reduced along with the loud channel
        // Check that both channels have been affected
        let left_peak = buffer.peak_db(0);
        let right_peak = buffer.peak_db(1);

        // Original right channel peak was about -20 dB (0.1 linear)
        // After linked compression, it should be lower
        assert!(
            right_peak < -20.0,
            "Right channel should be compressed due to linked detection: {}",
            right_peak
        );
    }

    #[test]
    fn test_prepare_updates_coefficients() {
        let mut comp = Compressor::with_params(CompressorParams {
            attack_ms: 10.0,
            release_ms: 100.0,
            ..Default::default()
        });

        // Prepare at 44100 Hz
        comp.prepare(44100.0, 512);
        let attack_44k = comp.attack_coeff;
        let release_44k = comp.release_coeff;

        // Prepare at 96000 Hz - coefficients should change
        comp.prepare(96000.0, 512);
        let attack_96k = comp.attack_coeff;
        let release_96k = comp.release_coeff;

        // Higher sample rate means more samples per time period,
        // so the coefficient should be different.
        // The coefficients are exp(-1/samples) where samples = time_ms * sample_rate / 1000
        // At higher sample rates, there are more samples so the coefficient is closer to 1.0
        // The difference can be small (especially for longer time constants like release),
        // so we use a smaller threshold.
        assert!(
            (attack_44k - attack_96k).abs() > 0.0001,
            "Attack coefficients should differ for different sample rates: 44k={}, 96k={}",
            attack_44k,
            attack_96k
        );
        assert!(
            (release_44k - release_96k).abs() > 0.00001,
            "Release coefficients should differ for different sample rates: 44k={}, 96k={}",
            release_44k,
            release_96k
        );

        // Verify the direction: higher sample rate should give coefficient closer to 1.0
        assert!(
            attack_96k > attack_44k,
            "Higher sample rate should give larger attack coefficient"
        );
        assert!(
            release_96k > release_44k,
            "Higher sample rate should give larger release coefficient"
        );
    }
}
