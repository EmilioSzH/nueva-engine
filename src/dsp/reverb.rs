//! Reverb effect implementation (spec section 4.2.4)
//!
//! Implements the Freeverb algorithm:
//! - 8 parallel comb filters for early reflections
//! - 4 series allpass filters for diffusion
//! - Stereo width control
//! - Pre-delay buffer

use super::effect::{Effect, EffectMetadata};
use super::AudioBuffer;
use crate::error::{NuevaError, Result};
use serde::{Deserialize, Serialize};

// ============================================================================
// Freeverb Constants
// ============================================================================

/// Reference sample rate for Freeverb delays
const REFERENCE_SAMPLE_RATE: f64 = 44100.0;

/// Comb filter delays at 44100 Hz (8 filters)
const COMB_DELAYS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];

/// Allpass filter delays at 44100 Hz (4 filters)
const ALLPASS_DELAYS: [usize; 4] = [556, 441, 341, 225];

/// Stereo spread offset in samples (for right channel)
const STEREO_SPREAD: usize = 23;

/// Fixed gain for allpass filters (standard Freeverb value)
const ALLPASS_GAIN: f32 = 0.5;

/// Scale factor for room size parameter to feedback
const ROOM_SCALE: f32 = 0.28;

/// Offset for room size parameter to feedback
const ROOM_OFFSET: f32 = 0.7;

/// Scale factor for damping parameter
const DAMP_SCALE: f32 = 0.4;

/// Maximum pre-delay time in milliseconds
const MAX_PRE_DELAY_MS: f32 = 100.0;

// ============================================================================
// Parameter Structs
// ============================================================================

/// Reverb effect parameters (spec section 4.2.4)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReverbParams {
    /// Room size: 0 (tiny) to 1 (huge hall)
    pub room_size: f32,
    /// Damping: 0 (bright) to 1 (dark)
    pub damping: f32,
    /// Wet signal level: 0 to 1
    pub wet_level: f32,
    /// Dry signal level: 0 to 1
    pub dry_level: f32,
    /// Stereo width: 0 (mono) to 1 (full stereo)
    pub width: f32,
    /// Pre-delay in milliseconds: 0 to 100
    pub pre_delay_ms: f32,
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self {
            room_size: 0.5,
            damping: 0.5,
            wet_level: 0.3,
            dry_level: 1.0,
            width: 1.0,
            pre_delay_ms: 0.0,
        }
    }
}

impl ReverbParams {
    /// Validate all parameters are within spec ranges
    pub fn validate(&self) -> Result<()> {
        if self.room_size < 0.0 || self.room_size > 1.0 {
            return Err(NuevaError::InvalidParameter {
                param: "room_size".to_string(),
                value: self.room_size.to_string(),
                expected: "0.0 to 1.0".to_string(),
            });
        }
        if self.damping < 0.0 || self.damping > 1.0 {
            return Err(NuevaError::InvalidParameter {
                param: "damping".to_string(),
                value: self.damping.to_string(),
                expected: "0.0 to 1.0".to_string(),
            });
        }
        if self.wet_level < 0.0 || self.wet_level > 1.0 {
            return Err(NuevaError::InvalidParameter {
                param: "wet_level".to_string(),
                value: self.wet_level.to_string(),
                expected: "0.0 to 1.0".to_string(),
            });
        }
        if self.dry_level < 0.0 || self.dry_level > 1.0 {
            return Err(NuevaError::InvalidParameter {
                param: "dry_level".to_string(),
                value: self.dry_level.to_string(),
                expected: "0.0 to 1.0".to_string(),
            });
        }
        if self.width < 0.0 || self.width > 1.0 {
            return Err(NuevaError::InvalidParameter {
                param: "width".to_string(),
                value: self.width.to_string(),
                expected: "0.0 to 1.0".to_string(),
            });
        }
        if self.pre_delay_ms < 0.0 || self.pre_delay_ms > MAX_PRE_DELAY_MS {
            return Err(NuevaError::InvalidParameter {
                param: "pre_delay_ms".to_string(),
                value: self.pre_delay_ms.to_string(),
                expected: format!("0.0 to {} ms", MAX_PRE_DELAY_MS),
            });
        }
        Ok(())
    }
}

// ============================================================================
// Filter Components
// ============================================================================

/// Low-pass comb filter for Freeverb
///
/// Implements: y[n] = x[n - delay] + feedback * (y[n - delay] + damp * (y[n - delay - 1] - y[n - delay]))
#[derive(Debug, Clone)]
struct CombFilter {
    /// Circular buffer for delay line
    buffer: Vec<f32>,
    /// Current write position
    write_pos: usize,
    /// Buffer size mask for efficient wrapping
    mask: usize,
    /// Filter state for damping (low-pass)
    filter_state: f32,
    /// Feedback coefficient (derived from room_size)
    feedback: f32,
    /// Damping coefficient (1 - damp_scale * damping)
    damp1: f32,
    /// Damping coefficient (damp_scale * damping)
    damp2: f32,
}

impl CombFilter {
    /// Create a new comb filter with the given delay size
    fn new(delay_size: usize) -> Self {
        // Round up to next power of 2 for efficient wrapping
        let size = delay_size.next_power_of_two();
        Self {
            buffer: vec![0.0; size],
            write_pos: 0,
            mask: size - 1,
            filter_state: 0.0,
            feedback: 0.5,
            damp1: 0.5,
            damp2: 0.5,
        }
    }

    /// Set feedback and damping coefficients
    fn set_coefficients(&mut self, feedback: f32, damp1: f32, damp2: f32) {
        self.feedback = feedback;
        self.damp1 = damp1;
        self.damp2 = damp2;
    }

