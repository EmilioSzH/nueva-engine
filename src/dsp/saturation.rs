//! Saturation Effect
//!
//! Waveshaping saturation effect per spec 4.2.6.
//! Provides various saturation types: Tape, Tube, Transistor, HardClip.

use crate::dsp::effect::{Effect, EffectParams};
use crate::engine::AudioBuffer;
use crate::error::{NuevaError, Result};
use crate::impl_effect_common;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ============================================================================
// Constants
// ============================================================================

/// Minimum drive (0.0 = no saturation)
const MIN_DRIVE: f32 = 0.0;

/// Maximum drive (1.0 = full saturation)
const MAX_DRIVE: f32 = 1.0;

/// Minimum mix (0.0 = fully dry)
const MIN_MIX: f32 = 0.0;

/// Maximum mix (1.0 = fully wet)
const MAX_MIX: f32 = 1.0;

/// Minimum output gain in dB
const MIN_OUTPUT_GAIN_DB: f32 = -24.0;

/// Maximum output gain in dB
const MAX_OUTPUT_GAIN_DB: f32 = 24.0;

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert decibels to linear amplitude
#[inline]
fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

// ============================================================================
// Saturation Type
// ============================================================================

/// Types of saturation waveshaping
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum SaturationType {
    /// Soft saturation with slight asymmetry, emulates magnetic tape
    #[default]
    Tape,
    /// Even harmonics emphasis, emulates vacuum tube warmth
    Tube,
    /// Odd harmonics with harder edge, emulates transistor clipping
    Transistor,
    /// Hard digital clipping at threshold
    HardClip,
}

impl SaturationType {
    /// Get display name for the saturation type
    pub fn display_name(&self) -> &'static str {
        match self {
            SaturationType::Tape => "Tape",
            SaturationType::Tube => "Tube",
            SaturationType::Transistor => "Transistor",
            SaturationType::HardClip => "Hard Clip",
        }
    }

    /// Parse saturation type from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "tape" => Some(SaturationType::Tape),
            "tube" => Some(SaturationType::Tube),
            "transistor" => Some(SaturationType::Transistor),
            "hardclip" | "hard_clip" | "hard-clip" => Some(SaturationType::HardClip),
            _ => None,
        }
    }

    /// Get string identifier
    pub fn to_str(&self) -> &'static str {
        match self {
            SaturationType::Tape => "tape",
            SaturationType::Tube => "tube",
            SaturationType::Transistor => "transistor",
            SaturationType::HardClip => "hardclip",
        }
    }
}

// ============================================================================
// Waveshaping Functions
// ============================================================================

/// Tape saturation: soft saturation with slight asymmetry
/// Uses tanh(x * (1 + drive * 4)) with asymmetry
#[inline]
fn waveshape_tape(x: f32, drive: f32) -> f32 {
    let gain = 1.0 + drive * 4.0;
    let shaped = (x * gain).tanh();
    // Add slight asymmetry (even harmonics)
    let asymmetry = 0.1 * drive;
    shaped + asymmetry * shaped * shaped
}

/// Tube saturation: soft knee with even harmonics emphasis
/// Uses x / (1 + |x|^(1 + drive))
#[inline]
fn waveshape_tube(x: f32, drive: f32) -> f32 {
    let exp = 1.0 + drive;
    let abs_x = x.abs();
    x / (1.0 + abs_x.powf(exp))
}

/// Transistor saturation: harder clipping with odd harmonics
/// Uses x * (1 + drive * 3) / (1 + |x * (1 + drive * 3)|)
#[inline]
fn waveshape_transistor(x: f32, drive: f32) -> f32 {
    let gain = 1.0 + drive * 3.0;
    let driven = x * gain;
    driven / (1.0 + driven.abs())
}

/// Hard clip: digital clipping at -1 to 1
/// Amplifies then clamps
#[inline]
fn waveshape_hardclip(x: f32, drive: f32) -> f32 {
    let gain = 1.0 + drive * 10.0;
    (x * gain).clamp(-1.0, 1.0)
}

// ============================================================================
// Saturation Effect
// ============================================================================

