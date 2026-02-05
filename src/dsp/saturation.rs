//! Saturation effect (spec §4.2.6)
//!
//! Provides various saturation/distortion types:
//! - TAPE: Soft saturation with asymmetry
//! - TUBE: Even harmonics emphasis
//! - TRANSISTOR: Odd harmonics, harder edge
//! - HARD_CLIP: Digital clipping

use super::effect::{Effect, EffectMetadata};
use super::AudioBuffer;
use crate::error::{NuevaError, Result};
use serde::{Deserialize, Serialize};

/// Saturation type enum (spec §4.2.6)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SaturationType {
    /// Soft saturation with subtle asymmetry - tanh based
    #[default]
    Tape,
    /// Even harmonics emphasis - adds warmth
    Tube,
    /// Odd harmonics, harder edge
    Transistor,
    /// Digital hard clipping (use sparingly)
    HardClip,
}

impl SaturationType {
    /// Get all available saturation types
    pub fn all() -> &'static [SaturationType] {
        &[
            SaturationType::Tape,
            SaturationType::Tube,
            SaturationType::Transistor,
            SaturationType::HardClip,
        ]
    }

    /// Get display name for this saturation type
    pub fn display_name(&self) -> &'static str {
        match self {
            SaturationType::Tape => "Tape",
            SaturationType::Tube => "Tube",
            SaturationType::Transistor => "Transistor",
            SaturationType::HardClip => "Hard Clip",
        }
    }
}

/// Saturation effect parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaturationParams {
    /// Drive amount (0.0 to 1.0), default 0.3
    drive: f32,
    /// Saturation algorithm type
    saturation_type: SaturationType,
    /// Wet/dry mix (0.0 = dry, 1.0 = fully saturated), default 0.5
    mix: f32,
    /// Output gain compensation in dB, default 0.0
    output_gain: f32,
}

impl Default for SaturationParams {
    fn default() -> Self {
        Self {
            drive: 0.3,
            saturation_type: SaturationType::Tape,
            mix: 0.5,
            output_gain: 0.0,
        }
    }
}

/// Saturation effect (spec §4.2.6)
///
/// Waveshaping-based saturation with multiple algorithms.
#[derive(Debug, Clone)]
pub struct Saturation {
    /// Effect parameters
    params: SaturationParams,
    /// Unique instance ID
    id: String,
    /// Whether the effect is enabled
    enabled: bool,
    /// Sample rate (set via prepare)
    sample_rate: f64,
}

impl Default for Saturation {
    fn default() -> Self {
        Self::new()
    }
}

impl Saturation {
    /// Create a new saturation effect with default parameters
    pub fn new() -> Self {
        Self {
            params: SaturationParams::default(),
            id: String::from("saturation-0"),
            enabled: true,
            sample_rate: 44100.0,
        }
    }

    /// Create saturation with specific settings
    pub fn with_params(
        drive: f32,
        saturation_type: SaturationType,
        mix: f32,
        output_gain: f32,
    ) -> Result<Self> {
        let mut sat = Self::new();
        sat.set_drive(drive)?;
        sat.set_saturation_type(saturation_type);
        sat.set_mix(mix)?;
        sat.set_output_gain(output_gain)?;
        Ok(sat)
    }

    // --- Parameter getters ---

    /// Get the drive amount (0.0 to 1.0)
    pub fn drive(&self) -> f32 {
        self.params.drive
    }

    /// Get the saturation type
    pub fn saturation_type(&self) -> SaturationType {
        self.params.saturation_type
    }

    /// Get the wet/dry mix (0.0 to 1.0)
    pub fn mix(&self) -> f32 {
        self.params.mix
    }

    /// Get the output gain in dB
    pub fn output_gain(&self) -> f32 {
        self.params.output_gain
    }

    // --- Parameter setters with validation ---