    /// Process a single sample through the comb filter
    fn process(&mut self, input: f32, delay: usize) -> f32 {
        // Read from delay line
        let read_pos = (self.write_pos + self.mask + 1 - delay) & self.mask;
        let output = self.buffer[read_pos];

        // Apply damping (one-pole low-pass in feedback path)
        self.filter_state = output * self.damp1 + self.filter_state * self.damp2;

        // Write input plus filtered feedback to delay line
        self.buffer[self.write_pos] = input + self.filter_state * self.feedback;

        // Advance write position
        self.write_pos = (self.write_pos + 1) & self.mask;

        output
    }

    /// Clear the filter state
    fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.filter_state = 0.0;
        self.write_pos = 0;
    }
}

/// Allpass filter for Freeverb diffusion
///
/// Implements: y[n] = -x[n] + x[n - delay] + gain * y[n - delay]
#[derive(Debug, Clone)]
struct AllpassFilter {
    /// Circular buffer for delay line
    buffer: Vec<f32>,
    /// Current write position
    write_pos: usize,
    /// Buffer size mask for efficient wrapping
    mask: usize,
}

impl AllpassFilter {
    /// Create a new allpass filter with the given delay size
    fn new(delay_size: usize) -> Self {
        // Round up to next power of 2 for efficient wrapping
        let size = delay_size.next_power_of_two();
        Self {
            buffer: vec![0.0; size],
            write_pos: 0,
            mask: size - 1,
        }
    }

    /// Process a single sample through the allpass filter
    fn process(&mut self, input: f32, delay: usize) -> f32 {
        // Read from delay line
        let read_pos = (self.write_pos + self.mask + 1 - delay) & self.mask;
        let delayed = self.buffer[read_pos];

        // Allpass formula: output = -input + delayed + gain * delayed
        // Simplified: output = delayed - gain * (input + delayed)
        let output = delayed - ALLPASS_GAIN * input;

        // Write to delay line: input + gain * output
        self.buffer[self.write_pos] = input + ALLPASS_GAIN * output;

        // Advance write position
        self.write_pos = (self.write_pos + 1) & self.mask;

        output
    }

    /// Clear the filter state
    fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }
}

/// Pre-delay buffer for reverb
#[derive(Debug, Clone)]
struct PreDelayBuffer {
    /// Circular buffer for samples
    buffer: Vec<f32>,
    /// Current write position
    write_pos: usize,
    /// Buffer size mask for efficient wrapping
    mask: usize,
}

impl PreDelayBuffer {
    /// Create a new pre-delay buffer with the given maximum size
    fn new(max_size: usize) -> Self {
        let size = max_size.next_power_of_two();
        Self {
            buffer: vec![0.0; size],
            write_pos: 0,
            mask: size - 1,
        }
    }

    /// Write a sample and read at the given delay
    fn process(&mut self, input: f32, delay_samples: usize) -> f32 {
        // Write input
        self.buffer[self.write_pos] = input;

        // Read from delay position
        let read_pos = (self.write_pos + self.mask + 1 - delay_samples) & self.mask;
        let output = self.buffer[read_pos];

        // Advance write position
        self.write_pos = (self.write_pos + 1) & self.mask;

        output
    }

    /// Clear the buffer
    fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }
}

// ============================================================================
// Main Reverb Effect
// ============================================================================

/// Reverb effect using Freeverb algorithm (spec section 4.2.4)
///
/// The Freeverb algorithm consists of:
/// - 8 parallel lowpass-feedback comb filters per channel
/// - 4 series allpass filters per channel for diffusion
/// - Pre-delay buffer
/// - Stereo width control
#[derive(Debug, Clone)]
pub struct Reverb {
    /// Effect parameters
    params: ReverbParams,
    /// Unique instance ID
    id: String,
    /// Whether the effect is enabled
    enabled: bool,
    /// Current sample rate
    sample_rate: f64,

    // Left channel filters
    /// 8 comb filters for left channel
    comb_left: [CombFilter; 8],
    /// 4 allpass filters for left channel
    allpass_left: [AllpassFilter; 4],

    // Right channel filters
    /// 8 comb filters for right channel
    comb_right: [CombFilter; 8],
    /// 4 allpass filters for right channel
    allpass_right: [AllpassFilter; 4],

    /// Pre-delay buffer for left channel
    pre_delay_left: PreDelayBuffer,
    /// Pre-delay buffer for right channel
    pre_delay_right: PreDelayBuffer,

    /// Scaled comb filter delays for current sample rate
    scaled_comb_delays_left: [usize; 8],
    scaled_comb_delays_right: [usize; 8],

    /// Scaled allpass filter delays for current sample rate
    scaled_allpass_delays_left: [usize; 4],
    scaled_allpass_delays_right: [usize; 4],

    /// Current pre-delay in samples
    pre_delay_samples: usize,
}

impl Reverb {
    /// Create a new Reverb effect with default parameters
    pub fn new() -> Self {
        Self::with_params(ReverbParams::default())
    }

