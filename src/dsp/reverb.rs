//! Reverb Effect
//!
//! Freeverb-style algorithmic reverb with room size, damping, and stereo width controls.
//! Per spec section 4.2.4.

use crate::dsp::effect::{Effect, EffectParams};
use crate::engine::AudioBuffer;
use crate::error::{NuevaError, Result};
use crate::impl_effect_common;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ============================================================================
// Freeverb Constants
// ============================================================================

/// Reference sample rate for Freeverb delay times (44.1kHz)
const FREEVERB_SAMPLE_RATE: f32 = 44100.0;

/// Comb filter delay times at 44.1kHz (in samples)
const COMB_DELAYS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];

/// Allpass filter delay times at 44.1kHz (in samples)
const ALLPASS_DELAYS: [usize; 4] = [556, 441, 341, 225];

/// Stereo spread (offset for right channel comb filters)
const STEREO_SPREAD: usize = 23;

/// Fixed feedback for allpass filters
const ALLPASS_FEEDBACK: f32 = 0.5;

/// Scaling factors for room size to feedback conversion
const ROOM_SCALE: f32 = 0.28;
const ROOM_OFFSET: f32 = 0.7;

/// Scaling factor for damping
const DAMP_SCALE: f32 = 0.4;

/// Input gain scaling
const INPUT_GAIN: f32 = 0.015;

// ============================================================================
// Comb Filter
// ============================================================================

/// Comb filter with damping for Freeverb
#[derive(Debug, Clone)]
struct CombFilter {
    /// Circular buffer
    buffer: Vec<f32>,
    /// Current position in buffer
    pos: usize,
    /// Feedback amount (derived from room size)
    feedback: f32,
    /// Damping coefficient 1 (derived from damping parameter)
    damp1: f32,
    /// Damping coefficient 2 (1 - damp1)
    damp2: f32,
    /// Filter store for damping low-pass
    filterstore: f32,
}

impl CombFilter {
    /// Create a new comb filter with specified delay size
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            pos: 0,
            feedback: 0.0,
            damp1: 0.0,
            damp2: 1.0,
            filterstore: 0.0,
        }
    }

    /// Set feedback amount
    fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback;
    }

    /// Set damping amount (0-1)
    fn set_damp(&mut self, damp: f32) {
        self.damp1 = damp;
        self.damp2 = 1.0 - damp;
    }

    /// Process a single sample through the comb filter
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.pos];

        // Low-pass filter on feedback path for damping
        self.filterstore = output * self.damp2 + self.filterstore * self.damp1;

        // Write input + filtered feedback to buffer
        self.buffer[self.pos] = input + self.filterstore * self.feedback;

        // Advance position
        self.pos = (self.pos + 1) % self.buffer.len();

        output
    }

    /// Reset the filter state
    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.pos = 0;
        self.filterstore = 0.0;
    }
}

// ============================================================================
// Allpass Filter
// ============================================================================

/// Allpass filter for diffusion in Freeverb
#[derive(Debug, Clone)]
struct AllpassFilter {
    /// Circular buffer
    buffer: Vec<f32>,
    /// Current position in buffer
    pos: usize,
    /// Feedback amount (fixed at 0.5 for Freeverb)
    feedback: f32,
}

impl AllpassFilter {
    /// Create a new allpass filter with specified delay size
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            pos: 0,
            feedback: ALLPASS_FEEDBACK,
        }
    }

    /// Process a single sample through the allpass filter
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let bufout = self.buffer[self.pos];

        // Allpass: output = -input + bufout
        let output = -input + bufout;

        // Write: input + bufout * feedback
        self.buffer[self.pos] = input + bufout * self.feedback;

        // Advance position
        self.pos = (self.pos + 1) % self.buffer.len();

        output
    }

    /// Reset the filter state
    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.pos = 0;
    }
}

// ============================================================================
// Reverb Effect
// ============================================================================