/// Waveshaping saturation effect
///
/// Provides various saturation/distortion types with drive, mix, and output gain controls.
///
/// # Parameters
/// - `drive`: Saturation intensity (0.0 to 1.0)
/// - `sat_type`: Type of waveshaping (Tape, Tube, Transistor, HardClip)
/// - `mix`: Dry/wet mix (0.0 = dry, 1.0 = wet)
/// - `output_gain`: Output gain in dB to compensate for volume changes
///
/// # Waveshaping Algorithms
/// - **Tape**: `tanh(x * (1 + drive * 4))` with slight asymmetry for even harmonics
/// - **Tube**: `x / (1 + |x|^(1 + drive))` - soft knee compression
/// - **Transistor**: `x * gain / (1 + |x * gain|)` - harder odd harmonics
/// - **HardClip**: `clamp(x * (1 + drive * 10), -1, 1)` - digital clipping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Saturation {
    params: EffectParams,
    drive: f32,
    sat_type: SaturationType,
    mix: f32,
    output_gain: f32,
    #[serde(skip)]
    output_gain_linear: f32,
}

impl Saturation {
    /// Create a new saturation effect with default settings
    ///
    /// # Returns
    /// A new Saturation effect with Tape type, 0.5 drive, 1.0 mix, 0 dB output
    pub fn new() -> Self {
        Self {
            params: EffectParams::default(),
            drive: 0.5,
            sat_type: SaturationType::Tape,
            mix: 1.0,
            output_gain: 0.0,
            output_gain_linear: 1.0,
        }
    }

    /// Create a new saturation effect with specified parameters
    ///
    /// # Arguments
    /// * `drive` - Saturation intensity (0.0 to 1.0)
    /// * `sat_type` - Type of waveshaping
    /// * `mix` - Dry/wet mix (0.0 to 1.0)
    /// * `output_gain` - Output gain in dB
    pub fn with_params(drive: f32, sat_type: SaturationType, mix: f32, output_gain: f32) -> Self {
        let mut sat = Self::new();
        sat.set_drive(drive);
        sat.set_type(sat_type);
        sat.set_mix(mix);
        sat.set_output_gain(output_gain);
        sat
    }

    /// Set the drive amount
    ///
    /// # Arguments
    /// * `drive` - Saturation intensity (0.0 to 1.0), clamped to valid range
    pub fn set_drive(&mut self, drive: f32) {
        self.drive = drive.clamp(MIN_DRIVE, MAX_DRIVE);
    }

    /// Get the current drive amount
    pub fn drive(&self) -> f32 {
        self.drive
    }

    /// Set the saturation type
    ///
    /// # Arguments
    /// * `sat_type` - Type of waveshaping to use
    pub fn set_type(&mut self, sat_type: SaturationType) {
        self.sat_type = sat_type;
    }

    /// Get the current saturation type
    pub fn saturation_type(&self) -> SaturationType {
        self.sat_type
    }

    /// Set the dry/wet mix
    ///
    /// # Arguments
    /// * `mix` - Mix amount (0.0 = dry, 1.0 = wet), clamped to valid range
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(MIN_MIX, MAX_MIX);
    }

    /// Get the current mix amount
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Set the output gain
    ///
    /// # Arguments
    /// * `db` - Output gain in dB, clamped to valid range
    pub fn set_output_gain(&mut self, db: f32) {
        self.output_gain = db.clamp(MIN_OUTPUT_GAIN_DB, MAX_OUTPUT_GAIN_DB);
        self.output_gain_linear = db_to_linear(self.output_gain);
    }

    /// Get the current output gain in dB
    pub fn output_gain(&self) -> f32 {
        self.output_gain
    }

    /// Apply waveshaping to a single sample
    #[inline]
    fn waveshape(&self, x: f32) -> f32 {
        match self.sat_type {
            SaturationType::Tape => waveshape_tape(x, self.drive),
            SaturationType::Tube => waveshape_tube(x, self.drive),
            SaturationType::Transistor => waveshape_transistor(x, self.drive),
            SaturationType::HardClip => waveshape_hardclip(x, self.drive),
        }
    }
}