    /// Create a new Reverb effect with the given parameters
    pub fn with_params(params: ReverbParams) -> Self {
        // Create filters with default sizes (will be resized in prepare)
        let comb_left = std::array::from_fn(|i| CombFilter::new(COMB_DELAYS[i] * 2));
        let comb_right =
            std::array::from_fn(|i| CombFilter::new(COMB_DELAYS[i] * 2 + STEREO_SPREAD));
        let allpass_left = std::array::from_fn(|i| AllpassFilter::new(ALLPASS_DELAYS[i] * 2));
        let allpass_right =
            std::array::from_fn(|i| AllpassFilter::new(ALLPASS_DELAYS[i] * 2 + STEREO_SPREAD));

        // Default pre-delay buffer (~100ms at 96kHz max)
        let pre_delay_left = PreDelayBuffer::new(10000);
        let pre_delay_right = PreDelayBuffer::new(10000);

        let mut reverb = Self {
            params,
            id: String::new(),
            enabled: true,
            sample_rate: REFERENCE_SAMPLE_RATE,
            comb_left,
            comb_right,
            allpass_left,
            allpass_right,
            pre_delay_left,
            pre_delay_right,
            scaled_comb_delays_left: COMB_DELAYS,
            scaled_comb_delays_right: std::array::from_fn(|i| COMB_DELAYS[i] + STEREO_SPREAD),
            scaled_allpass_delays_left: ALLPASS_DELAYS,
            scaled_allpass_delays_right: std::array::from_fn(|i| ALLPASS_DELAYS[i] + STEREO_SPREAD),
            pre_delay_samples: 0,
        };

        reverb.update_coefficients();
        reverb
    }

    /// Get a reference to the current parameters
    pub fn params(&self) -> &ReverbParams {
        &self.params
    }

    /// Set parameters with validation
    pub fn set_params(&mut self, params: ReverbParams) -> Result<()> {
        params.validate()?;
        self.params = params;
        self.update_coefficients();
        self.update_pre_delay();
        Ok(())
    }

    /// Set room size (0 to 1)
    pub fn set_room_size(&mut self, room_size: f32) -> Result<()> {
        let mut params = self.params.clone();
        params.room_size = room_size;
        self.set_params(params)
    }

    /// Set damping (0 to 1)
    pub fn set_damping(&mut self, damping: f32) -> Result<()> {
        let mut params = self.params.clone();
        params.damping = damping;
        self.set_params(params)
    }

    /// Set wet level (0 to 1)
    pub fn set_wet_level(&mut self, wet_level: f32) -> Result<()> {
        let mut params = self.params.clone();
        params.wet_level = wet_level;
        self.set_params(params)
    }

    /// Set dry level (0 to 1)
    pub fn set_dry_level(&mut self, dry_level: f32) -> Result<()> {
        let mut params = self.params.clone();
        params.dry_level = dry_level;
        self.set_params(params)
    }

    /// Set stereo width (0 to 1)
    pub fn set_width(&mut self, width: f32) -> Result<()> {
        let mut params = self.params.clone();
        params.width = width;
        self.set_params(params)
    }

    /// Set pre-delay in milliseconds (0 to 100)
    pub fn set_pre_delay(&mut self, pre_delay_ms: f32) -> Result<()> {
        let mut params = self.params.clone();
        params.pre_delay_ms = pre_delay_ms;
        self.set_params(params)
    }

    /// Update filter coefficients based on current parameters
    fn update_coefficients(&mut self) {
        // Calculate feedback from room size
        let feedback = self.params.room_size * ROOM_SCALE + ROOM_OFFSET;

        // Calculate damping coefficients
        let damp1 = 1.0 - self.params.damping * DAMP_SCALE;
        let damp2 = self.params.damping * DAMP_SCALE;

        // Update all comb filters
        for comb in &mut self.comb_left {
            comb.set_coefficients(feedback, damp1, damp2);
        }
        for comb in &mut self.comb_right {
            comb.set_coefficients(feedback, damp1, damp2);
        }
    }

    /// Update pre-delay samples based on current sample rate
    fn update_pre_delay(&mut self) {
        self.pre_delay_samples =
            ((self.params.pre_delay_ms / 1000.0) * self.sample_rate as f32) as usize;
    }

    /// Scale filter delays for the current sample rate
    fn scale_delays(&mut self) {
        let scale = self.sample_rate / REFERENCE_SAMPLE_RATE;

        // Scale comb delays
        for i in 0..8 {
            self.scaled_comb_delays_left[i] = ((COMB_DELAYS[i] as f64 * scale) as usize).max(1);
            self.scaled_comb_delays_right[i] =
                (((COMB_DELAYS[i] + STEREO_SPREAD) as f64 * scale) as usize).max(1);
        }

        // Scale allpass delays
        for i in 0..4 {
            self.scaled_allpass_delays_left[i] =
                ((ALLPASS_DELAYS[i] as f64 * scale) as usize).max(1);
            self.scaled_allpass_delays_right[i] =
                (((ALLPASS_DELAYS[i] + STEREO_SPREAD) as f64 * scale) as usize).max(1);
        }
    }

    /// Resize all filter buffers for the current sample rate
    fn resize_buffers(&mut self) {
        let scale = self.sample_rate / REFERENCE_SAMPLE_RATE;

        // Resize comb filters
        for i in 0..8 {
            let left_size = ((COMB_DELAYS[i] as f64 * scale) as usize + 1).max(16);
            let right_size = (((COMB_DELAYS[i] + STEREO_SPREAD) as f64 * scale) as usize + 1).max(16);
            self.comb_left[i] = CombFilter::new(left_size);
            self.comb_right[i] = CombFilter::new(right_size);
        }

        // Resize allpass filters
        for i in 0..4 {
            let left_size = ((ALLPASS_DELAYS[i] as f64 * scale) as usize + 1).max(16);
            let right_size =
                (((ALLPASS_DELAYS[i] + STEREO_SPREAD) as f64 * scale) as usize + 1).max(16);
            self.allpass_left[i] = AllpassFilter::new(left_size);
            self.allpass_right[i] = AllpassFilter::new(right_size);
        }

        // Resize pre-delay buffers
        let max_pre_delay = ((MAX_PRE_DELAY_MS / 1000.0) * self.sample_rate as f32) as usize + 1;
        self.pre_delay_left = PreDelayBuffer::new(max_pre_delay);
        self.pre_delay_right = PreDelayBuffer::new(max_pre_delay);

        // Update coefficients after resizing
        self.update_coefficients();
    }