/// Freeverb-style algorithmic reverb
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reverb {
    params: EffectParams,
    /// Room size (0 = tiny, 1 = huge hall)
    room_size: f32,
    /// Damping amount (0 = bright, 1 = dark)
    damping: f32,
    /// Wet signal level (0-1)
    wet_level: f32,
    /// Dry signal level (0-1)
    dry_level: f32,
    /// Stereo width (0 = mono, 1 = full stereo)
    width: f32,
    /// Pre-delay time in milliseconds (0-100)
    pre_delay_ms: f32,
    /// Left channel comb filters (8 parallel)
    #[serde(skip)]
    comb_l: Vec<CombFilter>,
    /// Right channel comb filters (8 parallel)
    #[serde(skip)]
    comb_r: Vec<CombFilter>,
    /// Left channel allpass filters (4 series)
    #[serde(skip)]
    allpass_l: Vec<AllpassFilter>,
    /// Right channel allpass filters (4 series)
    #[serde(skip)]
    allpass_r: Vec<AllpassFilter>,
    /// Pre-delay buffer for left channel
    #[serde(skip)]
    pre_delay_buffer_l: Vec<f32>,
    /// Pre-delay buffer for right channel
    #[serde(skip)]
    pre_delay_buffer_r: Vec<f32>,
    /// Pre-delay write position
    #[serde(skip)]
    pre_delay_pos: usize,
    /// Current sample rate
    #[serde(skip)]
    sample_rate: f32,
}

impl Reverb {
    /// Create a new reverb with sensible defaults
    pub fn new() -> Self {
        let mut reverb = Self {
            params: EffectParams::default(),
            room_size: 0.5,
            damping: 0.5,
            wet_level: 0.3,
            dry_level: 1.0,
            width: 1.0,
            pre_delay_ms: 0.0,
            comb_l: Vec::new(),
            comb_r: Vec::new(),
            allpass_l: Vec::new(),
            allpass_r: Vec::new(),
            pre_delay_buffer_l: Vec::new(),
            pre_delay_buffer_r: Vec::new(),
            pre_delay_pos: 0,
            sample_rate: 48000.0,
        };

        // Initialize filters with default sample rate
        reverb.init_filters(48000.0);

        reverb
    }

    /// Set room size (0-1)
    ///
    /// 0 = very small room, 1 = large hall
    pub fn set_room_size(&mut self, size: f32) {
        self.room_size = size.clamp(0.0, 1.0);
        self.update_comb_feedback();
    }

    /// Get room size
    pub fn room_size(&self) -> f32 {
        self.room_size
    }

    /// Set damping (0-1)
    ///
    /// 0 = bright reverb, 1 = dark reverb
    pub fn set_damping(&mut self, damp: f32) {
        self.damping = damp.clamp(0.0, 1.0);
        self.update_comb_damping();
    }

    /// Get damping
    pub fn damping(&self) -> f32 {
        self.damping
    }

    /// Set wet level (0-1)
    pub fn set_wet_level(&mut self, level: f32) {
        self.wet_level = level.clamp(0.0, 1.0);
    }

    /// Get wet level
    pub fn wet_level(&self) -> f32 {
        self.wet_level
    }

    /// Set dry level (0-1)
    pub fn set_dry_level(&mut self, level: f32) {
        self.dry_level = level.clamp(0.0, 1.0);
    }

    /// Get dry level
    pub fn dry_level(&self) -> f32 {
        self.dry_level
    }

    /// Set stereo width (0-1)
    ///
    /// 0 = mono reverb, 1 = full stereo spread
    pub fn set_width(&mut self, width: f32) {
        self.width = width.clamp(0.0, 1.0);
    }

    /// Get stereo width
    pub fn width(&self) -> f32 {
        self.width
    }

    /// Set pre-delay in milliseconds (0-100)
    pub fn set_pre_delay_ms(&mut self, ms: f32) {
        self.pre_delay_ms = ms.clamp(0.0, 100.0);
        self.resize_pre_delay();
    }

    /// Get pre-delay in milliseconds
    pub fn pre_delay_ms(&self) -> f32 {
        self.pre_delay_ms
    }