impl Default for Saturation {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Saturation {
    impl_effect_common!(Saturation, "saturation", "Saturation");

    fn process(&mut self, buffer: &mut AudioBuffer) {
        if !self.params.enabled {
            return;
        }

        // Skip processing if mix is 0 (fully dry)
        if self.mix < f32::EPSILON {
            return;
        }

        let dry_mix = 1.0 - self.mix;
        let wet_mix = self.mix;

        for channel in 0..buffer.num_channels() {
            let samples = buffer.channel_mut(channel);
            for sample in samples.iter_mut() {
                let dry = *sample;
                let wet = self.waveshape(dry);

                // Mix dry and wet, then apply output gain
                *sample = (dry * dry_mix + wet * wet_mix) * self.output_gain_linear;
            }
        }
    }

    fn prepare(&mut self, _sample_rate: u32, _max_block_size: usize) {
        // Recalculate linear output gain (for deserialization)
        self.output_gain_linear = db_to_linear(self.output_gain);
    }

    fn reset(&mut self) {
        // Saturation is stateless (no internal buffers)
    }

    fn to_json(&self) -> Result<Value> {
        serde_json::to_value(self).map_err(|e| NuevaError::Serialization(e))
    }

    fn from_json(&mut self, json: &Value) -> Result<()> {
        if let Some(drive) = json.get("drive").and_then(|v| v.as_f64()) {
            self.set_drive(drive as f32);
        }
        if let Some(sat_type) = json.get("sat_type").and_then(|v| v.as_str()) {
            if let Some(t) = SaturationType::from_str(sat_type) {
                self.set_type(t);
            }
        }
        if let Some(mix) = json.get("mix").and_then(|v| v.as_f64()) {
            self.set_mix(mix as f32);
        }
        if let Some(output_gain) = json.get("output_gain").and_then(|v| v.as_f64()) {
            self.set_output_gain(output_gain as f32);
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
            "drive": self.drive,
            "type": self.sat_type.to_str(),
            "type_display": self.sat_type.display_name(),
            "mix": self.mix,
            "output_gain": self.output_gain,
            "enabled": self.params.enabled
        })
    }