    /// Set the drive amount (0.0 to 1.0)
    pub fn set_drive(&mut self, drive: f32) -> Result<()> {
        if !(0.0..=1.0).contains(&drive) {
            return Err(NuevaError::InvalidParameter {
                param: "drive".to_string(),
                value: drive.to_string(),
                expected: "0.0 to 1.0".to_string(),
            });
        }
        self.params.drive = drive;
        Ok(())
    }

    /// Set the saturation type
    pub fn set_saturation_type(&mut self, saturation_type: SaturationType) {
        self.params.saturation_type = saturation_type;
    }

    /// Set the wet/dry mix (0.0 to 1.0)
    pub fn set_mix(&mut self, mix: f32) -> Result<()> {
        if !(0.0..=1.0).contains(&mix) {
            return Err(NuevaError::InvalidParameter {
                param: "mix".to_string(),
                value: mix.to_string(),
                expected: "0.0 to 1.0".to_string(),
            });
        }
        self.params.mix = mix;
        Ok(())
    }

    /// Set the output gain in dB (-24 to +24 dB)
    pub fn set_output_gain(&mut self, output_gain: f32) -> Result<()> {
        if !(-24.0..=24.0).contains(&output_gain) {
            return Err(NuevaError::InvalidParameter {
                param: "output_gain".to_string(),
                value: output_gain.to_string(),
                expected: "-24.0 to 24.0 dB".to_string(),
            });
        }
        self.params.output_gain = output_gain;
        Ok(())
    }

    // --- Waveshaping functions ---

    /// Apply tape saturation: tanh(x * drive) with subtle asymmetry
    ///
    /// Tape saturation is characterized by soft clipping with a slight
    /// asymmetry that adds even harmonics for warmth.
    #[inline]
    fn saturate_tape(x: f32, drive: f32) -> f32 {
        // Scale drive to useful range (1.0 to 5.0)
        let drive_scaled = 1.0 + drive * 4.0;
        // Add subtle asymmetry by biasing the input slightly
        let asymmetry = 0.05 * x.signum();
        (x * drive_scaled + asymmetry).tanh()
    }

    /// Apply tube saturation: (x + 0.1 * x^2) * tanh(x * drive)
    ///
    /// Tube saturation emphasizes even harmonics through the x^2 term,
    /// creating warmth while the tanh provides soft limiting.
    #[inline]
    fn saturate_tube(x: f32, drive: f32) -> f32 {
        let drive_scaled = 1.0 + drive * 4.0;
        // Even harmonic generation through x^2 term
        let shaped = x + 0.1 * x * x;
        shaped * (x * drive_scaled).tanh()
    }

    /// Apply transistor saturation: x * (|x| + drive) / (x^2 + (drive - 1) * |x| + 1)
    ///
    /// Transistor saturation produces odd harmonics with a harder edge
    /// than tape or tube saturation.
    #[inline]
    fn saturate_transistor(x: f32, drive: f32) -> f32 {
        // Scale drive to useful range
        let drive_scaled = 0.1 + drive * 0.9;
        let abs_x = x.abs();
        let numerator = x * (abs_x + drive_scaled);
        let denominator = x * x + (drive_scaled - 1.0).abs() * abs_x + 1.0;
        numerator / denominator
    }

    /// Apply hard clipping: clamp(x * drive, -1, 1)
    ///
    /// Digital hard clipping - produces harsh harmonics.
    /// Use sparingly as it can sound aggressive.
    #[inline]
    fn saturate_hard_clip(x: f32, drive: f32) -> f32 {
        // Scale drive to useful range (1.0 to 10.0)
        let drive_scaled = 1.0 + drive * 9.0;
        (x * drive_scaled).clamp(-1.0, 1.0)
    }

