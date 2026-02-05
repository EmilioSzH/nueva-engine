//! Parametric EQ Effect (spec 4.2.2)
//!
//! Implements a multi-band parametric equalizer with cascaded biquad filters.
//! Supports peak, shelf, and pass filters.

use super::{AudioBuffer, Effect, EffectMetadata};
use crate::error::{NuevaError, Result};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Maximum number of EQ bands
pub const MAX_BANDS: usize = 8;

/// Filter type for EQ bands
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterType {
    /// Bell curve boost/cut
    #[default]
    Peak,
    /// Boost/cut below frequency
    LowShelf,
    /// Boost/cut above frequency
    HighShelf,
    /// Remove above frequency (low-pass filter)
    LowPass,
    /// Remove below frequency (high-pass filter)
    HighPass,
}

/// Biquad filter coefficients
/// Transfer function: H(z) = (b0 + b1*z^-1 + b2*z^-2) / (a0 + a1*z^-1 + a2*z^-2)
/// Normalized: all coefficients divided by a0
#[derive(Debug, Clone, Copy, Default)]
struct BiquadCoeffs {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl BiquadCoeffs {
    /// Calculate biquad coefficients using Audio EQ Cookbook formulas
    /// Reference: https://www.w3.org/2011/audio/audio-eq-cookbook.html
    fn calculate(
        filter_type: FilterType,
        sample_rate: f64,
        frequency: f64,
        gain_db: f64,
        q: f64,
    ) -> Self {
        // Clamp frequency to valid range (below Nyquist)
        let freq = frequency.clamp(20.0, sample_rate / 2.0 - 1.0);
        let q = q.clamp(0.1, 10.0);

        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        // For shelf filters, calculate A (amplitude)
        let a = (10.0_f64).powf(gain_db / 40.0);

        let (b0, b1, b2, a0, a1, a2) = match filter_type {
            FilterType::Peak => {
                // Peaking EQ (constant-Q)
                let a_peak = (10.0_f64).powf(gain_db / 40.0);
                (
                    1.0 + alpha * a_peak, // b0
                    -2.0 * cos_w0,        // b1
                    1.0 - alpha * a_peak, // b2
                    1.0 + alpha / a_peak, // a0
                    -2.0 * cos_w0,        // a1
                    1.0 - alpha / a_peak, // a2
                )
            }
            FilterType::LowShelf => {
                // Low shelf filter
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                    (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
                    (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
                )
            }
            FilterType::HighShelf => {
                // High shelf filter
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                    (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
                    (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
                )
            }
            FilterType::LowPass => {
                // Low-pass filter (Butterworth-style)
                (
                    (1.0 - cos_w0) / 2.0,
                    1.0 - cos_w0,
                    (1.0 - cos_w0) / 2.0,
                    1.0 + alpha,
                    -2.0 * cos_w0,
                    1.0 - alpha,
                )
            }
            FilterType::HighPass => {
                // High-pass filter (Butterworth-style)
                (
                    (1.0 + cos_w0) / 2.0,
                    -(1.0 + cos_w0),
                    (1.0 + cos_w0) / 2.0,
                    1.0 + alpha,
                    -2.0 * cos_w0,
                    1.0 - alpha,
                )
            }
        };

        // Normalize by a0
        BiquadCoeffs {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// Check if coefficients represent a bypass (unity gain, no filtering)
    fn is_bypass(&self) -> bool {
        (self.b0 - 1.0).abs() < 1e-10
            && self.b1.abs() < 1e-10
            && self.b2.abs() < 1e-10
            && self.a1.abs() < 1e-10
            && self.a2.abs() < 1e-10
    }
}

/// Biquad filter state for one channel
#[derive(Debug, Clone, Copy, Default)]
struct BiquadState {
    x1: f64, // x[n-1]
    x2: f64, // x[n-2]
    y1: f64, // y[n-1]
    y2: f64, // y[n-2]
}

impl BiquadState {
    /// Process a single sample through the biquad filter
    /// Direct Form II implementation
    fn process(&mut self, input: f64, coeffs: &BiquadCoeffs) -> f64 {
        let output = coeffs.b0 * input + coeffs.b1 * self.x1 + coeffs.b2 * self.x2
            - coeffs.a1 * self.y1
            - coeffs.a2 * self.y2;

        // Shift delay line
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;

        output
    }

    /// Reset filter state
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// Single EQ band configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EQBand {
    /// Center/corner frequency in Hz (20-20000)
    pub frequency: f32,
    /// Gain in dB (-24 to +24)
    #[serde(rename = "gain_db")]
    pub gain_db: f32,
    /// Q factor / bandwidth (0.1 to 10.0)
    pub q: f32,
    /// Filter type
    pub filter_type: FilterType,
    /// Whether this band is enabled
    pub enabled: bool,
}

impl Default for EQBand {
    fn default() -> Self {
        Self {
            frequency: 1000.0,
            gain_db: 0.0,
            q: 1.0,
            filter_type: FilterType::Peak,
            enabled: true,
        }
    }
}

impl EQBand {
    /// Create a new EQ band with the specified parameters
    pub fn new(frequency: f32, gain_db: f32, q: f32, filter_type: FilterType) -> Self {
        Self {
            frequency,
            gain_db,
            q,
            filter_type,
            enabled: true,
        }
    }

    /// Create a peak filter band
    pub fn peak(frequency: f32, gain_db: f32, q: f32) -> Self {
        Self::new(frequency, gain_db, q, FilterType::Peak)
    }

    /// Create a low shelf filter band
    pub fn low_shelf(frequency: f32, gain_db: f32, q: f32) -> Self {
        Self::new(frequency, gain_db, q, FilterType::LowShelf)
    }

    /// Create a high shelf filter band
    pub fn high_shelf(frequency: f32, gain_db: f32, q: f32) -> Self {
        Self::new(frequency, gain_db, q, FilterType::HighShelf)
    }

    /// Create a low-pass filter band
    pub fn low_pass(frequency: f32, q: f32) -> Self {
        Self::new(frequency, 0.0, q, FilterType::LowPass)
    }

    /// Create a high-pass filter band
    pub fn high_pass(frequency: f32, q: f32) -> Self {
        Self::new(frequency, 0.0, q, FilterType::HighPass)
    }

    /// Validate band parameters
    pub fn validate(&self) -> Result<()> {
        if self.frequency < 20.0 || self.frequency > 20000.0 {
            return Err(NuevaError::InvalidParameter {
                param: "frequency".to_string(),
                value: self.frequency.to_string(),
                expected: "20-20000 Hz".to_string(),
            });
        }

        if self.gain_db < -24.0 || self.gain_db > 24.0 {
            return Err(NuevaError::InvalidParameter {
                param: "gain_db".to_string(),
                value: self.gain_db.to_string(),
                expected: "-24 to +24 dB".to_string(),
            });
        }

        if self.q < 0.1 || self.q > 10.0 {
            return Err(NuevaError::InvalidParameter {
                param: "q".to_string(),
                value: self.q.to_string(),
                expected: "0.1 to 10.0".to_string(),
            });
        }

        Ok(())
    }

    /// Check if this band should be bypassed (no effect on audio)
    fn is_bypass(&self) -> bool {
        !self.enabled
            || match self.filter_type {
                FilterType::Peak | FilterType::LowShelf | FilterType::HighShelf => {
                    self.gain_db.abs() < 0.01
                }
                FilterType::LowPass | FilterType::HighPass => false,
            }
    }
}

/// Internal state for a single band (per-channel)
#[derive(Debug, Clone, Default)]
struct BandState {
    coeffs: BiquadCoeffs,
    states: Vec<BiquadState>, // One per channel
}

/// Parametric EQ effect with up to 8 bands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParametricEQ {
    /// Unique instance ID
    id: String,
    /// Whether the effect is enabled
    enabled: bool,
    /// EQ bands (max 8)
    bands: Vec<EQBand>,
    /// Sample rate (not serialized)
    #[serde(skip)]
    sample_rate: f64,
    /// Number of channels (not serialized)
    #[serde(skip)]
    num_channels: usize,
    /// Filter states for each band (not serialized)
    #[serde(skip)]
    band_states: Vec<BandState>,
    /// Whether coefficients need recalculation (not serialized)
    #[serde(skip)]
    coeffs_dirty: bool,
}

impl Default for ParametricEQ {
    fn default() -> Self {
        Self {
            id: String::new(),
            enabled: true,
            bands: Vec::new(),
            sample_rate: 48000.0,
            num_channels: 2,
            band_states: Vec::new(),
            coeffs_dirty: true,
        }
    }
}

impl ParametricEQ {
    /// Create a new parametric EQ with no bands
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new parametric EQ with the specified bands
    pub fn with_bands(bands: Vec<EQBand>) -> Result<Self> {
        if bands.len() > MAX_BANDS {
            return Err(NuevaError::InvalidParameter {
                param: "bands".to_string(),
                value: bands.len().to_string(),
                expected: format!("0-{} bands", MAX_BANDS),
            });
        }

        for band in &bands {
            band.validate()?;
        }

        Ok(Self {
            bands,
            coeffs_dirty: true,
            ..Default::default()
        })
    }

    /// Add a band to the EQ
    pub fn add_band(&mut self, band: EQBand) -> Result<()> {
        if self.bands.len() >= MAX_BANDS {
            return Err(NuevaError::InvalidParameter {
                param: "bands".to_string(),
                value: self.bands.len().to_string(),
                expected: format!("maximum {} bands", MAX_BANDS),
            });
        }

        band.validate()?;
        self.bands.push(band);
        self.coeffs_dirty = true;
        Ok(())
    }

    /// Remove a band at the given index
    pub fn remove_band(&mut self, index: usize) -> Option<EQBand> {
        if index < self.bands.len() {
            self.coeffs_dirty = true;
            Some(self.bands.remove(index))
        } else {
            None
        }
    }

    /// Get a reference to the bands
    pub fn bands(&self) -> &[EQBand] {
        &self.bands
    }

    /// Get a mutable reference to a band
    pub fn band_mut(&mut self, index: usize) -> Option<&mut EQBand> {
        self.coeffs_dirty = true;
        self.bands.get_mut(index)
    }

    /// Clear all bands
    pub fn clear_bands(&mut self) {
        self.bands.clear();
        self.band_states.clear();
        self.coeffs_dirty = true;
    }

    /// Update filter coefficients if needed
    fn update_coefficients(&mut self) {
        if !self.coeffs_dirty {
            return;
        }

        // Resize band states to match number of bands
        self.band_states
            .resize_with(self.bands.len(), BandState::default);

        for (i, band) in self.bands.iter().enumerate() {
            // Resize channel states
            self.band_states[i]
                .states
                .resize_with(self.num_channels, BiquadState::default);

            // Calculate coefficients
            if band.is_bypass() {
                // Create unity/bypass coefficients
                self.band_states[i].coeffs = BiquadCoeffs {
                    b0: 1.0,
                    b1: 0.0,
                    b2: 0.0,
                    a1: 0.0,
                    a2: 0.0,
                };
            } else {
                self.band_states[i].coeffs = BiquadCoeffs::calculate(
                    band.filter_type,
                    self.sample_rate,
                    band.frequency as f64,
                    band.gain_db as f64,
                    band.q as f64,
                );
            }
        }

        self.coeffs_dirty = false;
    }

    /// Process a single sample through all bands for a given channel
    fn process_sample(&mut self, sample: f32, channel: usize) -> f32 {
        let mut output = sample as f64;

        for band_state in &mut self.band_states {
            if !band_state.coeffs.is_bypass() {
                if let Some(state) = band_state.states.get_mut(channel) {
                    output = state.process(output, &band_state.coeffs);
                }
            }
        }

        output as f32
    }
}

impl Effect for ParametricEQ {
    fn process(&mut self, buffer: &mut AudioBuffer) {
        if !self.enabled || self.bands.is_empty() {
            return;
        }

        // Update coefficients if needed
        self.update_coefficients();

        let num_channels = buffer.num_channels();
        let num_samples = buffer.num_samples();

        // Process each sample through all bands
        for frame in 0..num_samples {
            for channel in 0..num_channels {
                if let Some(sample) = buffer.get(frame, channel) {
                    let processed = self.process_sample(sample, channel);
                    buffer.set(frame, channel, processed);
                }
            }
        }
    }

    fn prepare(&mut self, sample_rate: f64, _samples_per_block: usize) {
        self.sample_rate = sample_rate;
        self.coeffs_dirty = true;
    }

    fn reset(&mut self) {
        for band_state in &mut self.band_states {
            for state in &mut band_state.states {
                state.reset();
            }
        }
    }

    fn to_json(&self) -> Result<serde_json::Value> {
        serde_json::to_value(self).map_err(|e| NuevaError::SerializationError {
            details: e.to_string(),
        })
    }

    fn from_json(&mut self, json: &serde_json::Value) -> Result<()> {
        let deserialized: ParametricEQ =
            serde_json::from_value(json.clone()).map_err(|e| NuevaError::SerializationError {
                details: e.to_string(),
            })?;

        // Validate all bands
        for band in &deserialized.bands {
            band.validate()?;
        }

        self.id = deserialized.id;
        self.enabled = deserialized.enabled;
        self.bands = deserialized.bands;
        self.coeffs_dirty = true;

        Ok(())
    }

    fn effect_type(&self) -> &'static str {
        "parametric-eq"
    }

    fn display_name(&self) -> &'static str {
        "Parametric EQ"
    }

    fn metadata(&self) -> EffectMetadata {
        EffectMetadata {
            effect_type: self.effect_type().to_string(),
            display_name: self.display_name().to_string(),
            category: "eq".to_string(),
            order_priority: 20, // EQ typically comes early in the chain
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

    /// Helper to create a test buffer with a specific frequency sine wave
    fn create_sine_buffer(frequency: f64, sample_rate: f64, duration_secs: f64) -> AudioBuffer {
        let num_samples = (sample_rate * duration_secs) as usize;
        let mut buffer = AudioBuffer::new(1, num_samples, sample_rate);

        for i in 0..num_samples {
            let t = i as f64 / sample_rate;
            let sample = (2.0 * PI * frequency * t).sin() as f32;
            buffer.set(i, 0, sample);
        }

        buffer
    }

    /// Calculate RMS of a buffer (linear, not dB)
    fn calculate_rms(buffer: &AudioBuffer, channel: usize) -> f64 {
        let sum_sq: f64 = (0..buffer.num_samples())
            .filter_map(|i| buffer.get(i, channel))
            .map(|s| (s as f64).powi(2))
            .sum();

        (sum_sq / buffer.num_samples() as f64).sqrt()
    }

    #[test]
    fn test_band_validation() {
        // Valid band
        let band = EQBand::peak(1000.0, 6.0, 1.0);
        assert!(band.validate().is_ok());

        // Invalid frequency - too low
        let band = EQBand::peak(10.0, 0.0, 1.0);
        assert!(band.validate().is_err());

        // Invalid frequency - too high
        let band = EQBand::peak(25000.0, 0.0, 1.0);
        assert!(band.validate().is_err());

        // Invalid gain - too high
        let band = EQBand::peak(1000.0, 30.0, 1.0);
        assert!(band.validate().is_err());

        // Invalid Q - too low
        let band = EQBand::peak(1000.0, 0.0, 0.05);
        assert!(band.validate().is_err());
    }

    #[test]
    fn test_max_bands_limit() {
        let mut eq = ParametricEQ::new();

        // Add maximum bands
        for _ in 0..MAX_BANDS {
            assert!(eq.add_band(EQBand::default()).is_ok());
        }

        // Try to add one more - should fail
        assert!(eq.add_band(EQBand::default()).is_err());
    }

    #[test]
    fn test_peak_filter_boost() {
        let mut eq = ParametricEQ::new();
        eq.prepare(48000.0, 512);
        eq.add_band(EQBand::peak(1000.0, 12.0, 1.0)).unwrap();

        // Create a sine wave at the boost frequency
        let mut buffer = create_sine_buffer(1000.0, 48000.0, 0.1);
        let rms_before = calculate_rms(&buffer, 0);

        eq.process(&mut buffer);

        let rms_after = calculate_rms(&buffer, 0);

        // 12dB boost should increase amplitude by ~4x (10^(12/20) = 3.98)
        let gain_ratio = rms_after / rms_before;
        assert!(
            gain_ratio > 3.0 && gain_ratio < 5.0,
            "Expected ~4x gain, got {}",
            gain_ratio
        );
    }

    #[test]
    fn test_peak_filter_cut() {
        let mut eq = ParametricEQ::new();
        eq.prepare(48000.0, 512);
        eq.add_band(EQBand::peak(1000.0, -12.0, 1.0)).unwrap();

        let mut buffer = create_sine_buffer(1000.0, 48000.0, 0.1);
        let rms_before = calculate_rms(&buffer, 0);

        eq.process(&mut buffer);

        let rms_after = calculate_rms(&buffer, 0);

        // -12dB cut should decrease amplitude by ~4x
        let gain_ratio = rms_after / rms_before;
        assert!(
            gain_ratio > 0.2 && gain_ratio < 0.35,
            "Expected ~0.25 gain, got {}",
            gain_ratio
        );
    }

    #[test]
    fn test_low_shelf_boost() {
        let mut eq = ParametricEQ::new();
        eq.prepare(48000.0, 512);
        eq.add_band(EQBand::low_shelf(500.0, 12.0, 0.7)).unwrap();

        // Low frequency sine (below shelf frequency)
        let mut buffer_low = create_sine_buffer(100.0, 48000.0, 0.1);
        let rms_low_before = calculate_rms(&buffer_low, 0);

        // High frequency sine (above shelf frequency)
        let mut buffer_high = create_sine_buffer(2000.0, 48000.0, 0.1);
        let rms_high_before = calculate_rms(&buffer_high, 0);

        eq.process(&mut buffer_low);
        eq.reset();
        eq.process(&mut buffer_high);

        let rms_low_after = calculate_rms(&buffer_low, 0);
        let rms_high_after = calculate_rms(&buffer_high, 0);

        // Low frequencies should be boosted significantly
        let low_gain = rms_low_after / rms_low_before;
        assert!(
            low_gain > 2.5,
            "Low frequencies should be boosted, got {}",
            low_gain
        );

        // High frequencies should be relatively unchanged
        let high_gain = rms_high_after / rms_high_before;
        assert!(
            high_gain < 1.5,
            "High frequencies should be less affected, got {}",
            high_gain
        );
    }

    #[test]
    fn test_high_shelf_boost() {
        let mut eq = ParametricEQ::new();
        eq.prepare(48000.0, 512);
        eq.add_band(EQBand::high_shelf(2000.0, 12.0, 0.7)).unwrap();

        // Low frequency sine
        let mut buffer_low = create_sine_buffer(200.0, 48000.0, 0.1);
        let rms_low_before = calculate_rms(&buffer_low, 0);

        // High frequency sine
        let mut buffer_high = create_sine_buffer(8000.0, 48000.0, 0.1);
        let rms_high_before = calculate_rms(&buffer_high, 0);

        eq.process(&mut buffer_low);
        eq.reset();
        eq.process(&mut buffer_high);

        let rms_low_after = calculate_rms(&buffer_low, 0);
        let rms_high_after = calculate_rms(&buffer_high, 0);

        // High frequencies should be boosted significantly
        let high_gain = rms_high_after / rms_high_before;
        assert!(
            high_gain > 2.5,
            "High frequencies should be boosted, got {}",
            high_gain
        );

        // Low frequencies should be relatively unchanged
        let low_gain = rms_low_after / rms_low_before;
        assert!(
            low_gain < 1.5,
            "Low frequencies should be less affected, got {}",
            low_gain
        );
    }

    #[test]
    fn test_low_pass_filter() {
        let mut eq = ParametricEQ::new();
        eq.prepare(48000.0, 512);
        eq.add_band(EQBand::low_pass(1000.0, 0.7)).unwrap();

        // Low frequency sine (below cutoff)
        let mut buffer_low = create_sine_buffer(200.0, 48000.0, 0.1);
        let rms_low_before = calculate_rms(&buffer_low, 0);

        // High frequency sine (above cutoff)
        let mut buffer_high = create_sine_buffer(4000.0, 48000.0, 0.1);
        let rms_high_before = calculate_rms(&buffer_high, 0);

        eq.process(&mut buffer_low);
        eq.reset();
        eq.process(&mut buffer_high);

        let rms_low_after = calculate_rms(&buffer_low, 0);
        let rms_high_after = calculate_rms(&buffer_high, 0);

        // Low frequencies should pass through (near unity)
        let low_gain = rms_low_after / rms_low_before;
        assert!(
            low_gain > 0.8 && low_gain < 1.2,
            "Low frequencies should pass, got {}",
            low_gain
        );

        // High frequencies should be significantly attenuated
        let high_gain = rms_high_after / rms_high_before;
        assert!(
            high_gain < 0.3,
            "High frequencies should be attenuated, got {}",
            high_gain
        );
    }

    #[test]
    fn test_high_pass_filter() {
        let mut eq = ParametricEQ::new();
        eq.prepare(48000.0, 512);
        eq.add_band(EQBand::high_pass(1000.0, 0.7)).unwrap();

        // Low frequency sine (below cutoff)
        let mut buffer_low = create_sine_buffer(200.0, 48000.0, 0.1);
        let rms_low_before = calculate_rms(&buffer_low, 0);

        // High frequency sine (above cutoff)
        let mut buffer_high = create_sine_buffer(4000.0, 48000.0, 0.1);
        let rms_high_before = calculate_rms(&buffer_high, 0);

        eq.process(&mut buffer_low);
        eq.reset();
        eq.process(&mut buffer_high);

        let rms_low_after = calculate_rms(&buffer_low, 0);
        let rms_high_after = calculate_rms(&buffer_high, 0);

        // Low frequencies should be significantly attenuated
        let low_gain = rms_low_after / rms_low_before;
        assert!(
            low_gain < 0.3,
            "Low frequencies should be attenuated, got {}",
            low_gain
        );

        // High frequencies should pass through (near unity)
        let high_gain = rms_high_after / rms_high_before;
        assert!(
            high_gain > 0.8 && high_gain < 1.2,
            "High frequencies should pass, got {}",
            high_gain
        );
    }

    #[test]
    fn test_zero_gain_bypass() {
        let mut eq = ParametricEQ::new();
        eq.prepare(48000.0, 512);
        eq.add_band(EQBand::peak(1000.0, 0.0, 1.0)).unwrap();

        let mut buffer = create_sine_buffer(1000.0, 48000.0, 0.1);
        let original = buffer.create_copy();

        eq.process(&mut buffer);

        // With zero gain, output should be identical to input
        for i in 0..buffer.num_samples() {
            let diff = (buffer.get(i, 0).unwrap() - original.get(i, 0).unwrap()).abs();
            assert!(diff < 0.001, "Zero gain should bypass, diff = {}", diff);
        }
    }

    #[test]
    fn test_disabled_band_bypass() {
        let mut eq = ParametricEQ::new();
        eq.prepare(48000.0, 512);

        let mut band = EQBand::peak(1000.0, 12.0, 1.0);
        band.enabled = false;
        eq.add_band(band).unwrap();

        let mut buffer = create_sine_buffer(1000.0, 48000.0, 0.1);
        let original = buffer.create_copy();

        eq.process(&mut buffer);

        // With disabled band, output should be identical to input
        for i in 0..buffer.num_samples() {
            let diff = (buffer.get(i, 0).unwrap() - original.get(i, 0).unwrap()).abs();
            assert!(diff < 0.001, "Disabled band should bypass, diff = {}", diff);
        }
    }

    #[test]
    fn test_disabled_eq_bypass() {
        let mut eq = ParametricEQ::new();
        eq.prepare(48000.0, 512);
        eq.add_band(EQBand::peak(1000.0, 12.0, 1.0)).unwrap();
        eq.set_enabled(false);

        let mut buffer = create_sine_buffer(1000.0, 48000.0, 0.1);
        let original = buffer.create_copy();

        eq.process(&mut buffer);

        // With disabled EQ, output should be identical to input
        for i in 0..buffer.num_samples() {
            let diff = (buffer.get(i, 0).unwrap() - original.get(i, 0).unwrap()).abs();
            assert!(diff < 0.001, "Disabled EQ should bypass, diff = {}", diff);
        }
    }

    #[test]
    fn test_multiple_bands() {
        let mut eq = ParametricEQ::new();
        eq.prepare(48000.0, 512);

        // Add multiple bands
        eq.add_band(EQBand::high_pass(80.0, 0.7)).unwrap();
        eq.add_band(EQBand::peak(250.0, -3.0, 2.0)).unwrap();
        eq.add_band(EQBand::peak(3000.0, 2.0, 1.5)).unwrap();
        eq.add_band(EQBand::high_shelf(8000.0, 3.0, 0.7)).unwrap();

        assert_eq!(eq.bands().len(), 4);

        let mut buffer = create_sine_buffer(1000.0, 48000.0, 0.1);

        // Should process without error
        eq.process(&mut buffer);

        // Buffer should still be valid
        assert!(buffer.is_valid());
    }

    #[test]
    fn test_stereo_processing() {
        let mut eq = ParametricEQ::new();
        eq.prepare(48000.0, 512);
        eq.add_band(EQBand::peak(1000.0, 6.0, 1.0)).unwrap();

        // Create stereo buffer
        let num_samples = 4800;
        let mut buffer = AudioBuffer::new(2, num_samples, 48000.0);

        // Fill with different signals on each channel
        for i in 0..num_samples {
            let t = i as f64 / 48000.0;
            buffer.set(i, 0, (2.0 * PI * 1000.0 * t).sin() as f32);
            buffer.set(i, 1, (2.0 * PI * 1000.0 * t).cos() as f32);
        }

        eq.process(&mut buffer);

        // Both channels should be processed
        assert!(buffer.is_valid());

        // Channels should still be different (phase relationship preserved)
        let mut channels_equal = true;
        for i in 0..100 {
            if (buffer.get(i, 0).unwrap() - buffer.get(i, 1).unwrap()).abs() > 0.001 {
                channels_equal = false;
                break;
            }
        }
        assert!(!channels_equal, "Stereo channels should remain independent");
    }

    #[test]
    fn test_serialization() {
        let mut eq = ParametricEQ::new();
        eq.set_id("test-eq-1".to_string());
        eq.add_band(EQBand::peak(1000.0, 6.0, 1.0)).unwrap();
        eq.add_band(EQBand::low_shelf(200.0, 3.0, 0.7)).unwrap();

        // Serialize
        let json = eq.to_json().expect("Serialization should succeed");

        // Deserialize into new instance
        let mut eq2 = ParametricEQ::new();
        eq2.from_json(&json)
            .expect("Deserialization should succeed");

        assert_eq!(eq2.id(), "test-eq-1");
        assert_eq!(eq2.bands().len(), 2);
        assert_eq!(eq2.bands()[0].frequency, 1000.0);
        assert_eq!(eq2.bands()[0].gain_db, 6.0);
        assert_eq!(eq2.bands()[1].filter_type, FilterType::LowShelf);
    }

    #[test]
    fn test_metadata() {
        let eq = ParametricEQ::new();
        let meta = eq.metadata();

        assert_eq!(meta.effect_type, "parametric-eq");
        assert_eq!(meta.display_name, "Parametric EQ");
        assert_eq!(meta.category, "eq");
    }

    #[test]
    fn test_reset_clears_state() {
        let mut eq = ParametricEQ::new();
        eq.prepare(48000.0, 512);
        eq.add_band(EQBand::low_pass(1000.0, 0.7)).unwrap();

        // Process some audio to build up filter state
        let mut buffer = create_sine_buffer(500.0, 48000.0, 0.1);
        eq.process(&mut buffer);

        // Reset the filter
        eq.reset();

        // After reset, processing should start fresh
        // Create a new buffer and verify it processes correctly
        let mut buffer2 = create_sine_buffer(500.0, 48000.0, 0.1);
        eq.process(&mut buffer2);

        assert!(buffer2.is_valid());
    }

    #[test]
    fn test_coefficient_recalculation_on_sample_rate_change() {
        let mut eq = ParametricEQ::new();
        eq.add_band(EQBand::peak(1000.0, 12.0, 1.0)).unwrap();

        // Prepare at 44100 Hz
        eq.prepare(44100.0, 512);
        let mut buffer_44k = create_sine_buffer(1000.0, 44100.0, 0.1);
        eq.process(&mut buffer_44k);
        let rms_44k = calculate_rms(&buffer_44k, 0);

        // Reset and prepare at 96000 Hz
        eq.reset();
        eq.prepare(96000.0, 512);
        let mut buffer_96k = create_sine_buffer(1000.0, 96000.0, 0.1);
        eq.process(&mut buffer_96k);
        let rms_96k = calculate_rms(&buffer_96k, 0);

        // Both should have similar gain at the target frequency
        // (within some tolerance due to different sample rates)
        let ratio = rms_44k / rms_96k;
        assert!(
            ratio > 0.7 && ratio < 1.4,
            "Gain should be similar at different sample rates, ratio = {}",
            ratio
        );
    }

    #[test]
    fn test_remove_band() {
        let mut eq = ParametricEQ::new();
        eq.add_band(EQBand::peak(100.0, 3.0, 1.0)).unwrap();
        eq.add_band(EQBand::peak(1000.0, 6.0, 1.0)).unwrap();
        eq.add_band(EQBand::peak(10000.0, 3.0, 1.0)).unwrap();

        assert_eq!(eq.bands().len(), 3);

        // Remove middle band
        let removed = eq.remove_band(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().frequency, 1000.0);
        assert_eq!(eq.bands().len(), 2);

        // Remove invalid index
        let removed = eq.remove_band(10);
        assert!(removed.is_none());
    }

    #[test]
    fn test_clear_bands() {
        let mut eq = ParametricEQ::new();
        eq.add_band(EQBand::peak(100.0, 3.0, 1.0)).unwrap();
        eq.add_band(EQBand::peak(1000.0, 6.0, 1.0)).unwrap();

        eq.clear_bands();

        assert!(eq.bands().is_empty());
    }

    #[test]
    fn test_with_bands_constructor() {
        let bands = vec![
            EQBand::high_pass(80.0, 0.7),
            EQBand::peak(1000.0, 3.0, 1.5),
            EQBand::high_shelf(8000.0, 2.0, 0.7),
        ];

        let eq = ParametricEQ::with_bands(bands).expect("Should create EQ with valid bands");
        assert_eq!(eq.bands().len(), 3);
    }

    #[test]
    fn test_with_bands_rejects_too_many() {
        let bands: Vec<EQBand> = (0..10).map(|_| EQBand::default()).collect();

        let result = ParametricEQ::with_bands(bands);
        assert!(result.is_err());
    }

    #[test]
    fn test_with_bands_rejects_invalid() {
        let bands = vec![EQBand::peak(10.0, 0.0, 1.0)]; // Invalid frequency

        let result = ParametricEQ::with_bands(bands);
        assert!(result.is_err());
    }

    #[test]
    fn test_band_mut_marks_dirty() {
        let mut eq = ParametricEQ::new();
        eq.prepare(48000.0, 512);
        eq.add_band(EQBand::peak(1000.0, 0.0, 1.0)).unwrap();

        // Force coefficient calculation
        let mut buffer = AudioBuffer::new(1, 100, 48000.0);
        eq.process(&mut buffer);

        // Modify band through mutable reference
        if let Some(band) = eq.band_mut(0) {
            band.gain_db = 12.0;
        }

        // Coefficients should be recalculated on next process
        let mut buffer2 = create_sine_buffer(1000.0, 48000.0, 0.1);
        let rms_before = calculate_rms(&buffer2, 0);

        eq.process(&mut buffer2);

        let rms_after = calculate_rms(&buffer2, 0);
        let gain_ratio = rms_after / rms_before;

        // Should now have 12dB boost
        assert!(
            gain_ratio > 3.0,
            "Gain should be applied after band modification, got {}",
            gain_ratio
        );
    }
}