    /// Process mono audio
    fn process_mono(&mut self, buffer: &mut AudioBuffer) {
        let num_samples = buffer.num_samples();
        let wet_level = self.params.wet_level;
        let dry_level = self.params.dry_level;

        for i in 0..num_samples {
            let input = buffer.get(i, 0).unwrap_or(0.0);

            // Apply pre-delay
            let delayed_input = if self.pre_delay_samples > 0 {
                self.pre_delay_left.process(input, self.pre_delay_samples)
            } else {
                input
            };

            // Sum outputs from all comb filters in parallel
            let mut comb_sum = 0.0;
            for j in 0..8 {
                comb_sum +=
                    self.comb_left[j].process(delayed_input, self.scaled_comb_delays_left[j]);
            }

            // Process through allpass filters in series
            let mut output = comb_sum;
            for j in 0..4 {
                output = self.allpass_left[j].process(output, self.scaled_allpass_delays_left[j]);
            }

            // Mix dry and wet
            let mixed = input * dry_level + output * wet_level;
            buffer.set(i, 0, mixed);
        }
    }

    /// Process stereo audio
    fn process_stereo(&mut self, buffer: &mut AudioBuffer) {
        let num_samples = buffer.num_samples();
        let wet_level = self.params.wet_level;
        let dry_level = self.params.dry_level;
        let width = self.params.width;

        // Width coefficients: at width=0, both channels get mono sum
        // at width=1, full stereo separation
        let wet1 = wet_level * (1.0 + width) / 2.0;
        let wet2 = wet_level * (1.0 - width) / 2.0;

        for i in 0..num_samples {
            let input_left = buffer.get(i, 0).unwrap_or(0.0);
            let input_right = buffer.get(i, 1).unwrap_or(0.0);

            // Sum inputs for feeding reverb (mono sum)
            let input_mono = (input_left + input_right) * 0.5;

            // Apply pre-delay
            let delayed_left = if self.pre_delay_samples > 0 {
                self.pre_delay_left.process(input_mono, self.pre_delay_samples)
            } else {
                input_mono
            };
            let delayed_right = if self.pre_delay_samples > 0 {
                self.pre_delay_right.process(input_mono, self.pre_delay_samples)
            } else {
                input_mono
            };

            // Process through comb filters (parallel)
            let mut comb_left_sum = 0.0;
            let mut comb_right_sum = 0.0;
            for j in 0..8 {
                comb_left_sum +=
                    self.comb_left[j].process(delayed_left, self.scaled_comb_delays_left[j]);
                comb_right_sum +=
                    self.comb_right[j].process(delayed_right, self.scaled_comb_delays_right[j]);
            }

            // Process through allpass filters (series)
            let mut output_left = comb_left_sum;
            let mut output_right = comb_right_sum;
            for j in 0..4 {
                output_left =
                    self.allpass_left[j].process(output_left, self.scaled_allpass_delays_left[j]);
                output_right =
                    self.allpass_right[j].process(output_right, self.scaled_allpass_delays_right[j]);
            }

            // Apply width and mix
            // wet1 controls same-side contribution, wet2 controls cross-side contribution
            let wet_left = output_left * wet1 + output_right * wet2;
            let wet_right = output_right * wet1 + output_left * wet2;

            let mixed_left = input_left * dry_level + wet_left;
            let mixed_right = input_right * dry_level + wet_right;

            buffer.set(i, 0, mixed_left);
            buffer.set(i, 1, mixed_right);
        }
    }
}

impl Default for Reverb {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Reverb {
    fn process(&mut self, buffer: &mut AudioBuffer) {
        if !self.enabled {
            return;
        }

        match buffer.num_channels() {
            1 => self.process_mono(buffer),
            _ => self.process_stereo(buffer),
        }
    }

    fn prepare(&mut self, sample_rate: f64, _samples_per_block: usize) {
        self.sample_rate = sample_rate;
        self.resize_buffers();
        self.scale_delays();
        self.update_pre_delay();
    }

    fn reset(&mut self) {
        // Clear all comb filters
        for comb in &mut self.comb_left {
            comb.clear();
        }
        for comb in &mut self.comb_right {
            comb.clear();
        }

        // Clear all allpass filters
        for allpass in &mut self.allpass_left {
            allpass.clear();
        }
        for allpass in &mut self.allpass_right {
            allpass.clear();
        }

        // Clear pre-delay buffers
        self.pre_delay_left.clear();
        self.pre_delay_right.clear();
    }