    /// Apply saturation to a single sample based on current type
    #[inline]
    fn saturate_sample(&self, x: f32) -> f32 {
        match self.params.saturation_type {
            SaturationType::Tape => Self::saturate_tape(x, self.params.drive),
            SaturationType::Tube => Self::saturate_tube(x, self.params.drive),
            SaturationType::Transistor => Self::saturate_transistor(x, self.params.drive),
            SaturationType::HardClip => Self::saturate_hard_clip(x, self.params.drive),
        }
    }

    /// Convert dB to linear gain
    #[inline]
    fn db_to_linear(db: f32) -> f32 {
        10.0_f32.powf(db / 20.0)
    }
}

impl Effect for Saturation {
    fn process(&mut self, buffer: &mut AudioBuffer) {
        if !self.enabled {
            return;
        }

        let output_gain_linear = Self::db_to_linear(self.params.output_gain);
        let mix = self.params.mix;
        let dry_mix = 1.0 - mix;

        for sample in buffer.samples_mut().iter_mut() {
            let dry = *sample;
            let wet = self.saturate_sample(dry);
            // Apply wet/dry mix and output gain
            *sample = (dry * dry_mix + wet * mix) * output_gain_linear;
        }
    }

    fn prepare(&mut self, sample_rate: f64, _samples_per_block: usize) {
        self.sample_rate = sample_rate;
    }

    fn reset(&mut self) {
        // Saturation is stateless (no delay lines or envelope followers)
        // Nothing to reset
    }

    fn to_json(&self) -> Result<serde_json::Value> {
        serde_json::to_value(&self.params).map_err(|e| NuevaError::SerializationError {
            details: e.to_string(),
        })
    }

    fn from_json(&mut self, json: &serde_json::Value) -> Result<()> {
        let params: SaturationParams =
            serde_json::from_value(json.clone()).map_err(|e| NuevaError::SerializationError {
                details: e.to_string(),
            })?;

        // Validate parameters
        if !(0.0..=1.0).contains(&params.drive) {
            return Err(NuevaError::InvalidParameter {
                param: "drive".to_string(),
                value: params.drive.to_string(),
                expected: "0.0 to 1.0".to_string(),
            });
        }
        if !(0.0..=1.0).contains(&params.mix) {
            return Err(NuevaError::InvalidParameter {
                param: "mix".to_string(),
                value: params.mix.to_string(),
                expected: "0.0 to 1.0".to_string(),
            });
        }
        if !(-24.0..=24.0).contains(&params.output_gain) {
            return Err(NuevaError::InvalidParameter {
                param: "output_gain".to_string(),
                value: params.output_gain.to_string(),
                expected: "-24.0 to 24.0 dB".to_string(),
            });
        }

        self.params = params;
        Ok(())
    }