    /// Scale delay size from reference sample rate to current sample rate
    fn scale_delay_size(&self, size: usize) -> usize {
        ((size as f32 * self.sample_rate / FREEVERB_SAMPLE_RATE) as usize).max(1)
    }

    /// Initialize all filters for given sample rate
    fn init_filters(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;

        // Initialize comb filters
        self.comb_l.clear();
        self.comb_r.clear();

        for &delay in &COMB_DELAYS {
            let size_l = self.scale_delay_size(delay);
            let size_r = self.scale_delay_size(delay + STEREO_SPREAD);

            self.comb_l.push(CombFilter::new(size_l));
            self.comb_r.push(CombFilter::new(size_r));
        }

        // Initialize allpass filters
        self.allpass_l.clear();
        self.allpass_r.clear();

        for &delay in &ALLPASS_DELAYS {
            let size_l = self.scale_delay_size(delay);
            let size_r = self.scale_delay_size(delay + STEREO_SPREAD);

            self.allpass_l.push(AllpassFilter::new(size_l));
            self.allpass_r.push(AllpassFilter::new(size_r));
        }

        // Initialize pre-delay
        self.resize_pre_delay();

        // Update filter parameters
        self.update_comb_feedback();
        self.update_comb_damping();
    }

    /// Update comb filter feedback based on room size
    fn update_comb_feedback(&mut self) {
        let feedback = self.room_size * ROOM_SCALE + ROOM_OFFSET;

        for comb in &mut self.comb_l {
            comb.set_feedback(feedback);
        }
        for comb in &mut self.comb_r {
            comb.set_feedback(feedback);
        }
    }

    /// Update comb filter damping
    fn update_comb_damping(&mut self) {
        let damp = self.damping * DAMP_SCALE;

        for comb in &mut self.comb_l {
            comb.set_damp(damp);
        }
        for comb in &mut self.comb_r {
            comb.set_damp(damp);
        }
    }

    /// Resize pre-delay buffers
    fn resize_pre_delay(&mut self) {
        let size = ((self.pre_delay_ms * self.sample_rate / 1000.0) as usize).max(1);

        if self.pre_delay_buffer_l.len() != size {
            self.pre_delay_buffer_l.resize(size, 0.0);
            self.pre_delay_buffer_r.resize(size, 0.0);

            if self.pre_delay_pos >= size {
                self.pre_delay_pos = 0;
            }
        }
    }

    /// Apply pre-delay to a sample
    #[inline]
    fn apply_pre_delay(&mut self, input_l: f32, input_r: f32) -> (f32, f32) {
        if self.pre_delay_ms <= 0.0 || self.pre_delay_buffer_l.is_empty() {
            return (input_l, input_r);
        }

        // Read from pre-delay buffer
        let output_l = self.pre_delay_buffer_l[self.pre_delay_pos];
        let output_r = self.pre_delay_buffer_r[self.pre_delay_pos];

        // Write input to buffer
        self.pre_delay_buffer_l[self.pre_delay_pos] = input_l;
        self.pre_delay_buffer_r[self.pre_delay_pos] = input_r;

        // Advance position
        self.pre_delay_pos = (self.pre_delay_pos + 1) % self.pre_delay_buffer_l.len();

        (output_l, output_r)
    }

    /// Process mono audio
    fn process_mono(&mut self, buffer: &mut AudioBuffer) {
        let num_samples = buffer.num_samples();

        for i in 0..num_samples {
            let input = buffer.samples[0][i] * INPUT_GAIN;

            // Apply pre-delay (use same signal for both channels internally)
            let (pre_l, _) = self.apply_pre_delay(input, input);

            // Sum through all comb filters in parallel
            let mut comb_out = 0.0;
            for comb in &mut self.comb_l {
                comb_out += comb.process(pre_l);
            }

            // Pass through allpass filters in series
            let mut allpass_out = comb_out;
            for allpass in &mut self.allpass_l {
                allpass_out = allpass.process(allpass_out);
            }

            // Mix dry and wet
            let dry = buffer.samples[0][i];
            buffer.samples[0][i] = dry * self.dry_level + allpass_out * self.wet_level;
        }
    }