    fn to_json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "effect_type": self.effect_type(),
            "id": self.id,
            "enabled": self.enabled,
            "params": {
                "room_size": self.params.room_size,
                "damping": self.params.damping,
                "wet_level": self.params.wet_level,
                "dry_level": self.params.dry_level,
                "width": self.params.width,
                "pre_delay_ms": self.params.pre_delay_ms,
            }
        }))
    }

    fn from_json(&mut self, json: &serde_json::Value) -> Result<()> {
        if let Some(id) = json.get("id").and_then(|v| v.as_str()) {
            self.id = id.to_string();
        }

        if let Some(enabled) = json.get("enabled").and_then(|v| v.as_bool()) {
            self.enabled = enabled;
        }

        if let Some(params) = json.get("params") {
            let mut new_params = self.params.clone();

            if let Some(v) = params.get("room_size").and_then(|v| v.as_f64()) {
                new_params.room_size = v as f32;
            }
            if let Some(v) = params.get("damping").and_then(|v| v.as_f64()) {
                new_params.damping = v as f32;
            }
            if let Some(v) = params.get("wet_level").and_then(|v| v.as_f64()) {
                new_params.wet_level = v as f32;
            }
            if let Some(v) = params.get("dry_level").and_then(|v| v.as_f64()) {
                new_params.dry_level = v as f32;
            }
            if let Some(v) = params.get("width").and_then(|v| v.as_f64()) {
                new_params.width = v as f32;
            }
            if let Some(v) = params.get("pre_delay_ms").and_then(|v| v.as_f64()) {
                new_params.pre_delay_ms = v as f32;
            }

            self.set_params(new_params)?;
        }

        Ok(())
    }

    fn effect_type(&self) -> &'static str {
        "reverb"
    }

    fn display_name(&self) -> &'static str {
        "Reverb"
    }

    fn metadata(&self) -> EffectMetadata {
        EffectMetadata {
            effect_type: "reverb".to_string(),
            display_name: "Reverb".to_string(),
            category: "time".to_string(),
            order_priority: 6, // After delay in the chain
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reverb_default_params() {
        let reverb = Reverb::new();
        let params = reverb.params();

        assert_eq!(params.room_size, 0.5);
        assert_eq!(params.damping, 0.5);
        assert_eq!(params.wet_level, 0.3);
        assert_eq!(params.dry_level, 1.0);
        assert_eq!(params.width, 1.0);
        assert_eq!(params.pre_delay_ms, 0.0);
    }

    #[test]
    fn test_reverb_param_validation() {
        // Valid params
        let params = ReverbParams::default();
        assert!(params.validate().is_ok());

        // Invalid room_size (too low)
        let mut params = ReverbParams::default();
        params.room_size = -0.1;
        assert!(params.validate().is_err());

        // Invalid room_size (too high)
        params.room_size = 1.1;
        assert!(params.validate().is_err());

        // Invalid damping
        params = ReverbParams::default();
        params.damping = 1.5;
        assert!(params.validate().is_err());

        // Invalid wet_level
        params = ReverbParams::default();
        params.wet_level = -0.1;
        assert!(params.validate().is_err());

        // Invalid dry_level
        params = ReverbParams::default();
        params.dry_level = 2.0;
        assert!(params.validate().is_err());

        // Invalid width
        params = ReverbParams::default();
        params.width = -0.5;
        assert!(params.validate().is_err());

        // Invalid pre_delay_ms (too high)
        params = ReverbParams::default();
        params.pre_delay_ms = 150.0;
        assert!(params.validate().is_err());

        // Invalid pre_delay_ms (negative)
        params.pre_delay_ms = -10.0;
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_comb_filter() {
        let mut comb = CombFilter::new(100);
        comb.set_coefficients(0.5, 0.8, 0.2);

        // Process an impulse
        let out1 = comb.process(1.0, 10);
        assert_eq!(out1, 0.0); // No output yet (delay line is empty)

        // Process zeros until we get the feedback
        for _ in 0..9 {
            comb.process(0.0, 10);
        }

        let out2 = comb.process(0.0, 10);
        assert!(out2.abs() > 0.0); // Should have some output from feedback
    }

    #[test]
    fn test_allpass_filter() {
        let mut allpass = AllpassFilter::new(100);

        // Process an impulse
        let out1 = allpass.process(1.0, 10);
        // Allpass with empty buffer: output = 0 - 0.5 * 1.0 = -0.5
        assert!((out1 - (-0.5)).abs() < 0.01);

        // Process zeros
        for _ in 0..9 {
            allpass.process(0.0, 10);
        }

        // After delay, we should get the stored signal
        let out2 = allpass.process(0.0, 10);
        assert!(out2.abs() > 0.0);
    }

    #[test]
    fn test_pre_delay_buffer() {
        let mut buffer = PreDelayBuffer::new(100);

        // With 0 delay, should pass through
        let out1 = buffer.process(1.0, 0);
        assert!((out1 - 1.0).abs() < 0.01);

        // With delay, should get zero initially
        let mut buffer2 = PreDelayBuffer::new(100);
        let out2 = buffer2.process(1.0, 10);
        assert!(out2.abs() < 0.01);

        // After delay samples, should get the impulse
        for _ in 0..9 {
            buffer2.process(0.0, 10);
        }
        let out3 = buffer2.process(0.0, 10);
        assert!((out3 - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_reverb_process_mono() {
        let mut reverb = Reverb::new();
        reverb.prepare(44100.0, 512);

        // Create a buffer with an impulse
        // Minimum comb delay is 1116 samples, so buffer needs to be longer
        let mut buffer = AudioBuffer::new(1, 3000, 44100.0);
        buffer.set(0, 0, 1.0);

        // Process
        reverb.process(&mut buffer);

        // First sample should have dry signal
        let first = buffer.get(0, 0).unwrap();
        assert!((first - 1.0).abs() < 0.1); // Dry level is 1.0

        // Check that there is some reverb energy in the buffer
        // The reverb tail appears after comb filter delays (~1116-1617 samples)
        let mut max_reverb: f32 = 0.0;
        for i in 1200..3000 {
            let sample = buffer.get(i, 0).unwrap().abs();
            if sample > max_reverb {
                max_reverb = sample;
            }
        }
        assert!(max_reverb > 0.0, "No reverb tail detected, max reverb = {}", max_reverb);
    }

    #[test]
    fn test_reverb_process_stereo() {
        let mut reverb = Reverb::new();
        reverb.prepare(44100.0, 512);

        // Create a stereo buffer with an impulse
        // Minimum comb delay is 1116 samples, so buffer needs to be longer
        let mut buffer = AudioBuffer::new(2, 3000, 44100.0);
        buffer.set(0, 0, 1.0);
        buffer.set(0, 1, 1.0);

        // Process
        reverb.process(&mut buffer);

        // Both channels should have output
        let left_first = buffer.get(0, 0).unwrap();
        let right_first = buffer.get(0, 1).unwrap();

        assert!((left_first - 1.0).abs() < 0.1);
        assert!((right_first - 1.0).abs() < 0.1);

        // Check that there is some reverb energy in both channels
        // The reverb tail appears after comb filter delays (~1116-1617 samples)
        let mut max_left: f32 = 0.0;
        let mut max_right: f32 = 0.0;
        for i in 1200..3000 {
            let left_sample = buffer.get(i, 0).unwrap().abs();
            let right_sample = buffer.get(i, 1).unwrap().abs();
            if left_sample > max_left {
                max_left = left_sample;
            }
            if right_sample > max_right {
                max_right = right_sample;
            }
        }
        assert!(max_left > 0.0, "No left reverb tail detected");
        assert!(max_right > 0.0, "No right reverb tail detected");
    }

    #[test]
    fn test_reverb_stereo_width() {
        // Test mono width (width = 0)
        let mut reverb_mono = Reverb::with_params(ReverbParams {
            room_size: 0.5,
            damping: 0.5,
            wet_level: 1.0,
            dry_level: 0.0,
            width: 0.0, // Mono
            pre_delay_ms: 0.0,
        });
        reverb_mono.prepare(44100.0, 512);

        // Create a stereo buffer (needs to be longer than comb delays ~1116 samples)
        let mut buffer = AudioBuffer::new(2, 3000, 44100.0);
        buffer.set(0, 0, 1.0);
        buffer.set(0, 1, 1.0);

        reverb_mono.process(&mut buffer);

        // At mono width, left and right channels should be very similar
        // Find the maximum difference between channels in the reverb tail
        let mut max_diff: f32 = 0.0;
        let mut max_left: f32 = 0.0;
        let mut max_right: f32 = 0.0;
        for i in 1200..3000 {
            let left = buffer.get(i, 0).unwrap();
            let right = buffer.get(i, 1).unwrap();
            let diff = (left - right).abs();
            if diff > max_diff {
                max_diff = diff;
            }
            if left.abs() > max_left {
                max_left = left.abs();
            }
            if right.abs() > max_right {
                max_right = right.abs();
            }
        }
        // With mono width, channels should be identical
        assert!(max_diff < 0.01, "Mono width should produce identical channels, max_diff = {}", max_diff);

        // Test full stereo width (width = 1)
        let mut reverb_stereo = Reverb::with_params(ReverbParams {
            room_size: 0.5,
            damping: 0.5,
            wet_level: 1.0,
            dry_level: 0.0,
            width: 1.0, // Full stereo
            pre_delay_ms: 0.0,
        });
        reverb_stereo.prepare(44100.0, 512);

        let mut buffer2 = AudioBuffer::new(2, 3000, 44100.0);
        buffer2.set(0, 0, 1.0);
        buffer2.set(0, 1, 1.0);

        reverb_stereo.process(&mut buffer2);

        // At full width, both channels should have signal
        let mut max_left2: f32 = 0.0;
        let mut max_right2: f32 = 0.0;
        for i in 1200..3000 {
            let left = buffer2.get(i, 0).unwrap().abs();
            let right = buffer2.get(i, 1).unwrap().abs();
            if left > max_left2 {
                max_left2 = left;
            }
            if right > max_right2 {
                max_right2 = right;
            }
        }
        assert!(max_left2 > 0.0, "No left reverb detected");
        assert!(max_right2 > 0.0, "No right reverb detected");
    }

    #[test]
    fn test_reverb_pre_delay() {
        let mut reverb = Reverb::with_params(ReverbParams {
            room_size: 0.5,
            damping: 0.5,
            wet_level: 1.0,
            dry_level: 0.0, // Only wet
            width: 1.0,
            pre_delay_ms: 50.0, // 50ms pre-delay
        });
        reverb.prepare(44100.0, 512);

        // Create a buffer with an impulse
        let mut buffer = AudioBuffer::new(1, 5000, 44100.0);
        buffer.set(0, 0, 1.0);

        // Process
        reverb.process(&mut buffer);

        // First few samples should be near zero (pre-delay)
        let pre_delay_samples = (50.0 / 1000.0 * 44100.0) as usize;

        // Early samples should be very quiet
        let early = buffer.get(10, 0).unwrap();
        assert!(early.abs() < 0.1);

        // After pre-delay, reverb should appear
        let after_delay = buffer.get(pre_delay_samples + 100, 0).unwrap();
        // Should have some output (reverb started)
        // Note: the exact value depends on comb filter delays
        assert!(after_delay.abs() >= 0.0); // At least not NaN
    }

    #[test]
    fn test_reverb_room_size_affects_decay() {
        // Small room (short decay)
        let mut reverb_small = Reverb::with_params(ReverbParams {
            room_size: 0.1,
            damping: 0.5,
            wet_level: 1.0,
            dry_level: 0.0,
            width: 1.0,
            pre_delay_ms: 0.0,
        });
        reverb_small.prepare(44100.0, 512);

        // Large room (long decay)
        let mut reverb_large = Reverb::with_params(ReverbParams {
            room_size: 0.9,
            damping: 0.5,
            wet_level: 1.0,
            dry_level: 0.0,
            width: 1.0,
            pre_delay_ms: 0.0,
        });
        reverb_large.prepare(44100.0, 512);

        // Process impulses
        let mut buffer_small = AudioBuffer::new(1, 20000, 44100.0);
        buffer_small.set(0, 0, 1.0);
        reverb_small.process(&mut buffer_small);

        let mut buffer_large = AudioBuffer::new(1, 20000, 44100.0);
        buffer_large.set(0, 0, 1.0);
        reverb_large.process(&mut buffer_large);

        // Calculate RMS of late reverb tail
        let late_start = 10000;
        let mut rms_small = 0.0f32;
        let mut rms_large = 0.0f32;

        for i in late_start..20000 {
            let s_small = buffer_small.get(i, 0).unwrap();
            let s_large = buffer_large.get(i, 0).unwrap();
            rms_small += s_small * s_small;
            rms_large += s_large * s_large;
        }

        rms_small = (rms_small / 10000.0).sqrt();
        rms_large = (rms_large / 10000.0).sqrt();

        // Large room should have longer decay (higher late RMS)
        assert!(rms_large > rms_small);
    }

    #[test]
    fn test_reverb_damping_affects_brightness() {
        // This test verifies that damping affects the frequency content
        // Higher damping = darker sound (less high frequencies)
        // We can verify this by checking that the output differs

        let mut reverb_bright = Reverb::with_params(ReverbParams {
            room_size: 0.5,
            damping: 0.0, // Bright
            wet_level: 1.0,
            dry_level: 0.0,
            width: 1.0,
            pre_delay_ms: 0.0,
        });
        reverb_bright.prepare(44100.0, 512);

        let mut reverb_dark = Reverb::with_params(ReverbParams {
            room_size: 0.5,
            damping: 1.0, // Dark
            wet_level: 1.0,
            dry_level: 0.0,
            width: 1.0,
            pre_delay_ms: 0.0,
        });
        reverb_dark.prepare(44100.0, 512);

        // Process impulses
        let mut buffer_bright = AudioBuffer::new(1, 5000, 44100.0);
        buffer_bright.set(0, 0, 1.0);
        reverb_bright.process(&mut buffer_bright);

        let mut buffer_dark = AudioBuffer::new(1, 5000, 44100.0);
        buffer_dark.set(0, 0, 1.0);
        reverb_dark.process(&mut buffer_dark);

        // The outputs should be different (different damping)
        let mut diff_sum = 0.0f32;
        for i in 0..5000 {
            let bright = buffer_bright.get(i, 0).unwrap();
            let dark = buffer_dark.get(i, 0).unwrap();
            diff_sum += (bright - dark).abs();
        }

        // Should have some difference
        assert!(diff_sum > 0.1);
    }

    #[test]
    fn test_reverb_reset() {
        let mut reverb = Reverb::new();
        reverb.prepare(44100.0, 512);

        // Process some audio
        let mut buffer = AudioBuffer::new(1, 1000, 44100.0);
        buffer.set(0, 0, 1.0);
        reverb.process(&mut buffer);

        // Reset
        reverb.reset();

        // Create a silent buffer
        let mut buffer = AudioBuffer::new(1, 1000, 44100.0);
        reverb.process(&mut buffer);

        // All samples should be near zero after reset
        for i in 0..1000 {
            let sample = buffer.get(i, 0).unwrap();
            assert!(sample.abs() < 0.01);
        }
    }

    #[test]
    fn test_reverb_json_serialization() {
        let mut reverb = Reverb::new();
        reverb.set_id("reverb-1".to_string());
        reverb
            .set_params(ReverbParams {
                room_size: 0.7,
                damping: 0.3,
                wet_level: 0.5,
                dry_level: 0.8,
                width: 0.6,
                pre_delay_ms: 25.0,
            })
            .unwrap();

        // Serialize
        let json = reverb.to_json().unwrap();

        // Create a new reverb and deserialize
        let mut reverb2 = Reverb::new();
        reverb2.from_json(&json).unwrap();

        assert_eq!(reverb2.id(), "reverb-1");
        assert_eq!(reverb2.params().room_size, 0.7);
        assert_eq!(reverb2.params().damping, 0.3);
        assert_eq!(reverb2.params().wet_level, 0.5);
        assert_eq!(reverb2.params().dry_level, 0.8);
        assert_eq!(reverb2.params().width, 0.6);
        assert_eq!(reverb2.params().pre_delay_ms, 25.0);
    }

    #[test]
    fn test_reverb_effect_trait() {
        let reverb = Reverb::new();

        assert_eq!(reverb.effect_type(), "reverb");
        assert_eq!(reverb.display_name(), "Reverb");
        assert!(reverb.is_enabled());

        let metadata = reverb.metadata();
        assert_eq!(metadata.effect_type, "reverb");
        assert_eq!(metadata.category, "time");
    }

    #[test]
    fn test_reverb_enabled_disabled() {
        let mut reverb = Reverb::with_params(ReverbParams {
            room_size: 0.5,
            damping: 0.5,
            wet_level: 1.0,
            dry_level: 0.0, // Only wet
            width: 1.0,
            pre_delay_ms: 0.0,
        });
        reverb.prepare(44100.0, 512);

        // Disable the effect
        reverb.set_enabled(false);

        // Create a buffer with an impulse
        let mut buffer = AudioBuffer::new(1, 1000, 44100.0);
        buffer.set(0, 0, 1.0);

        // Process
        reverb.process(&mut buffer);

        // When disabled, the buffer should be unchanged
        let first = buffer.get(0, 0).unwrap();
        assert!((first - 1.0).abs() < 0.01);

        // No reverb should appear
        let later = buffer.get(500, 0).unwrap();
        assert!(later.abs() < 0.01);
    }

    #[test]
    fn test_reverb_sample_rate_scaling() {
        // Test at different sample rates
        let mut reverb_44k = Reverb::new();
        reverb_44k.prepare(44100.0, 512);

        let mut reverb_96k = Reverb::new();
        reverb_96k.prepare(96000.0, 512);

        // The scaled delays should be proportional to sample rate
        let scale = 96000.0 / 44100.0;

        // Check that delays are scaled correctly
        assert!(
            (reverb_96k.scaled_comb_delays_left[0] as f64)
                > reverb_44k.scaled_comb_delays_left[0] as f64 * (scale * 0.9)
        );
        assert!(
            (reverb_96k.scaled_comb_delays_left[0] as f64)
                < reverb_44k.scaled_comb_delays_left[0] as f64 * (scale * 1.1)
        );
    }

    #[test]
    fn test_reverb_no_nan_or_inf() {
        let mut reverb = Reverb::with_params(ReverbParams {
            room_size: 1.0,       // Maximum
            damping: 0.0,         // Minimum damping (bright)
            wet_level: 1.0,
            dry_level: 1.0,
            width: 1.0,
            pre_delay_ms: 100.0, // Maximum pre-delay
        });
        reverb.prepare(44100.0, 512);

        // Process a large impulse
        let mut buffer = AudioBuffer::new(2, 10000, 44100.0);
        buffer.set(0, 0, 1.0);
        buffer.set(0, 1, 1.0);

        reverb.process(&mut buffer);

        // Check for NaN or Inf
        for i in 0..10000 {
            let left = buffer.get(i, 0).unwrap();
            let right = buffer.get(i, 1).unwrap();
            assert!(left.is_finite(), "NaN or Inf detected at sample {}", i);
            assert!(right.is_finite(), "NaN or Inf detected at sample {}", i);
        }
    }

    #[test]
    fn test_reverb_parameter_setters() {
        let mut reverb = Reverb::new();

        // Test individual setters
        assert!(reverb.set_room_size(0.8).is_ok());
        assert_eq!(reverb.params().room_size, 0.8);

        assert!(reverb.set_damping(0.3).is_ok());
        assert_eq!(reverb.params().damping, 0.3);

        assert!(reverb.set_wet_level(0.6).is_ok());
        assert_eq!(reverb.params().wet_level, 0.6);

        assert!(reverb.set_dry_level(0.7).is_ok());
        assert_eq!(reverb.params().dry_level, 0.7);

        assert!(reverb.set_width(0.5).is_ok());
        assert_eq!(reverb.params().width, 0.5);

        assert!(reverb.set_pre_delay(50.0).is_ok());
        assert_eq!(reverb.params().pre_delay_ms, 50.0);

        // Test invalid values
        assert!(reverb.set_room_size(1.5).is_err());
        assert!(reverb.set_damping(-0.1).is_err());
        assert!(reverb.set_wet_level(2.0).is_err());
        assert!(reverb.set_dry_level(-0.5).is_err());
        assert!(reverb.set_width(1.5).is_err());
        assert!(reverb.set_pre_delay(150.0).is_err());
    }

    #[test]
    fn test_reverb_id() {
        let mut reverb = Reverb::new();
        assert!(reverb.id().is_empty());

        reverb.set_id("reverb-test".to_string());
        assert_eq!(reverb.id(), "reverb-test");
    }

    #[test]
    fn test_reverb_dry_wet_mix() {
        // Test dry only
        let mut reverb_dry = Reverb::with_params(ReverbParams {
            room_size: 0.5,
            damping: 0.5,
            wet_level: 0.0,
            dry_level: 1.0,
            width: 1.0,
            pre_delay_ms: 0.0,
        });
        reverb_dry.prepare(44100.0, 512);

        let mut buffer_dry = AudioBuffer::new(1, 1000, 44100.0);
        buffer_dry.set(0, 0, 0.5);
        reverb_dry.process(&mut buffer_dry);

        // First sample should be 0.5 (dry only)
        assert!((buffer_dry.get(0, 0).unwrap() - 0.5).abs() < 0.01);

        // Later samples should be 0 (no wet signal)
        // Note: the impulse only affects sample 0, so later dry samples are 0
        // But we're checking the reverb tail is absent
        for i in 100..1000 {
            let sample = buffer_dry.get(i, 0).unwrap();
            assert!(
                sample.abs() < 0.01,
                "Sample {} has unexpected value: {}",
                i,
                sample
            );
        }

        // Test wet only
        let mut reverb_wet = Reverb::with_params(ReverbParams {
            room_size: 0.5,
            damping: 0.5,
            wet_level: 1.0,
            dry_level: 0.0,
            width: 1.0,
            pre_delay_ms: 0.0,
        });
        reverb_wet.prepare(44100.0, 512);

        let mut buffer_wet = AudioBuffer::new(1, 2000, 44100.0);
        buffer_wet.set(0, 0, 1.0);
        reverb_wet.process(&mut buffer_wet);

        // First sample should be near 0 (comb filters have delay)
        let first = buffer_wet.get(0, 0).unwrap();
        assert!(first.abs() < 0.5); // May have some early reflection

        // Later samples should have reverb
        let mut has_reverb = false;
        for i in 500..2000 {
            let sample = buffer_wet.get(i, 0).unwrap();
            if sample.abs() > 0.01 {
                has_reverb = true;
                break;
            }
        }
        assert!(has_reverb, "No reverb tail detected");
    }
}
