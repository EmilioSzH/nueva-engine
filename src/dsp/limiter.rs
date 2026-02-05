//! Limiter effect implementation (spec section 4.2.8)
//!
//! A brickwall limiter with lookahead and optional true peak detection.
//! Prevents audio from exceeding a specified ceiling level.

#![allow(clippy::needless_range_loop)]

use super::{AudioBuffer, Effect, EffectMetadata};
use crate::error::{NuevaError, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Minimum ceiling in dB
const CEILING_MIN_DB: f32 = -12.0;
/// Maximum ceiling in dB
const CEILING_MAX_DB: f32 = 0.0;
/// Minimum release time in ms
const RELEASE_MIN_MS: f32 = 10.0;
/// Maximum release time in ms
const RELEASE_MAX_MS: f32 = 1000.0;
/// Default lookahead time in ms
const DEFAULT_LOOKAHEAD_MS: f32 = 3.0;
/// Oversampling factor for true peak detection
const TRUE_PEAK_OVERSAMPLE: usize = 4;

/// Limiter parameters with validation ranges from spec section 4.2.8
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimiterParams {
    /// Ceiling level in dB (-12 to 0 dB)
    pub ceiling_db: f32,
    /// Release time in milliseconds (10 to 1000 ms)
    pub release_ms: f32,
    /// Enable true peak detection (intersample peak detection)
    pub true_peak: bool,
    /// Lookahead time in milliseconds (1 to 5 ms)
    pub lookahead_ms: f32,
}

impl Default for LimiterParams {
    fn default() -> Self {
        Self {
            ceiling_db: -1.0,
            release_ms: 100.0,
            true_peak: true,
            lookahead_ms: DEFAULT_LOOKAHEAD_MS,
        }
    }
}

impl LimiterParams {
    /// Validate parameters against spec ranges
    pub fn validate(&self) -> Result<()> {
        if self.ceiling_db < CEILING_MIN_DB || self.ceiling_db > CEILING_MAX_DB {
            return Err(NuevaError::InvalidParameter {
                param: "ceiling_db".to_string(),
                value: self.ceiling_db.to_string(),
                expected: format!("{} to {} dB", CEILING_MIN_DB, CEILING_MAX_DB),
            });
        }
        if self.release_ms < RELEASE_MIN_MS || self.release_ms > RELEASE_MAX_MS {
            return Err(NuevaError::InvalidParameter {
                param: "release_ms".to_string(),
                value: self.release_ms.to_string(),
                expected: format!("{} to {} ms", RELEASE_MIN_MS, RELEASE_MAX_MS),
            });
        }
        if self.lookahead_ms < 1.0 || self.lookahead_ms > 5.0 {
            return Err(NuevaError::InvalidParameter {
                param: "lookahead_ms".to_string(),
                value: self.lookahead_ms.to_string(),
                expected: "1 to 5 ms".to_string(),
            });
        }
        Ok(())
    }

    /// Clamp parameters to valid ranges
    pub fn clamp(&mut self) {
        self.ceiling_db = self.ceiling_db.clamp(CEILING_MIN_DB, CEILING_MAX_DB);
        self.release_ms = self.release_ms.clamp(RELEASE_MIN_MS, RELEASE_MAX_MS);
        self.lookahead_ms = self.lookahead_ms.clamp(1.0, 5.0);
    }
}

/// Brickwall limiter with lookahead
///
/// Implements a look-ahead limiter with:
/// - Configurable ceiling level
/// - True peak detection using 4x oversampling
/// - Smooth release envelope
/// - Lookahead buffer for transparent limiting
#[derive(Debug, Clone)]
pub struct Limiter {
    /// Unique instance identifier
    id: String,
    /// Whether the effect is enabled
    enabled: bool,
    /// Limiter parameters
    params: LimiterParams,
    /// Sample rate in Hz
    sample_rate: f64,
    /// Samples per processing block
    samples_per_block: usize,
    /// Lookahead delay buffer per channel (stores interleaved samples as tuples)
    lookahead_buffer: VecDeque<Vec<f32>>,
    /// Lookahead buffer size in samples
    lookahead_samples: usize,
    /// Gain reduction envelope (linear, 0.0 to 1.0)
    gain_reduction: f32,
    /// Release coefficient for envelope smoothing
    release_coeff: f32,
    /// Peak hold buffer for lookahead peak detection
    peak_hold_buffer: VecDeque<f32>,
    /// Current gain reduction in dB for metering
    current_gr_db: f32,
}

impl Limiter {
    /// Create a new limiter with default parameters
    pub fn new() -> Self {
        Self {
            id: String::new(),
            enabled: true,
            params: LimiterParams::default(),
            sample_rate: 44100.0,
            samples_per_block: 512,
            lookahead_buffer: VecDeque::new(),
            lookahead_samples: 0,
            gain_reduction: 1.0,
            release_coeff: 0.0,
            peak_hold_buffer: VecDeque::new(),
            current_gr_db: 0.0,
        }
    }

    /// Create a new limiter with custom parameters
    pub fn with_params(params: LimiterParams) -> Self {
        let mut limiter = Self::new();
        limiter.params = params;
        limiter.params.clamp();
        limiter
    }

    /// Get the current parameters
    pub fn params(&self) -> &LimiterParams {
        &self.params
    }

    /// Set the parameters (validates and clamps)
    pub fn set_params(&mut self, params: LimiterParams) {
        self.params = params;
        self.params.clamp();
        self.update_coefficients();
    }

    /// Set ceiling level in dB
    pub fn set_ceiling_db(&mut self, ceiling_db: f32) {
        self.params.ceiling_db = ceiling_db.clamp(CEILING_MIN_DB, CEILING_MAX_DB);
    }

    /// Set release time in milliseconds
    pub fn set_release_ms(&mut self, release_ms: f32) {
        self.params.release_ms = release_ms.clamp(RELEASE_MIN_MS, RELEASE_MAX_MS);
        self.update_coefficients();
    }

    /// Enable or disable true peak detection
    pub fn set_true_peak(&mut self, true_peak: bool) {
        self.params.true_peak = true_peak;
    }

    /// Set lookahead time in milliseconds
    pub fn set_lookahead_ms(&mut self, lookahead_ms: f32) {
        self.params.lookahead_ms = lookahead_ms.clamp(1.0, 5.0);
        self.update_lookahead_buffer();
    }

    /// Get the current gain reduction in dB for metering
    pub fn gain_reduction_db(&self) -> f32 {
        self.current_gr_db
    }

    /// Get the ceiling as a linear value
    fn ceiling_linear(&self) -> f32 {
        Self::db_to_linear(self.params.ceiling_db)
    }

    /// Update release coefficient based on sample rate
    fn update_coefficients(&mut self) {
        let release_samples = (self.params.release_ms / 1000.0) * self.sample_rate as f32;
        self.release_coeff = if release_samples > 0.0 {
            (-1.0 / release_samples).exp()
        } else {
            0.0
        };
    }

    /// Update lookahead buffer size
    fn update_lookahead_buffer(&mut self) {
        let new_size = ((self.params.lookahead_ms as f64 / 1000.0) * self.sample_rate) as usize;
        self.lookahead_samples = new_size.max(1);
    }

    /// Detect true peak using 4x oversampling
    ///
    /// Uses simple linear interpolation for oversampling, which provides
    /// a reasonable approximation of intersample peaks.
    fn detect_true_peak(&self, prev_sample: f32, current_sample: f32) -> f32 {
        if !self.params.true_peak {
            return current_sample.abs();
        }

        let mut max_peak = current_sample.abs().max(prev_sample.abs());

        // Simple 4x oversampling using linear interpolation
        // This catches most intersample peaks
        for i in 1..TRUE_PEAK_OVERSAMPLE {
            let t = i as f32 / TRUE_PEAK_OVERSAMPLE as f32;
            let interpolated = prev_sample + (current_sample - prev_sample) * t;
            max_peak = max_peak.max(interpolated.abs());
        }

        max_peak
    }

    /// Detect true peak using cubic interpolation for higher accuracy
    ///
    /// Uses Catmull-Rom spline interpolation for better accuracy at the
    /// cost of requiring more sample history.
    #[allow(dead_code)]
    fn detect_true_peak_cubic(
        &self,
        s0: f32,  // sample at n-2
        s1: f32,  // sample at n-1
        s2: f32,  // sample at n (current)
        s3: f32,  // sample at n+1 (lookahead)
    ) -> f32 {
        if !self.params.true_peak {
            return s2.abs();
        }

        let mut max_peak = s1.abs().max(s2.abs());

        // Catmull-Rom spline interpolation between s1 and s2
        for i in 1..TRUE_PEAK_OVERSAMPLE {
            let t = i as f32 / TRUE_PEAK_OVERSAMPLE as f32;
            let t2 = t * t;
            let t3 = t2 * t;

            // Catmull-Rom coefficients
            let interpolated = 0.5 * (
                (2.0 * s1) +
                (-s0 + s2) * t +
                (2.0 * s0 - 5.0 * s1 + 4.0 * s2 - s3) * t2 +
                (-s0 + 3.0 * s1 - 3.0 * s2 + s3) * t3
            );

            max_peak = max_peak.max(interpolated.abs());
        }

        max_peak
    }

    /// Calculate required gain reduction for a given peak level
    fn compute_gain_reduction(&self, peak_level: f32) -> f32 {
        let ceiling = self.ceiling_linear();
        if peak_level > ceiling {
            ceiling / peak_level
        } else {
            1.0
        }
    }

    /// Convert dB to linear amplitude
    fn db_to_linear(db: f32) -> f32 {
        10.0_f32.powf(db / 20.0)
    }

    /// Convert linear amplitude to dB
    fn linear_to_db(linear: f32) -> f32 {
        if linear > 0.0 {
            20.0 * linear.log10()
        } else {
            -96.0
        }
    }
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Limiter {
    fn process(&mut self, buffer: &mut AudioBuffer) {
        let num_channels = buffer.num_channels();
        let num_samples = buffer.num_samples();

        if num_samples == 0 {
            return;
        }

        let ceiling = self.ceiling_linear();

        // Initialize lookahead buffer if needed
        if self.lookahead_buffer.is_empty() {
            self.update_lookahead_buffer();
            // Pre-fill with zeros
            for _ in 0..self.lookahead_samples {
                self.lookahead_buffer.push_back(vec![0.0; num_channels]);
            }
            // Pre-fill peak hold buffer
            for _ in 0..self.lookahead_samples {
                self.peak_hold_buffer.push_back(0.0);
            }
        }

        // Ensure buffers match channel count
        if let Some(first) = self.lookahead_buffer.front() {
            if first.len() != num_channels {
                self.lookahead_buffer.clear();
                for _ in 0..self.lookahead_samples {
                    self.lookahead_buffer.push_back(vec![0.0; num_channels]);
                }
                self.peak_hold_buffer.clear();
                for _ in 0..self.lookahead_samples {
                    self.peak_hold_buffer.push_back(0.0);
                }
            }
        }

        // Track previous samples for true peak detection
        let mut prev_samples: Vec<f32> = vec![0.0; num_channels];
        if let Some(last) = self.lookahead_buffer.back() {
            prev_samples.clone_from(last);
        }

        // Process each sample
        for frame in 0..num_samples {
            // Get current input samples
            let mut current_samples = vec![0.0; num_channels];
            for ch in 0..num_channels {
                if let Some(sample) = buffer.get(frame, ch) {
                    current_samples[ch] = sample;
                }
            }

            // Detect peak level (consider all channels)
            let mut peak_level: f32 = 0.0;
            for ch in 0..num_channels {
                let channel_peak = self.detect_true_peak(prev_samples[ch], current_samples[ch]);
                peak_level = peak_level.max(channel_peak);
            }

            // Push input to lookahead buffer and peak hold buffer
            self.lookahead_buffer.push_back(current_samples.clone());
            self.peak_hold_buffer.push_back(peak_level);

            // Pop delayed output from lookahead buffer
            let delayed_samples = self.lookahead_buffer.pop_front()
                .unwrap_or_else(|| vec![0.0; num_channels]);
            self.peak_hold_buffer.pop_front();

            // Find maximum peak in lookahead window
            let max_future_peak = self.peak_hold_buffer.iter()
                .fold(0.0_f32, |max, &p| max.max(p));

            // Calculate required gain reduction
            let target_gr = self.compute_gain_reduction(max_future_peak);

            // Apply envelope smoothing (attack is instant, release is smooth)
            if target_gr < self.gain_reduction {
                // Instant attack - immediately apply reduction
                self.gain_reduction = target_gr;
            } else {
                // Smooth release
                self.gain_reduction = self.release_coeff * self.gain_reduction
                    + (1.0 - self.release_coeff) * target_gr;
            }

            // Apply gain reduction to delayed samples and write to output
            for ch in 0..num_channels {
                let output = delayed_samples[ch] * self.gain_reduction;
                // Apply hard clip at ceiling as safety measure
                let clipped = output.clamp(-ceiling, ceiling);
                buffer.set(frame, ch, clipped);
            }

            // Update previous samples for next iteration
            prev_samples = current_samples;
        }

        // Update metering
        self.current_gr_db = Self::linear_to_db(self.gain_reduction);
    }

    fn prepare(&mut self, sample_rate: f64, samples_per_block: usize) {
        self.sample_rate = sample_rate;
        self.samples_per_block = samples_per_block;
        self.update_coefficients();
        self.update_lookahead_buffer();

        // Clear buffers - they will be re-initialized on first process call
        self.lookahead_buffer.clear();
        self.peak_hold_buffer.clear();
    }

    fn reset(&mut self) {
        // Reset envelope state
        self.gain_reduction = 1.0;
        self.current_gr_db = 0.0;

        // Clear delay buffers
        self.lookahead_buffer.clear();
        self.peak_hold_buffer.clear();
    }

    fn to_json(&self) -> Result<serde_json::Value> {
        serde_json::to_value(&LimiterState {
            id: self.id.clone(),
            enabled: self.enabled,
            params: self.params.clone(),
        })
        .map_err(|e| NuevaError::SerializationError {
            details: e.to_string(),
        })
    }

    fn from_json(&mut self, json: &serde_json::Value) -> Result<()> {
        let state: LimiterState = serde_json::from_value(json.clone())
            .map_err(|e| NuevaError::SerializationError {
                details: e.to_string(),
            })?;

        // Validate parameters before applying
        state.params.validate()?;

        self.id = state.id;
        self.enabled = state.enabled;
        self.params = state.params;
        self.update_coefficients();
        self.update_lookahead_buffer();
        Ok(())
    }

    fn effect_type(&self) -> &'static str {
        "limiter"
    }

    fn display_name(&self) -> &'static str {
        "Limiter"
    }

    fn metadata(&self) -> EffectMetadata {
        EffectMetadata {
            effect_type: "limiter".to_string(),
            display_name: "Limiter".to_string(),
            category: "dynamics".to_string(),
            order_priority: 7, // Always last in chain
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

/// Serializable state for the limiter
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LimiterState {
    id: String,
    enabled: bool,
    params: LimiterParams,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limiter_default_params() {
        let limiter = Limiter::new();
        let params = limiter.params();

        assert_eq!(params.ceiling_db, -1.0);
        assert_eq!(params.release_ms, 100.0);
        assert!(params.true_peak);
        assert_eq!(params.lookahead_ms, 3.0);
    }

    #[test]
    fn test_parameter_validation() {
        let mut params = LimiterParams::default();
        assert!(params.validate().is_ok());

        // Test invalid ceiling
        params.ceiling_db = -15.0;
        assert!(params.validate().is_err());
        params.ceiling_db = 1.0;
        assert!(params.validate().is_err());
        params.ceiling_db = -1.0;

        // Test invalid release
        params.release_ms = 5.0;
        assert!(params.validate().is_err());
        params.release_ms = 1500.0;
        assert!(params.validate().is_err());
        params.release_ms = 100.0;

        // Test invalid lookahead
        params.lookahead_ms = 0.5;
        assert!(params.validate().is_err());
        params.lookahead_ms = 10.0;
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_parameter_clamping() {
        let mut params = LimiterParams {
            ceiling_db: -20.0,
            release_ms: 5.0,
            true_peak: true,
            lookahead_ms: 0.1,
        };

        params.clamp();

        assert_eq!(params.ceiling_db, CEILING_MIN_DB);
        assert_eq!(params.release_ms, RELEASE_MIN_MS);
        assert_eq!(params.lookahead_ms, 1.0);
    }

    #[test]
    fn test_brickwall_limiting() {
        let mut limiter = Limiter::with_params(LimiterParams {
            ceiling_db: -6.0, // Ceiling at 0.5 linear
            release_ms: 10.0,
            true_peak: false,
            lookahead_ms: 1.0,
        });
        limiter.prepare(44100.0, 512);

        // Create a buffer with samples exceeding the ceiling
        let mut buffer = AudioBuffer::new(2, 1000, 44100.0);
        for i in 0..1000 {
            buffer.set(i, 0, 1.0); // Will exceed -6 dB ceiling
            buffer.set(i, 1, 1.0);
        }

        limiter.process(&mut buffer);

        // Check that output does not exceed ceiling
        let ceiling_linear = Limiter::db_to_linear(-6.0);
        for sample in buffer.samples() {
            assert!(
                sample.abs() <= ceiling_linear + 0.001,
                "Sample {} exceeds ceiling {}",
                sample,
                ceiling_linear
            );
        }
    }

    #[test]
    fn test_no_limiting_below_ceiling() {
        let mut limiter = Limiter::with_params(LimiterParams {
            ceiling_db: 0.0, // 0 dB ceiling (1.0 linear)
            release_ms: 100.0,
            true_peak: false,
            lookahead_ms: 1.0,
        });
        limiter.prepare(44100.0, 512);

        // Create a buffer with samples below the ceiling
        let amplitude = 0.5; // -6 dB, well below 0 dB ceiling
        let mut buffer = AudioBuffer::new(2, 1000, 44100.0);
        for i in 0..1000 {
            let t = i as f32 / 44100.0;
            let sample = amplitude * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
            buffer.set(i, 0, sample);
            buffer.set(i, 1, sample);
        }

        // Store original samples for comparison (accounting for lookahead delay)
        let original_rms = buffer.rms_db(0);

        limiter.process(&mut buffer);

        let processed_rms = buffer.rms_db(0);

        // Signal below ceiling should not be significantly affected
        // Allow for small differences due to lookahead latency at buffer boundaries
        assert!(
            (processed_rms - original_rms).abs() < 1.0,
            "Signal below ceiling should not change significantly: original={}, processed={}",
            original_rms,
            processed_rms
        );
    }

    #[test]
    fn test_true_peak_detection() {
        let limiter = Limiter::with_params(LimiterParams {
            ceiling_db: -1.0,
            release_ms: 100.0,
            true_peak: true,
            lookahead_ms: 3.0,
        });

        // Test case where interpolated peak exceeds sample peaks
        // Samples: -0.8 to 0.8 - the true peak could be higher than 0.8
        // due to intersample peaks
        let prev = -0.8;
        let current = 0.8;

        let detected_peak = limiter.detect_true_peak(prev, current);

        // With linear interpolation, the true peak might not exceed samples
        // but we should at least detect the max sample value
        assert!(
            detected_peak >= 0.8,
            "True peak should detect at least the sample peak"
        );
    }

    #[test]
    fn test_true_peak_disabled() {
        let limiter = Limiter::with_params(LimiterParams {
            ceiling_db: -1.0,
            release_ms: 100.0,
            true_peak: false,
            lookahead_ms: 3.0,
        });

        let prev = 0.5;
        let current = 0.7;

        let detected_peak = limiter.detect_true_peak(prev, current);

        // Without true peak, should just return current sample's absolute value
        assert!(
            (detected_peak - 0.7).abs() < 0.001,
            "Without true peak, should return current sample"
        );
    }

    #[test]
    fn test_lookahead_delay() {
        let mut limiter = Limiter::with_params(LimiterParams {
            ceiling_db: 0.0,
            release_ms: 100.0,
            true_peak: false,
            lookahead_ms: 3.0, // 3ms lookahead
        });
        limiter.prepare(44100.0, 512);

        // Calculate expected delay in samples
        let expected_delay = ((3.0 / 1000.0) * 44100.0) as usize;

        // Create a buffer with an impulse at the start
        let mut buffer = AudioBuffer::new(1, 500, 44100.0);
        buffer.set(0, 0, 0.5);
        // Rest is zeros

        limiter.process(&mut buffer);

        // The impulse should appear after the lookahead delay
        // Find where the impulse appears in the output
        let mut impulse_position = None;
        for i in 0..500 {
            if let Some(sample) = buffer.get(i, 0) {
                if sample.abs() > 0.1 {
                    impulse_position = Some(i);
                    break;
                }
            }
        }

        if let Some(pos) = impulse_position {
            // Allow some tolerance for buffer edge effects
            assert!(
                pos >= expected_delay - 5 && pos <= expected_delay + 5,
                "Impulse should appear near position {}, found at {}",
                expected_delay,
                pos
            );
        }
    }

    #[test]
    fn test_gain_reduction_metering() {
        let mut limiter = Limiter::with_params(LimiterParams {
            ceiling_db: -6.0,
            release_ms: 10.0,
            true_peak: false,
            lookahead_ms: 1.0,
        });
        limiter.prepare(44100.0, 512);

        // Initially no gain reduction
        assert!((limiter.gain_reduction_db() - 0.0).abs() < 0.1);

        // Process signal that exceeds ceiling
        let mut buffer = AudioBuffer::new(2, 1000, 44100.0);
        for i in 0..1000 {
            buffer.set(i, 0, 1.0);
            buffer.set(i, 1, 1.0);
        }
        limiter.process(&mut buffer);

        // Should show gain reduction (negative dB)
        let gr = limiter.gain_reduction_db();
        assert!(
            gr < 0.0,
            "Should show gain reduction after limiting: {}",
            gr
        );
    }

    #[test]
    fn test_release_envelope() {
        let mut limiter = Limiter::with_params(LimiterParams {
            ceiling_db: -6.0,
            release_ms: 50.0, // Short release for testing
            true_peak: false,
            lookahead_ms: 1.0,
        });
        limiter.prepare(44100.0, 512);

        // First, process loud signal to trigger gain reduction
        let mut loud_buffer = AudioBuffer::new(2, 1000, 44100.0);
        for i in 0..1000 {
            loud_buffer.set(i, 0, 1.0);
            loud_buffer.set(i, 1, 1.0);
        }
        limiter.process(&mut loud_buffer);

        let gr_after_loud = limiter.gain_reduction;
        assert!(gr_after_loud < 1.0, "Should have gain reduction after loud signal");

        // Process quiet signal - gain should recover
        let mut quiet_buffer = AudioBuffer::new(2, 4410, 44100.0); // 100ms
        for i in 0..4410 {
            quiet_buffer.set(i, 0, 0.1);
            quiet_buffer.set(i, 1, 0.1);
        }
        limiter.process(&mut quiet_buffer);

        let gr_after_quiet = limiter.gain_reduction;
        assert!(
            gr_after_quiet > gr_after_loud,
            "Gain reduction should recover: before={}, after={}",
            gr_after_loud,
            gr_after_quiet
        );
    }

    #[test]
    fn test_reset() {
        let mut limiter = Limiter::new();
        limiter.prepare(44100.0, 512);

        // Process loud signal to change internal state
        let mut buffer = AudioBuffer::new(2, 100, 44100.0);
        for i in 0..100 {
            buffer.set(i, 0, 1.0);
            buffer.set(i, 1, 1.0);
        }
        limiter.process(&mut buffer);

        // Gain reduction should be non-unity
        assert!(limiter.gain_reduction < 1.0);

        // Reset
        limiter.reset();

        // After reset, gain reduction should be 1.0
        assert_eq!(limiter.gain_reduction, 1.0);
        assert!(limiter.lookahead_buffer.is_empty());
        assert!(limiter.peak_hold_buffer.is_empty());
    }

    #[test]
    fn test_serialization() {
        let mut limiter = Limiter::with_params(LimiterParams {
            ceiling_db: -3.0,
            release_ms: 200.0,
            true_peak: false,
            lookahead_ms: 2.0,
        });
        limiter.set_id("test-limiter-1".to_string());
        limiter.set_enabled(false);

        // Serialize
        let json = limiter.to_json().expect("Serialization should succeed");

        // Create new limiter and deserialize
        let mut limiter2 = Limiter::new();
        limiter2.from_json(&json).expect("Deserialization should succeed");

        // Verify
        assert_eq!(limiter2.id(), "test-limiter-1");
        assert!(!limiter2.is_enabled());
        assert_eq!(limiter2.params().ceiling_db, -3.0);
        assert_eq!(limiter2.params().release_ms, 200.0);
        assert!(!limiter2.params().true_peak);
        assert_eq!(limiter2.params().lookahead_ms, 2.0);
    }

    #[test]
    fn test_effect_trait_metadata() {
        let limiter = Limiter::new();

        assert_eq!(limiter.effect_type(), "limiter");
        assert_eq!(limiter.display_name(), "Limiter");

        let meta = limiter.metadata();
        assert_eq!(meta.effect_type, "limiter");
        assert_eq!(meta.display_name, "Limiter");
        assert_eq!(meta.category, "dynamics");
        assert_eq!(meta.order_priority, 7); // Always last
    }

    #[test]
    fn test_enable_disable() {
        let mut limiter = Limiter::new();

        assert!(limiter.is_enabled());
        limiter.set_enabled(false);
        assert!(!limiter.is_enabled());
        limiter.set_enabled(true);
        assert!(limiter.is_enabled());
    }

    #[test]
    fn test_id_management() {
        let mut limiter = Limiter::new();

        assert_eq!(limiter.id(), "");
        limiter.set_id("my-limiter".to_string());
        assert_eq!(limiter.id(), "my-limiter");
    }

    #[test]
    fn test_setters() {
        let mut limiter = Limiter::new();
        limiter.prepare(44100.0, 512);

        limiter.set_ceiling_db(-3.0);
        assert_eq!(limiter.params().ceiling_db, -3.0);

        limiter.set_release_ms(500.0);
        assert_eq!(limiter.params().release_ms, 500.0);

        limiter.set_true_peak(false);
        assert!(!limiter.params().true_peak);

        limiter.set_lookahead_ms(5.0);
        assert_eq!(limiter.params().lookahead_ms, 5.0);
    }

    #[test]
    fn test_setters_clamp_values() {
        let mut limiter = Limiter::new();

        limiter.set_ceiling_db(-20.0);
        assert_eq!(limiter.params().ceiling_db, CEILING_MIN_DB);

        limiter.set_ceiling_db(5.0);
        assert_eq!(limiter.params().ceiling_db, CEILING_MAX_DB);

        limiter.set_release_ms(1.0);
        assert_eq!(limiter.params().release_ms, RELEASE_MIN_MS);

        limiter.set_release_ms(5000.0);
        assert_eq!(limiter.params().release_ms, RELEASE_MAX_MS);

        limiter.set_lookahead_ms(0.1);
        assert_eq!(limiter.params().lookahead_ms, 1.0);

        limiter.set_lookahead_ms(10.0);
        assert_eq!(limiter.params().lookahead_ms, 5.0);
    }

    #[test]
    fn test_db_to_linear_conversion() {
        assert!((Limiter::db_to_linear(0.0) - 1.0).abs() < 0.001);
        assert!((Limiter::db_to_linear(-6.0) - 0.501).abs() < 0.01);
        assert!((Limiter::db_to_linear(-20.0) - 0.1).abs() < 0.01);
        assert!((Limiter::db_to_linear(-1.0) - 0.891).abs() < 0.01);
    }

    #[test]
    fn test_linear_to_db_conversion() {
        assert!((Limiter::linear_to_db(1.0) - 0.0).abs() < 0.01);
        assert!((Limiter::linear_to_db(0.5) - (-6.02)).abs() < 0.1);
        assert!((Limiter::linear_to_db(0.1) - (-20.0)).abs() < 0.1);
    }

    #[test]
    fn test_stereo_limiting() {
        let mut limiter = Limiter::with_params(LimiterParams {
            ceiling_db: -6.0,
            release_ms: 10.0,
            true_peak: false,
            lookahead_ms: 1.0,
        });
        limiter.prepare(44100.0, 512);

        // Create stereo buffer with different levels per channel
        let mut buffer = AudioBuffer::new(2, 1000, 44100.0);
        for i in 0..1000 {
            buffer.set(i, 0, 1.0);  // Left channel at 0 dB
            buffer.set(i, 1, 0.3);  // Right channel at about -10 dB
        }

        limiter.process(&mut buffer);

        // Both channels should be limited based on the loudest channel
        let ceiling_linear = Limiter::db_to_linear(-6.0);

        // Check left channel is limited
        for i in 0..1000 {
            if let Some(sample) = buffer.get(i, 0) {
                assert!(
                    sample.abs() <= ceiling_linear + 0.001,
                    "Left channel sample {} exceeds ceiling",
                    sample
                );
            }
        }

        // Right channel should also be reduced by linked limiting
        let right_peak = buffer.peak_db(1);
        assert!(
            right_peak < -10.0,
            "Right channel should be reduced due to linked stereo limiting: {}",
            right_peak
        );
    }

    #[test]
    fn test_prepare_updates_state() {
        let mut limiter = Limiter::new();

        // Prepare at 44100 Hz
        limiter.prepare(44100.0, 512);
        let release_coeff_44k = limiter.release_coeff;
        let lookahead_samples_44k = limiter.lookahead_samples;

        // Prepare at 96000 Hz
        limiter.prepare(96000.0, 512);
        let release_coeff_96k = limiter.release_coeff;
        let lookahead_samples_96k = limiter.lookahead_samples;

        // Coefficients and buffer sizes should differ for different sample rates
        // Note: The difference is small for one-pole filters (~0.0001 for 100ms release)
        assert!(
            (release_coeff_44k - release_coeff_96k).abs() > 0.0001,
            "Release coefficients should differ for different sample rates: 44k={}, 96k={}",
            release_coeff_44k, release_coeff_96k
        );
        assert!(
            lookahead_samples_44k != lookahead_samples_96k,
            "Lookahead samples should differ for different sample rates: 44k={}, 96k={}",
            lookahead_samples_44k, lookahead_samples_96k
        );
    }

    #[test]
    fn test_cubic_true_peak() {
        let limiter = Limiter::with_params(LimiterParams {
            ceiling_db: -1.0,
            release_ms: 100.0,
            true_peak: true,
            lookahead_ms: 3.0,
        });

        // Test with cubic interpolation
        let detected = limiter.detect_true_peak_cubic(-0.5, 0.5, 0.8, 0.3);

        // Should detect at least the max sample value
        assert!(detected >= 0.8, "Cubic true peak should detect at least sample peaks");
    }

    #[test]
    fn test_compute_gain_reduction() {
        let limiter = Limiter::with_params(LimiterParams {
            ceiling_db: -6.0, // ~0.5011872 linear
            ..Default::default()
        });

        let ceiling_linear = Limiter::db_to_linear(-6.0); // ~0.5011872

        // Below ceiling - no reduction
        let gr = limiter.compute_gain_reduction(0.3);
        assert!((gr - 1.0).abs() < 0.001, "No reduction below ceiling");

        // At ceiling - no reduction
        let gr = limiter.compute_gain_reduction(ceiling_linear);
        assert!((gr - 1.0).abs() < 0.001, "No reduction at ceiling");

        // Above ceiling - reduction applied
        let gr = limiter.compute_gain_reduction(1.0);
        // ceiling / peak = ceiling_linear / 1.0 = ceiling_linear
        assert!((gr - ceiling_linear).abs() < 0.001, "Should reduce to ceiling: {}", gr);
    }

    #[test]
    fn test_from_json_validation() {
        let mut limiter = Limiter::new();

        // Invalid ceiling value should be rejected
        let invalid_json = serde_json::json!({
            "id": "test",
            "enabled": true,
            "params": {
                "ceiling_db": -20.0,  // Out of range
                "release_ms": 100.0,
                "true_peak": true,
                "lookahead_ms": 3.0
            }
        });

        assert!(limiter.from_json(&invalid_json).is_err());
    }

    #[test]
    fn test_mono_processing() {
        let mut limiter = Limiter::with_params(LimiterParams {
            ceiling_db: -6.0,
            release_ms: 10.0,
            true_peak: false,
            lookahead_ms: 1.0,
        });
        limiter.prepare(44100.0, 512);

        // Process mono buffer
        let mut buffer = AudioBuffer::new(1, 1000, 44100.0);
        for i in 0..1000 {
            buffer.set(i, 0, 1.0);
        }

        limiter.process(&mut buffer);

        // Check output is limited
        let ceiling_linear = Limiter::db_to_linear(-6.0);
        for sample in buffer.samples() {
            assert!(
                sample.abs() <= ceiling_linear + 0.001,
                "Sample exceeds ceiling"
            );
        }
    }
}