    /// Process stereo audio
    fn process_stereo(&mut self, buffer: &mut AudioBuffer) {
        let num_samples = buffer.num_samples();

        for i in 0..num_samples {
            let input_l = buffer.samples[0][i] * INPUT_GAIN;
            let input_r = buffer.samples[1][i] * INPUT_GAIN;

            // Apply pre-delay
            let (pre_l, pre_r) = self.apply_pre_delay(input_l, input_r);

            // Sum through all comb filters in parallel (each channel)
            let mut comb_out_l = 0.0;
            let mut comb_out_r = 0.0;

            for comb in &mut self.comb_l {
                comb_out_l += comb.process(pre_l);
            }
            for comb in &mut self.comb_r {
                comb_out_r += comb.process(pre_r);
            }

            // Pass through allpass filters in series
            let mut allpass_out_l = comb_out_l;
            let mut allpass_out_r = comb_out_r;

            for allpass in &mut self.allpass_l {
                allpass_out_l = allpass.process(allpass_out_l);
            }
            for allpass in &mut self.allpass_r {
                allpass_out_r = allpass.process(allpass_out_r);
            }

            // Apply stereo width
            // At width=0, both channels get the average (mono)
            // At width=1, full stereo separation
            let wet_l = allpass_out_l;
            let wet_r = allpass_out_r;

            let width1 = self.width;
            let width2 = 1.0 - self.width;

            let wet_out_l = wet_l * width1 + wet_r * width2;
            let wet_out_r = wet_r * width1 + wet_l * width2;

            // Mix dry and wet
            let dry_l = buffer.samples[0][i];
            let dry_r = buffer.samples[1][i];

            buffer.samples[0][i] = dry_l * self.dry_level + wet_out_l * self.wet_level;
            buffer.samples[1][i] = dry_r * self.dry_level + wet_out_r * self.wet_level;
        }
    }
}

impl Default for Reverb {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Reverb {
    impl_effect_common!(Reverb, "reverb", "Reverb");

    fn process(&mut self, buffer: &mut AudioBuffer) {
        if !self.params.enabled || buffer.is_empty() {
            return;
        }

        let num_channels = buffer.num_channels();

        match num_channels {
            1 => self.process_mono(buffer),
            _ => self.process_stereo(buffer),
        }
    }

    fn prepare(&mut self, sample_rate: u32, _max_block_size: usize) {
        if (self.sample_rate - sample_rate as f32).abs() > 1.0 {
            self.init_filters(sample_rate as f32);
        }
    }

    fn reset(&mut self) {
        for comb in &mut self.comb_l {
            comb.reset();
        }
        for comb in &mut self.comb_r {
            comb.reset();
        }
        for allpass in &mut self.allpass_l {
            allpass.reset();
        }
        for allpass in &mut self.allpass_r {
            allpass.reset();
        }

        self.pre_delay_buffer_l.fill(0.0);
        self.pre_delay_buffer_r.fill(0.0);
        self.pre_delay_pos = 0;
    }

    fn to_json(&self) -> Result<Value> {
        serde_json::to_value(self).map_err(NuevaError::Serialization)
    }

    fn from_json(&mut self, json: &Value) -> Result<()> {
        let parsed: Reverb =
            serde_json::from_value(json.clone()).map_err(NuevaError::Serialization)?;

        self.params = parsed.params;
        self.room_size = parsed.room_size;
        self.damping = parsed.damping;
        self.wet_level = parsed.wet_level;
        self.dry_level = parsed.dry_level;
        self.width = parsed.width;
        self.pre_delay_ms = parsed.pre_delay_ms;

        // Re-initialize filters and update parameters
        self.init_filters(self.sample_rate);

        Ok(())
    }