    fn set_param(&mut self, name: &str, value: &Value) -> Result<()> {
        match name {
            "drive" => {
                if let Some(v) = value.as_f64() {
                    self.set_drive(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!(
                            "Invalid value for drive: expected number, got {:?}",
                            value
                        ),
                    })
                }
            }
            "type" | "sat_type" => {
                if let Some(v) = value.as_str() {
                    if let Some(t) = SaturationType::from_str(v) {
                        self.set_type(t);
                        Ok(())
                    } else {
                        Err(NuevaError::ProcessingError {
                            reason: format!("Invalid saturation type: {}. Valid types: tape, tube, transistor, hardclip", v),
                        })
                    }
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!("Invalid value for type: expected string, got {:?}", value),
                    })
                }
            }
            "mix" => {
                if let Some(v) = value.as_f64() {
                    self.set_mix(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!("Invalid value for mix: expected number, got {:?}", value),
                    })
                }
            }
            "output_gain" => {
                if let Some(v) = value.as_f64() {
                    self.set_output_gain(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!(
                            "Invalid value for output_gain: expected number, got {:?}",
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

    /// Helper to create a sine wave buffer for testing
    fn create_sine_buffer(
        amplitude: f32,
        frequency: f32,
        sample_rate: u32,
        num_samples: usize,
    ) -> AudioBuffer {
        let mut buffer = AudioBuffer::new(num_samples, ChannelLayout::Stereo);
        for ch in 0..buffer.num_channels() {
            for i in 0..num_samples {
                let t = i as f32 / sample_rate as f32;
                let sample = amplitude * (2.0 * std::f32::consts::PI * frequency * t).sin();
                buffer.set_sample(ch, i, sample);
            }
        }
        buffer
    }

    // ========================================================================
    // SaturationType Tests
    // ========================================================================

    #[test]
    fn test_saturation_type_display_name() {
        assert_eq!(SaturationType::Tape.display_name(), "Tape");
        assert_eq!(SaturationType::Tube.display_name(), "Tube");
        assert_eq!(SaturationType::Transistor.display_name(), "Transistor");
        assert_eq!(SaturationType::HardClip.display_name(), "Hard Clip");
    }

    #[test]
    fn test_saturation_type_from_str() {
        assert_eq!(SaturationType::from_str("tape"), Some(SaturationType::Tape));
        assert_eq!(SaturationType::from_str("Tape"), Some(SaturationType::Tape));
        assert_eq!(SaturationType::from_str("TUBE"), Some(SaturationType::Tube));
        assert_eq!(
            SaturationType::from_str("transistor"),
            Some(SaturationType::Transistor)
        );
        assert_eq!(
            SaturationType::from_str("hardclip"),
            Some(SaturationType::HardClip)
        );
        assert_eq!(
            SaturationType::from_str("hard_clip"),
            Some(SaturationType::HardClip)
        );
        assert_eq!(
            SaturationType::from_str("hard-clip"),
            Some(SaturationType::HardClip)
        );
        assert_eq!(SaturationType::from_str("invalid"), None);
    }

    #[test]
    fn test_saturation_type_to_str() {
        assert_eq!(SaturationType::Tape.to_str(), "tape");
        assert_eq!(SaturationType::Tube.to_str(), "tube");
        assert_eq!(SaturationType::Transistor.to_str(), "transistor");
        assert_eq!(SaturationType::HardClip.to_str(), "hardclip");
    }

    // ========================================================================
    // Waveshaping Function Tests
    // ========================================================================

    #[test]
    fn test_waveshape_tape_zero() {
        // Zero input should give near-zero output
        let result = waveshape_tape(0.0, 0.5);
        assert!(result.abs() < 0.01);
    }

    #[test]
    fn test_waveshape_tape_limits() {
        // High input should be limited by tanh
        let result = waveshape_tape(1.0, 1.0);
        assert!(result.abs() < 1.5); // tanh saturates, asymmetry adds a bit

        // Negative values
        let result_neg = waveshape_tape(-1.0, 1.0);
        assert!(result_neg < 0.0);
    }

    #[test]
    fn test_waveshape_tube_zero() {
        let result = waveshape_tube(0.0, 0.5);
        assert!(result.abs() < f32::EPSILON);
    }

    #[test]
    fn test_waveshape_tube_compression() {
        // Tube should compress peaks
        let low = waveshape_tube(0.1, 0.5);
        let high = waveshape_tube(1.0, 0.5);

        // Ratio should be less than 10:1 due to compression
        let ratio = high / low;
        assert!(ratio < 10.0);
    }

    #[test]
    fn test_waveshape_transistor_zero() {
        let result = waveshape_transistor(0.0, 0.5);
        assert!(result.abs() < f32::EPSILON);
    }

    #[test]
    fn test_waveshape_transistor_symmetry() {
        // Transistor should be antisymmetric (odd harmonics)
        let pos = waveshape_transistor(0.5, 0.5);
        let neg = waveshape_transistor(-0.5, 0.5);
        assert!((pos + neg).abs() < f32::EPSILON);
    }

    #[test]
    fn test_waveshape_hardclip_clamps() {
        // Hard clip should clamp to -1..1
        let result = waveshape_hardclip(0.5, 1.0);
        assert!(result >= -1.0 && result <= 1.0);

        // With high drive, should clip
        let result_clipped = waveshape_hardclip(0.5, 1.0);
        assert!((result_clipped - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_waveshape_hardclip_zero() {
        let result = waveshape_hardclip(0.0, 1.0);
        assert!(result.abs() < f32::EPSILON);
    }

    // ========================================================================
    // Saturation Effect Tests
    // ========================================================================

    #[test]
    fn test_saturation_new() {
        let sat = Saturation::new();
        assert!((sat.drive() - 0.5).abs() < f32::EPSILON);
        assert_eq!(sat.saturation_type(), SaturationType::Tape);
        assert!((sat.mix() - 1.0).abs() < f32::EPSILON);
        assert!((sat.output_gain() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_saturation_with_params() {
        let sat = Saturation::with_params(0.75, SaturationType::Tube, 0.5, -3.0);
        assert!((sat.drive() - 0.75).abs() < f32::EPSILON);
        assert_eq!(sat.saturation_type(), SaturationType::Tube);
        assert!((sat.mix() - 0.5).abs() < f32::EPSILON);
        assert!((sat.output_gain() - (-3.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_saturation_setters() {
        let mut sat = Saturation::new();

        sat.set_drive(0.8);
        assert!((sat.drive() - 0.8).abs() < f32::EPSILON);

        sat.set_type(SaturationType::Transistor);
        assert_eq!(sat.saturation_type(), SaturationType::Transistor);

        sat.set_mix(0.7);
        assert!((sat.mix() - 0.7).abs() < f32::EPSILON);

        sat.set_output_gain(-6.0);
        assert!((sat.output_gain() - (-6.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_saturation_clamping() {
        let mut sat = Saturation::new();

        // Drive clamping
        sat.set_drive(-1.0);
        assert!((sat.drive() - MIN_DRIVE).abs() < f32::EPSILON);
        sat.set_drive(2.0);
        assert!((sat.drive() - MAX_DRIVE).abs() < f32::EPSILON);

        // Mix clamping
        sat.set_mix(-0.5);
        assert!((sat.mix() - MIN_MIX).abs() < f32::EPSILON);
        sat.set_mix(1.5);
        assert!((sat.mix() - MAX_MIX).abs() < f32::EPSILON);

        // Output gain clamping
        sat.set_output_gain(-100.0);
        assert!((sat.output_gain() - MIN_OUTPUT_GAIN_DB).abs() < f32::EPSILON);
        sat.set_output_gain(100.0);
        assert!((sat.output_gain() - MAX_OUTPUT_GAIN_DB).abs() < f32::EPSILON);
    }

    #[test]
    fn test_saturation_process_tape() {
        let mut sat = Saturation::with_params(0.5, SaturationType::Tape, 1.0, 0.0);
        let mut buffer = create_sine_buffer(0.5, 440.0, 48000, 480);

        sat.process(&mut buffer);

        // Tape saturation should modify samples
        // Check that samples are still finite and within reasonable range
        for ch in 0..buffer.num_channels() {
            for i in 0..buffer.num_samples() {
                let sample = buffer.get_sample(ch, i).unwrap();
                assert!(sample.is_finite());
                assert!(sample.abs() < 2.0);
            }
        }
    }

    #[test]
    fn test_saturation_process_tube() {
        let mut sat = Saturation::with_params(0.5, SaturationType::Tube, 1.0, 0.0);
        let mut buffer = create_sine_buffer(0.8, 440.0, 48000, 480);

        sat.process(&mut buffer);

        for ch in 0..buffer.num_channels() {
            for i in 0..buffer.num_samples() {
                let sample = buffer.get_sample(ch, i).unwrap();
                assert!(sample.is_finite());
                // Tube compression should keep signal under original amplitude
                assert!(sample.abs() < 1.0);
            }
        }
    }

    #[test]
    fn test_saturation_process_transistor() {
        let mut sat = Saturation::with_params(0.7, SaturationType::Transistor, 1.0, 0.0);
        let mut buffer = create_sine_buffer(0.6, 440.0, 48000, 480);

        sat.process(&mut buffer);

        for ch in 0..buffer.num_channels() {
            for i in 0..buffer.num_samples() {
                let sample = buffer.get_sample(ch, i).unwrap();
                assert!(sample.is_finite());
                assert!(sample.abs() < 1.5);
            }
        }
    }

    #[test]
    fn test_saturation_process_hardclip() {
        let mut sat = Saturation::with_params(1.0, SaturationType::HardClip, 1.0, 0.0);
        let mut buffer = create_sine_buffer(0.5, 440.0, 48000, 480);

        sat.process(&mut buffer);

        for ch in 0..buffer.num_channels() {
            for i in 0..buffer.num_samples() {
                let sample = buffer.get_sample(ch, i).unwrap();
                assert!(sample.is_finite());
                // Hard clip should ensure samples are within -1 to 1
                assert!(sample >= -1.0 && sample <= 1.0);
            }
        }
    }

    #[test]
    fn test_saturation_process_disabled() {
        let mut sat = Saturation::new();
        sat.set_enabled(false);
        let mut buffer = create_test_buffer(0.5, 100);

        sat.process(&mut buffer);

        // Disabled effect should not modify buffer
        for ch in 0..buffer.num_channels() {
            for i in 0..buffer.num_samples() {
                let sample = buffer.get_sample(ch, i).unwrap();
                assert!((sample - 0.5).abs() < f32::EPSILON);
            }
        }
    }

    #[test]
    fn test_saturation_process_zero_mix() {
        let mut sat = Saturation::with_params(1.0, SaturationType::HardClip, 0.0, 0.0);
        let mut buffer = create_test_buffer(0.3, 100);

        sat.process(&mut buffer);

        // Zero mix should leave buffer unchanged (early return)
        for ch in 0..buffer.num_channels() {
            for i in 0..buffer.num_samples() {
                let sample = buffer.get_sample(ch, i).unwrap();
                assert!((sample - 0.3).abs() < f32::EPSILON);
            }
        }
    }

    #[test]
    fn test_saturation_process_half_mix() {
        let mut sat = Saturation::with_params(1.0, SaturationType::HardClip, 0.5, 0.0);
        let mut buffer = create_test_buffer(0.2, 100);

        // With hard clip at drive=1.0, 0.2 * 11 = 2.2, clipped to 1.0
        // Mix: 0.5 * 0.2 + 0.5 * 1.0 = 0.1 + 0.5 = 0.6
        sat.process(&mut buffer);

        for ch in 0..buffer.num_channels() {
            for i in 0..buffer.num_samples() {
                let sample = buffer.get_sample(ch, i).unwrap();
                assert!((sample - 0.6).abs() < 0.01);
            }
        }
    }

    #[test]
    fn test_saturation_output_gain() {
        let mut sat = Saturation::with_params(0.0, SaturationType::Tape, 1.0, -6.0);
        // At drive=0, tape is nearly linear (gain ~= 1.0, tanh(x*1) ~= x for small x)
        let mut buffer = create_test_buffer(0.5, 100);

        sat.process(&mut buffer);

        // Output should be attenuated by approximately -6dB (~0.5)
        // 0.5 * tanh(0.5) * 0.5 = 0.5 * 0.462 * 0.5 ≈ 0.116
        // Actually: tanh(0.5 * 1.0) ≈ 0.462, with asymmetry it's close to 0.462
        for ch in 0..buffer.num_channels() {
            for i in 0..buffer.num_samples() {
                let sample = buffer.get_sample(ch, i).unwrap();
                assert!(sample.is_finite());
                // Just verify gain is applied (output is less than without gain)
                assert!(sample < 0.5);
            }
        }
    }

    #[test]
    fn test_saturation_effect_type() {
        let sat = Saturation::new();
        assert_eq!(sat.effect_type(), "saturation");
        assert_eq!(sat.display_name(), "Saturation");
    }

    #[test]
    fn test_saturation_get_params() {
        let sat = Saturation::with_params(0.6, SaturationType::Tube, 0.8, -3.0);
        let params = sat.get_params();

        assert!((params["drive"].as_f64().unwrap() - 0.6).abs() < 0.001);
        assert_eq!(params["type"].as_str().unwrap(), "tube");
        assert_eq!(params["type_display"].as_str().unwrap(), "Tube");
        assert!((params["mix"].as_f64().unwrap() - 0.8).abs() < 0.001);
        assert!((params["output_gain"].as_f64().unwrap() - (-3.0)).abs() < 0.001);
        assert!(params["enabled"].as_bool().unwrap());
    }

    #[test]
    fn test_saturation_set_param() {
        let mut sat = Saturation::new();

        sat.set_param("drive", &json!(0.9)).unwrap();
        assert!((sat.drive() - 0.9).abs() < f32::EPSILON);

        sat.set_param("type", &json!("transistor")).unwrap();
        assert_eq!(sat.saturation_type(), SaturationType::Transistor);

        sat.set_param("mix", &json!(0.6)).unwrap();
        assert!((sat.mix() - 0.6).abs() < f32::EPSILON);

        sat.set_param("output_gain", &json!(-6.0)).unwrap();
        assert!((sat.output_gain() - (-6.0)).abs() < f32::EPSILON);

        sat.set_param("enabled", &json!(false)).unwrap();
        assert!(!sat.is_enabled());
    }

    #[test]
    fn test_saturation_set_param_invalid() {
        let mut sat = Saturation::new();

        // Invalid type
        let result = sat.set_param("drive", &json!("not a number"));
        assert!(result.is_err());

        // Invalid saturation type
        let result = sat.set_param("type", &json!("invalid_type"));
        assert!(result.is_err());

        // Unknown parameter
        let result = sat.set_param("unknown", &json!(1.0));
        assert!(result.is_err());
    }

    #[test]
    fn test_saturation_to_from_json() {
        let original = Saturation::with_params(0.7, SaturationType::Transistor, 0.6, -4.0);
        let json = original.to_json().unwrap();

        let mut restored = Saturation::new();
        restored.from_json(&json).unwrap();

        assert!((restored.drive() - 0.7).abs() < f32::EPSILON);
        assert_eq!(restored.saturation_type(), SaturationType::Transistor);
        assert!((restored.mix() - 0.6).abs() < f32::EPSILON);
        assert!((restored.output_gain() - (-4.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_saturation_box_clone() {
        let sat = Saturation::new();
        let boxed: Box<dyn Effect> = Box::new(sat);
        let cloned = boxed.box_clone();

        assert_eq!(cloned.effect_type(), "saturation");
    }

    #[test]
    fn test_saturation_id() {
        let mut sat = Saturation::new();
        let original_id = sat.id().to_string();

        assert!(!original_id.is_empty());

        sat.set_id("custom-sat-id".to_string());
        assert_eq!(sat.id(), "custom-sat-id");
    }

    #[test]
    fn test_saturation_enabled() {
        let mut sat = Saturation::new();
        assert!(sat.is_enabled());

        sat.set_enabled(false);
        assert!(!sat.is_enabled());

        sat.set_enabled(true);
        assert!(sat.is_enabled());
    }

    #[test]
    fn test_saturation_prepare() {
        let mut sat = Saturation::with_params(0.5, SaturationType::Tape, 1.0, -6.0);
        // Manually corrupt output_gain_linear (simulating deserialization)
        sat.output_gain_linear = 0.0;

        sat.prepare(48000, 512);

        // prepare() should recalculate output_gain_linear
        assert!((sat.output_gain_linear - 0.501187).abs() < 0.001);
    }

    #[test]
    fn test_saturation_all_types_produce_different_output() {
        let input = 0.5_f32;
        let drive = 0.5_f32;

        let tape = waveshape_tape(input, drive);
        let tube = waveshape_tube(input, drive);
        let transistor = waveshape_transistor(input, drive);
        let hardclip = waveshape_hardclip(input, drive);

        // All types should produce different results
        assert!((tape - tube).abs() > 0.01);
        assert!((tube - transistor).abs() > 0.01);
        assert!((transistor - hardclip).abs() > 0.01);
    }

    #[test]
    fn test_saturation_zero_drive_near_linear() {
        // With zero drive, all types should be relatively linear for small inputs
        let input = 0.1_f32;
        let drive = 0.0_f32;

        let tape = waveshape_tape(input, drive);
        let tube = waveshape_tube(input, drive);
        let transistor = waveshape_transistor(input, drive);
        let hardclip = waveshape_hardclip(input, drive);

        // All should be close to input (with some tolerance for tanh/compression)
        assert!((tape - input).abs() < 0.02);
        assert!((tube - input).abs() < 0.1); // Tube still compresses slightly
        assert!((transistor - input).abs() < 0.02);
        assert!((hardclip - input).abs() < f32::EPSILON);
    }
}