    fn effect_type(&self) -> &'static str {
        "saturation"
    }

    fn display_name(&self) -> &'static str {
        "Saturation"
    }

    fn metadata(&self) -> EffectMetadata {
        EffectMetadata {
            effect_type: "saturation".to_string(),
            display_name: "Saturation".to_string(),
            category: "distortion".to_string(),
            order_priority: 50, // After EQ/dynamics, before time-based
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_params() {
        let sat = Saturation::new();
        assert_eq!(sat.drive(), 0.3);
        assert_eq!(sat.saturation_type(), SaturationType::Tape);
        assert_eq!(sat.mix(), 0.5);
        assert_eq!(sat.output_gain(), 0.0);
        assert!(sat.is_enabled());
    }

    #[test]
    fn test_param_validation() {
        let mut sat = Saturation::new();

        // Valid drive values
        assert!(sat.set_drive(0.0).is_ok());
        assert!(sat.set_drive(1.0).is_ok());
        assert!(sat.set_drive(0.5).is_ok());

        // Invalid drive values
        assert!(sat.set_drive(-0.1).is_err());
        assert!(sat.set_drive(1.1).is_err());

        // Valid mix values
        assert!(sat.set_mix(0.0).is_ok());
        assert!(sat.set_mix(1.0).is_ok());

        // Invalid mix values
        assert!(sat.set_mix(-0.1).is_err());
        assert!(sat.set_mix(1.1).is_err());

        // Valid output gain values
        assert!(sat.set_output_gain(-24.0).is_ok());
        assert!(sat.set_output_gain(24.0).is_ok());
        assert!(sat.set_output_gain(0.0).is_ok());

        // Invalid output gain values
        assert!(sat.set_output_gain(-25.0).is_err());
        assert!(sat.set_output_gain(25.0).is_err());
    }

    #[test]
    fn test_with_params() {
        let sat = Saturation::with_params(0.5, SaturationType::Tube, 0.8, -3.0).unwrap();
        assert_eq!(sat.drive(), 0.5);
        assert_eq!(sat.saturation_type(), SaturationType::Tube);
        assert_eq!(sat.mix(), 0.8);
        assert_eq!(sat.output_gain(), -3.0);
    }

    #[test]
    fn test_with_params_invalid() {
        assert!(Saturation::with_params(1.5, SaturationType::Tape, 0.5, 0.0).is_err());
        assert!(Saturation::with_params(0.5, SaturationType::Tape, 1.5, 0.0).is_err());
        assert!(Saturation::with_params(0.5, SaturationType::Tape, 0.5, 30.0).is_err());
    }

    #[test]
    fn test_tape_saturation_shape() {
        // Tape saturation should produce soft clipping
        let result = Saturation::saturate_tape(0.5, 0.5);
        assert!(result.abs() < 1.0); // Should be below clipping
        assert!(result > 0.0); // Should preserve sign

        // High input should be compressed
        let high_input = Saturation::saturate_tape(2.0, 1.0);
        assert!(high_input < 2.0); // Should compress
        assert!(high_input.abs() <= 1.0); // tanh is bounded
    }

    #[test]
    fn test_tube_saturation_even_harmonics() {
        // Tube saturation includes x^2 term for even harmonics
        let pos = Saturation::saturate_tube(0.5, 0.3);
        let neg = Saturation::saturate_tube(-0.5, 0.3);

        // Due to the x^2 term, the response should be asymmetric
        // (not perfectly antisymmetric like odd-harmonic distortion)
        assert!((pos.abs() - neg.abs()).abs() > 0.001);
    }

    #[test]
    fn test_transistor_saturation() {
        let result = Saturation::saturate_transistor(0.5, 0.5);
        assert!(result.is_finite());
        assert!(result.abs() < 2.0); // Should be bounded

        // Test symmetry (odd harmonics = antisymmetric)
        let pos = Saturation::saturate_transistor(0.5, 0.5);
        let neg = Saturation::saturate_transistor(-0.5, 0.5);
        assert!((pos + neg).abs() < 0.01); // Should be roughly antisymmetric
    }

    #[test]
    fn test_hard_clip() {
        // Hard clip should clamp to [-1, 1]
        let result = Saturation::saturate_hard_clip(0.5, 1.0);
        assert!(result >= -1.0 && result <= 1.0);

        // With high drive, should clip
        let clipped = Saturation::saturate_hard_clip(0.5, 1.0);
        assert_eq!(clipped, 1.0); // 0.5 * (1 + 9*1) = 5.0, clipped to 1.0
    }

    #[test]
    fn test_process_bypassed() {
        let mut sat = Saturation::new();
        sat.set_enabled(false);

        let mut buffer = AudioBuffer::new(2, 100, 44100.0);
        // Fill with test signal
        for i in 0..100 {
            buffer.set(i, 0, 0.5);
            buffer.set(i, 1, -0.5);
        }

        sat.process(&mut buffer);

        // Should be unchanged when bypassed
        assert_eq!(buffer.get(0, 0), Some(0.5));
        assert_eq!(buffer.get(0, 1), Some(-0.5));
    }

    #[test]
    fn test_process_dry_mix() {
        let mut sat = Saturation::new();
        sat.set_mix(0.0).unwrap(); // Fully dry
        sat.set_output_gain(0.0).unwrap();

        let mut buffer = AudioBuffer::new(1, 100, 44100.0);
        for i in 0..100 {
            buffer.set(i, 0, 0.5);
        }

        sat.process(&mut buffer);

        // With mix=0, output should equal input
        assert!((buffer.get(0, 0).unwrap() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_process_wet_mix() {
        let mut sat = Saturation::new();
        sat.set_drive(0.5).unwrap();
        sat.set_mix(1.0).unwrap(); // Fully wet
        sat.set_output_gain(0.0).unwrap();

        let mut buffer = AudioBuffer::new(1, 100, 44100.0);
        for i in 0..100 {
            buffer.set(i, 0, 0.5);
        }

        let original = buffer.get(0, 0).unwrap();
        sat.process(&mut buffer);
        let processed = buffer.get(0, 0).unwrap();

        // With mix=1, output should be different from input (saturated)
        assert!((processed - original).abs() > 0.01);
    }

    #[test]
    fn test_output_gain() {
        let mut sat = Saturation::new();
        sat.set_mix(0.0).unwrap(); // Dry to isolate gain effect
        sat.set_output_gain(6.0).unwrap(); // +6 dB (approx 2x)

        let mut buffer = AudioBuffer::new(1, 100, 44100.0);
        for i in 0..100 {
            buffer.set(i, 0, 0.25);
        }

        sat.process(&mut buffer);

        // +6 dB should approximately double the amplitude
        let result = buffer.get(0, 0).unwrap();
        assert!((result - 0.5).abs() < 0.05);
    }

    #[test]
    fn test_serialization() {
        let mut sat = Saturation::new();
        sat.set_drive(0.7).unwrap();
        sat.set_saturation_type(SaturationType::Transistor);
        sat.set_mix(0.8).unwrap();
        sat.set_output_gain(-3.0).unwrap();

        let json = sat.to_json().unwrap();

        let mut sat2 = Saturation::new();
        sat2.from_json(&json).unwrap();

        assert_eq!(sat2.drive(), 0.7);
        assert_eq!(sat2.saturation_type(), SaturationType::Transistor);
        assert_eq!(sat2.mix(), 0.8);
        assert_eq!(sat2.output_gain(), -3.0);
    }

    #[test]
    fn test_deserialization_validation() {
        let mut sat = Saturation::new();

        // Invalid drive
        let bad_json = serde_json::json!({
            "drive": 2.0,
            "saturationType": "TAPE",
            "mix": 0.5,
            "outputGain": 0.0
        });
        assert!(sat.from_json(&bad_json).is_err());

        // Invalid mix
        let bad_json = serde_json::json!({
            "drive": 0.5,
            "saturationType": "TAPE",
            "mix": 1.5,
            "outputGain": 0.0
        });
        assert!(sat.from_json(&bad_json).is_err());

        // Invalid output gain
        let bad_json = serde_json::json!({
            "drive": 0.5,
            "saturationType": "TAPE",
            "mix": 0.5,
            "outputGain": 50.0
        });
        assert!(sat.from_json(&bad_json).is_err());
    }

    #[test]
    fn test_effect_trait_methods() {
        let mut sat = Saturation::new();

        assert_eq!(sat.effect_type(), "saturation");
        assert_eq!(sat.display_name(), "Saturation");

        let meta = sat.metadata();
        assert_eq!(meta.effect_type, "saturation");
        assert_eq!(meta.category, "distortion");

        sat.set_id("my-saturation".to_string());
        assert_eq!(sat.id(), "my-saturation");

        sat.set_enabled(false);
        assert!(!sat.is_enabled());
        sat.set_enabled(true);
        assert!(sat.is_enabled());
    }

    #[test]
    fn test_prepare_and_reset() {
        let mut sat = Saturation::new();

        // prepare should set sample rate
        sat.prepare(48000.0, 512);
        assert_eq!(sat.sample_rate, 48000.0);

        // reset should not panic (saturation is stateless)
        sat.reset();
    }

    #[test]
    fn test_saturation_type_display_names() {
        assert_eq!(SaturationType::Tape.display_name(), "Tape");
        assert_eq!(SaturationType::Tube.display_name(), "Tube");
        assert_eq!(SaturationType::Transistor.display_name(), "Transistor");
        assert_eq!(SaturationType::HardClip.display_name(), "Hard Clip");
    }

    #[test]
    fn test_saturation_type_all() {
        let all = SaturationType::all();
        assert_eq!(all.len(), 4);
        assert!(all.contains(&SaturationType::Tape));
        assert!(all.contains(&SaturationType::Tube));
        assert!(all.contains(&SaturationType::Transistor));
        assert!(all.contains(&SaturationType::HardClip));
    }

    #[test]
    fn test_all_saturation_types_process() {
        for sat_type in SaturationType::all() {
            let mut sat = Saturation::new();
            sat.set_saturation_type(*sat_type);
            sat.set_drive(0.5).unwrap();
            sat.set_mix(1.0).unwrap();

            let mut buffer = AudioBuffer::new(2, 100, 44100.0);
            // Fill with test signal
            for i in 0..100 {
                let t = i as f32 / 44100.0;
                let sample = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
                buffer.set(i, 0, sample);
                buffer.set(i, 1, sample);
            }

            sat.process(&mut buffer);

            // Verify output is valid
            for i in 0..100 {
                let left = buffer.get(i, 0).unwrap();
                let right = buffer.get(i, 1).unwrap();
                assert!(left.is_finite(), "NaN/Inf for {:?}", sat_type);
                assert!(right.is_finite(), "NaN/Inf for {:?}", sat_type);
                assert!(
                    left.abs() <= 2.0,
                    "Extreme value {} for {:?}",
                    left,
                    sat_type
                );
            }
        }
    }

    #[test]
    fn test_stereo_processing() {
        let mut sat = Saturation::new();
        sat.set_drive(0.5).unwrap();
        sat.set_mix(1.0).unwrap();

        let mut buffer = AudioBuffer::new(2, 100, 44100.0);
        for i in 0..100 {
            buffer.set(i, 0, 0.3);
            buffer.set(i, 1, -0.3);
        }

        sat.process(&mut buffer);

        // Left and right should be processed independently
        let left = buffer.get(0, 0).unwrap();
        let right = buffer.get(0, 1).unwrap();

        // For tape saturation with symmetric input, output should be roughly antisymmetric
        // (with small asymmetry due to tape character)
        assert!(left > 0.0);
        assert!(right < 0.0);
    }

    #[test]
    fn test_zero_input() {
        let mut sat = Saturation::new();
        sat.set_drive(1.0).unwrap();
        sat.set_mix(1.0).unwrap();

        let mut buffer = AudioBuffer::new(1, 100, 44100.0);
        // Buffer is already zero-initialized

        sat.process(&mut buffer);

        // Zero in should produce (near) zero out
        for i in 0..100 {
            let sample = buffer.get(i, 0).unwrap();
            assert!(
                sample.abs() < 0.1,
                "Zero input produced non-zero output: {}",
                sample
            );
        }
    }

    #[test]
    fn test_db_to_linear() {
        // 0 dB = 1.0
        assert!((Saturation::db_to_linear(0.0) - 1.0).abs() < 0.001);

        // -6 dB = 0.5 (approximately)
        assert!((Saturation::db_to_linear(-6.02) - 0.5).abs() < 0.01);

        // +6 dB = 2.0 (approximately)
        assert!((Saturation::db_to_linear(6.02) - 2.0).abs() < 0.01);

        // -20 dB = 0.1
        assert!((Saturation::db_to_linear(-20.0) - 0.1).abs() < 0.01);
    }
}