    fn get_params(&self) -> Value {
        json!({
            "id": self.params.id,
            "enabled": self.params.enabled,
            "room_size": self.room_size,
            "damping": self.damping,
            "wet_level": self.wet_level,
            "dry_level": self.dry_level,
            "width": self.width,
            "pre_delay_ms": self.pre_delay_ms
        })
    }

    fn set_param(&mut self, name: &str, value: &Value) -> Result<()> {
        match name {
            "room_size" => {
                if let Some(v) = value.as_f64() {
                    self.set_room_size(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!("Invalid value for room_size: {:?}", value),
                    })
                }
            }
            "damping" => {
                if let Some(v) = value.as_f64() {
                    self.set_damping(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!("Invalid value for damping: {:?}", value),
                    })
                }
            }
            "wet_level" => {
                if let Some(v) = value.as_f64() {
                    self.set_wet_level(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!("Invalid value for wet_level: {:?}", value),
                    })
                }
            }
            "dry_level" => {
                if let Some(v) = value.as_f64() {
                    self.set_dry_level(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!("Invalid value for dry_level: {:?}", value),
                    })
                }
            }
            "width" => {
                if let Some(v) = value.as_f64() {
                    self.set_width(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!("Invalid value for width: {:?}", value),
                    })
                }
            }
            "pre_delay_ms" => {
                if let Some(v) = value.as_f64() {
                    self.set_pre_delay_ms(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!("Invalid value for pre_delay_ms: {:?}", value),
                    })
                }
            }
            "enabled" => {
                if let Some(v) = value.as_bool() {
                    self.set_enabled(v);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!("Invalid value for enabled: {:?}", value),
                    })
                }
            }
            _ => Err(NuevaError::ProcessingError {
                reason: format!("Unknown parameter: {}", name),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_buffer(samples: Vec<Vec<f32>>, sample_rate: u32) -> AudioBuffer {
        AudioBuffer {
            samples,
            sample_rate,
        }
    }

    #[test]
    fn test_reverb_new() {
        let reverb = Reverb::new();
        assert_eq!(reverb.room_size(), 0.5);
        assert_eq!(reverb.damping(), 0.5);
        assert_eq!(reverb.wet_level(), 0.3);
        assert_eq!(reverb.dry_level(), 1.0);
        assert_eq!(reverb.width(), 1.0);
        assert_eq!(reverb.pre_delay_ms(), 0.0);
    }

    #[test]
    fn test_reverb_default() {
        let reverb = Reverb::default();
        assert_eq!(reverb.room_size(), 0.5);
    }

    #[test]
    fn test_reverb_room_size_clamp() {
        let mut reverb = Reverb::new();

        reverb.set_room_size(-0.5);
        assert_eq!(reverb.room_size(), 0.0);

        reverb.set_room_size(1.5);
        assert_eq!(reverb.room_size(), 1.0);
    }

    #[test]
    fn test_reverb_damping_clamp() {
        let mut reverb = Reverb::new();

        reverb.set_damping(-0.5);
        assert_eq!(reverb.damping(), 0.0);

        reverb.set_damping(1.5);
        assert_eq!(reverb.damping(), 1.0);
    }

    #[test]
    fn test_reverb_pre_delay_clamp() {
        let mut reverb = Reverb::new();

        reverb.set_pre_delay_ms(-10.0);
        assert_eq!(reverb.pre_delay_ms(), 0.0);

        reverb.set_pre_delay_ms(200.0);
        assert_eq!(reverb.pre_delay_ms(), 100.0);
    }

    #[test]
    fn test_reverb_filters_initialized() {
        let reverb = Reverb::new();

        // Should have 8 comb filters per channel
        assert_eq!(reverb.comb_l.len(), 8);
        assert_eq!(reverb.comb_r.len(), 8);

        // Should have 4 allpass filters per channel
        assert_eq!(reverb.allpass_l.len(), 4);
        assert_eq!(reverb.allpass_r.len(), 4);
    }

    #[test]
    fn test_reverb_prepare() {
        let mut reverb = Reverb::new();

        // Change sample rate
        reverb.prepare(96000, 512);

        // Filters should be reinitialized with new sample rate
        assert_eq!(reverb.sample_rate, 96000.0);

        // Comb filter sizes should be scaled
        // Original 1116 samples at 44.1kHz -> ~2424 samples at 96kHz
        let expected_size = ((1116.0 * 96000.0 / 44100.0) as usize).max(1);
        assert_eq!(reverb.comb_l[0].buffer.len(), expected_size);
    }

    #[test]
    fn test_reverb_reset() {
        let mut reverb = Reverb::new();
        reverb.prepare(48000, 512);

        // Fill filters with some data by processing
        let mut buffer = create_test_buffer(vec![vec![0.5; 1000]], 48000);
        reverb.process(&mut buffer);

        // Reset
        reverb.reset();

        // All comb filter states should be zero
        for comb in &reverb.comb_l {
            assert!(comb.buffer.iter().all(|&x| x == 0.0));
            assert_eq!(comb.filterstore, 0.0);
        }

        // All allpass filter states should be zero
        for allpass in &reverb.allpass_l {
            assert!(allpass.buffer.iter().all(|&x| x == 0.0));
        }
    }

    #[test]
    fn test_reverb_dry_passthrough() {
        let mut reverb = Reverb::new();
        reverb.prepare(48000, 512);
        reverb.set_wet_level(0.0);
        reverb.set_dry_level(1.0);

        let original = vec![0.5; 100];
        let mut buffer = create_test_buffer(vec![original.clone()], 48000);

        reverb.process(&mut buffer);

        // Dry signal should be unchanged
        for (orig, processed) in original.iter().zip(buffer.samples[0].iter()) {
            assert!((orig - processed).abs() < 0.0001);
        }
    }

    #[test]
    fn test_reverb_produces_tail() {
        let mut reverb = Reverb::new();
        reverb.prepare(48000, 512);
        reverb.set_wet_level(1.0);
        reverb.set_dry_level(0.0);
        reverb.set_room_size(0.8);

        // Create an impulse
        let mut samples = vec![0.0; 10000];
        samples[0] = 1.0;
        let mut buffer = create_test_buffer(vec![samples], 48000);

        reverb.process(&mut buffer);

        // Reverb should produce a tail (non-zero samples after the impulse)
        // Check that there's significant energy in the tail
        let tail_energy: f32 = buffer.samples[0][1000..5000].iter().map(|x| x.abs()).sum();

        assert!(tail_energy > 0.01, "Reverb should produce a tail");
    }

    #[test]
    fn test_reverb_stereo() {
        let mut reverb = Reverb::new();
        reverb.prepare(48000, 512);
        reverb.set_width(1.0);

        let mut buffer = create_test_buffer(vec![vec![0.5; 1000], vec![0.3; 1000]], 48000);

        reverb.process(&mut buffer);

        // Both channels should be processed
        assert_eq!(buffer.samples.len(), 2);
    }

    #[test]
    fn test_reverb_width() {
        let mut reverb = Reverb::new();
        reverb.prepare(48000, 512);
        reverb.set_width(1.0); // Full stereo
        reverb.set_wet_level(1.0);
        reverb.set_dry_level(0.0);

        // Create impulse in left channel only
        let mut left = vec![0.0; 5000];
        let right = vec![0.0; 5000];
        left[0] = 1.0;

        let mut buffer = create_test_buffer(vec![left, right], 48000);

        reverb.process(&mut buffer);

        // With width=1 (full stereo), left should have more energy than right
        // since input was only in left channel
        let l_energy: f32 = buffer.samples[0][100..2000].iter().map(|x| x.abs()).sum();
        let r_energy: f32 = buffer.samples[1][100..2000].iter().map(|x| x.abs()).sum();

        // Left should have significantly more energy
        assert!(
            l_energy > r_energy * 0.5,
            "Left should have significant energy"
        );
        assert!(l_energy > 0.001, "Left channel should have reverb tail");
    }

    #[test]
    fn test_reverb_serialization() {
        let mut reverb = Reverb::new();
        reverb.set_room_size(0.8);
        reverb.set_damping(0.3);
        reverb.set_wet_level(0.5);
        reverb.set_pre_delay_ms(20.0);

        let json = reverb.to_json().unwrap();

        let mut reverb2 = Reverb::new();
        reverb2.from_json(&json).unwrap();

        assert_eq!(reverb2.room_size(), 0.8);
        assert_eq!(reverb2.damping(), 0.3);
        assert_eq!(reverb2.wet_level(), 0.5);
        assert_eq!(reverb2.pre_delay_ms(), 20.0);
    }

    #[test]
    fn test_reverb_get_params() {
        let mut reverb = Reverb::new();
        reverb.set_room_size(0.7);

        let params = reverb.get_params();

        // Use approximate comparison for f32->f64 conversion
        assert!((params["room_size"].as_f64().unwrap() - 0.7).abs() < 0.001);
        assert!((params["damping"].as_f64().unwrap() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_reverb_set_param() {
        let mut reverb = Reverb::new();

        reverb.set_param("room_size", &json!(0.9)).unwrap();
        assert_eq!(reverb.room_size(), 0.9);

        reverb.set_param("damping", &json!(0.7)).unwrap();
        assert_eq!(reverb.damping(), 0.7);

        reverb.set_param("width", &json!(0.5)).unwrap();
        assert_eq!(reverb.width(), 0.5);
    }

    #[test]
    fn test_reverb_set_param_invalid() {
        let mut reverb = Reverb::new();

        let result = reverb.set_param("unknown_param", &json!(1.0));
        assert!(result.is_err());

        let result = reverb.set_param("room_size", &json!("not a number"));
        assert!(result.is_err());
    }

    #[test]
    fn test_reverb_effect_type() {
        let reverb = Reverb::new();
        assert_eq!(reverb.effect_type(), "reverb");
        assert_eq!(reverb.display_name(), "Reverb");
    }

    #[test]
    fn test_reverb_enabled() {
        let mut reverb = Reverb::new();
        reverb.prepare(48000, 512);
        reverb.set_enabled(false);

        let original = vec![0.5; 100];
        let mut buffer = create_test_buffer(vec![original.clone()], 48000);

        reverb.process(&mut buffer);

        // When disabled, buffer should be unchanged
        assert_eq!(buffer.samples[0], original);
    }

    #[test]
    fn test_comb_filter() {
        let mut comb = CombFilter::new(10);
        comb.set_feedback(0.5);
        comb.set_damp(0.0);

        // Process an impulse
        let out0 = comb.process(1.0);
        assert_eq!(out0, 0.0); // First output is from empty buffer

        // Process zeros
        for _ in 0..9 {
            comb.process(0.0);
        }

        // After delay time, should see the impulse + feedback
        let out10 = comb.process(0.0);
        assert!((out10 - 1.0).abs() < 0.001); // Echo of original impulse
    }

    #[test]
    fn test_allpass_filter() {
        let mut allpass = AllpassFilter::new(10);

        // Process an impulse
        let out0 = allpass.process(1.0);
        // Allpass initial output: -input + 0 = -1.0
        assert!((out0 - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_reverb_pre_delay() {
        let mut reverb = Reverb::new();
        reverb.prepare(48000, 512);
        reverb.set_pre_delay_ms(10.0);
        reverb.set_wet_level(1.0);
        reverb.set_dry_level(0.0);

        let pre_delay_samples = (10.0 * 48000.0 / 1000.0) as usize;

        // Create impulse
        let mut samples = vec![0.0; 5000];
        samples[0] = 1.0;
        let mut buffer = create_test_buffer(vec![samples], 48000);

        reverb.process(&mut buffer);

        // Output should be delayed by pre_delay_samples
        // Initial samples should be near zero
        let initial_energy: f32 = buffer.samples[0][0..pre_delay_samples]
            .iter()
            .map(|x| x.abs())
            .sum();

        // There should be less energy in the pre-delay period
        assert!(
            initial_energy < 0.1,
            "Pre-delay should delay the reverb tail"
        );
    }
}
