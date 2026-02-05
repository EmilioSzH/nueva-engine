//! Parametric EQ Effect
//!
//! Multi-band parametric equalizer using biquad filters.
//! Per spec section 4.2.2.

use crate::dsp::effect::{Effect, EffectParams};
use crate::engine::AudioBuffer;
use crate::error::{NuevaError, Result};
use crate::impl_effect_common;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::f32::consts::PI;

// ============================================================================
// Constants
// ============================================================================

/// Maximum number of EQ bands allowed
const MAX_BANDS: usize = 8;

/// Minimum frequency (Hz)
const MIN_FREQUENCY: f32 = 20.0;

/// Maximum frequency (Hz)
const MAX_FREQUENCY: f32 = 20000.0;

/// Minimum gain (dB)
const MIN_GAIN_DB: f32 = -24.0;

/// Maximum gain (dB)
const MAX_GAIN_DB: f32 = 24.0;

/// Minimum Q factor
const MIN_Q: f32 = 0.1;

/// Maximum Q factor
const MAX_Q: f32 = 10.0;

/// Default sample rate for coefficient calculation
const DEFAULT_SAMPLE_RATE: f32 = 48000.0;

// ============================================================================
// Filter Type
// ============================================================================

/// Type of filter for an EQ band
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum FilterType {
    /// Bell curve boost/cut (parametric)
    Peak,
    /// Boost/cut below frequency
    LowShelf,
    /// Boost/cut above frequency
    HighShelf,
    /// Remove frequencies above cutoff
    LowPass,
    /// Remove frequencies below cutoff
    HighPass,
}

impl Default for FilterType {
    fn default() -> Self {
        FilterType::Peak
    }
}

// ============================================================================
// EQ Band
// ============================================================================

/// A single band in the parametric EQ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EQBand {
    /// Center/cutoff frequency in Hz (20 - 20000)
    pub frequency: f32,
    /// Gain in dB (-24 to +24)
    pub gain_db: f32,
    /// Q factor / bandwidth (0.1 to 10.0)
    pub q: f32,
    /// Type of filter
    pub filter_type: FilterType,
    /// Whether this band is active
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
    /// Create a new EQ band with the given parameters
    pub fn new(frequency: f32, gain_db: f32, q: f32, filter_type: FilterType) -> Self {
        Self {
            frequency: frequency.clamp(MIN_FREQUENCY, MAX_FREQUENCY),
            gain_db: gain_db.clamp(MIN_GAIN_DB, MAX_GAIN_DB),
            q: q.clamp(MIN_Q, MAX_Q),
            filter_type,
            enabled: true,
        }
    }

    /// Create a peak (bell) filter band
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

    /// Create a low pass filter band
    pub fn low_pass(frequency: f32, q: f32) -> Self {
        Self::new(frequency, 0.0, q, FilterType::LowPass)
    }

    /// Create a high pass filter band
    pub fn high_pass(frequency: f32, q: f32) -> Self {
        Self::new(frequency, 0.0, q, FilterType::HighPass)
    }

    /// Validate the band parameters
    pub fn validate(&self) -> Result<()> {
        if self.frequency < MIN_FREQUENCY || self.frequency > MAX_FREQUENCY {
            return Err(NuevaError::ProcessingError {
                reason: format!(
                    "Frequency {} Hz out of range ({} - {} Hz)",
                    self.frequency, MIN_FREQUENCY, MAX_FREQUENCY
                ),
            });
        }
        if self.gain_db < MIN_GAIN_DB || self.gain_db > MAX_GAIN_DB {
            return Err(NuevaError::ProcessingError {
                reason: format!(
                    "Gain {} dB out of range ({} - {} dB)",
                    self.gain_db, MIN_GAIN_DB, MAX_GAIN_DB
                ),
            });
        }
        if self.q < MIN_Q || self.q > MAX_Q {
            return Err(NuevaError::ProcessingError {
                reason: format!("Q {} out of range ({} - {})", self.q, MIN_Q, MAX_Q),
            });
        }
        Ok(())
    }
}

// ============================================================================
// Biquad Filter
// ============================================================================

/// A second-order IIR (biquad) filter
///
/// Transfer function:
/// H(z) = (b0 + b1*z^-1 + b2*z^-2) / (a0 + a1*z^-1 + a2*z^-2)
///
/// Direct Form I implementation:
/// y[n] = (b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]) / a0
#[derive(Debug, Clone)]
struct BiquadFilter {
    // Coefficients
    b0: f32,
    b1: f32,
    b2: f32,
    a0: f32,
    a1: f32,
    a2: f32,
    // Input history
    x1: f32,
    x2: f32,
    // Output history
    y1: f32,
    y2: f32,
}

impl Default for BiquadFilter {
    fn default() -> Self {
        Self {
            // Unity pass-through
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a0: 1.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }
}

impl BiquadFilter {
    /// Create a new biquad filter with unity gain (pass-through)
    fn new() -> Self {
        Self::default()
    }

    /// Reset filter state (clear history)
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    /// Process a single sample through the filter
    #[inline]
    fn process_sample(&mut self, input: f32) -> f32 {
        // Direct Form I implementation
        let output = (self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2)
            / self.a0;

        // Update history
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;

        output
    }

    /// Calculate coefficients for a peak (bell) filter
    ///
    /// Audio EQ Cookbook formulas
    fn calculate_peak(&mut self, freq: f32, gain_db: f32, q: f32, sample_rate: f32) {
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);
        let a = 10.0_f32.powf(gain_db / 40.0);

        self.b0 = 1.0 + alpha * a;
        self.b1 = -2.0 * cos_w0;
        self.b2 = 1.0 - alpha * a;
        self.a0 = 1.0 + alpha / a;
        self.a1 = -2.0 * cos_w0;
        self.a2 = 1.0 - alpha / a;
    }

    /// Calculate coefficients for a low shelf filter
    fn calculate_low_shelf(&mut self, freq: f32, gain_db: f32, q: f32, sample_rate: f32) {
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let a = 10.0_f32.powf(gain_db / 40.0);
        let alpha = sin_w0 / (2.0 * q);
        let sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        self.b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + sqrt_a_alpha);
        self.b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
        self.b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - sqrt_a_alpha);
        self.a0 = (a + 1.0) + (a - 1.0) * cos_w0 + sqrt_a_alpha;
        self.a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
        self.a2 = (a + 1.0) + (a - 1.0) * cos_w0 - sqrt_a_alpha;
    }

    /// Calculate coefficients for a high shelf filter
    fn calculate_high_shelf(&mut self, freq: f32, gain_db: f32, q: f32, sample_rate: f32) {
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let a = 10.0_f32.powf(gain_db / 40.0);
        let alpha = sin_w0 / (2.0 * q);
        let sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        self.b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + sqrt_a_alpha);
        self.b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
        self.b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - sqrt_a_alpha);
        self.a0 = (a + 1.0) - (a - 1.0) * cos_w0 + sqrt_a_alpha;
        self.a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
        self.a2 = (a + 1.0) - (a - 1.0) * cos_w0 - sqrt_a_alpha;
    }

    /// Calculate coefficients for a low pass filter
    fn calculate_low_pass(&mut self, freq: f32, q: f32, sample_rate: f32) {
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        self.b0 = (1.0 - cos_w0) / 2.0;
        self.b1 = 1.0 - cos_w0;
        self.b2 = (1.0 - cos_w0) / 2.0;
        self.a0 = 1.0 + alpha;
        self.a1 = -2.0 * cos_w0;
        self.a2 = 1.0 - alpha;
    }

    /// Calculate coefficients for a high pass filter
    fn calculate_high_pass(&mut self, freq: f32, q: f32, sample_rate: f32) {
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        self.b0 = (1.0 + cos_w0) / 2.0;
        self.b1 = -(1.0 + cos_w0);
        self.b2 = (1.0 + cos_w0) / 2.0;
        self.a0 = 1.0 + alpha;
        self.a1 = -2.0 * cos_w0;
        self.a2 = 1.0 - alpha;
    }

    /// Calculate coefficients based on band parameters
    fn calculate_from_band(&mut self, band: &EQBand, sample_rate: f32) {
        match band.filter_type {
            FilterType::Peak => {
                self.calculate_peak(band.frequency, band.gain_db, band.q, sample_rate);
            }
            FilterType::LowShelf => {
                self.calculate_low_shelf(band.frequency, band.gain_db, band.q, sample_rate);
            }
            FilterType::HighShelf => {
                self.calculate_high_shelf(band.frequency, band.gain_db, band.q, sample_rate);
            }
            FilterType::LowPass => {
                self.calculate_low_pass(band.frequency, band.q, sample_rate);
            }
            FilterType::HighPass => {
                self.calculate_high_pass(band.frequency, band.q, sample_rate);
            }
        }
    }
}

// ============================================================================
// Parametric EQ
// ============================================================================

/// Multi-band parametric equalizer
///
/// Supports up to 8 bands, each with independent filter type, frequency,
/// gain, and Q settings. Uses cascaded biquad filters for each band.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParametricEQ {
    /// Common effect parameters
    params: EffectParams,
    /// EQ bands configuration
    bands: Vec<EQBand>,
    /// Biquad filters - one per band per channel
    /// Layout: [band0_ch0, band0_ch1, band1_ch0, band1_ch1, ...]
    #[serde(skip)]
    biquads: Vec<BiquadFilter>,
    /// Current sample rate
    #[serde(skip)]
    sample_rate: f32,
    /// Number of channels
    #[serde(skip)]
    num_channels: usize,
}

impl Default for ParametricEQ {
    fn default() -> Self {
        Self::new()
    }
}

impl ParametricEQ {
    /// Create a new empty parametric EQ
    pub fn new() -> Self {
        Self {
            params: EffectParams::default(),
            bands: Vec::new(),
            biquads: Vec::new(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            num_channels: 2,
        }
    }

    /// Create a parametric EQ with the given bands
    pub fn with_bands(bands: Vec<EQBand>) -> Self {
        let mut eq = Self::new();
        for band in bands {
            let _ = eq.add_band(band);
        }
        eq
    }

    /// Add a new band to the EQ
    ///
    /// Returns the index of the new band, or an error if max bands reached.
    pub fn add_band(&mut self, band: EQBand) -> Result<usize> {
        if self.bands.len() >= MAX_BANDS {
            return Err(NuevaError::ProcessingError {
                reason: format!("Maximum of {} EQ bands reached", MAX_BANDS),
            });
        }

        band.validate()?;
        self.bands.push(band);
        self.recalculate_coefficients();
        Ok(self.bands.len() - 1)
    }

    /// Remove a band from the EQ
    ///
    /// Returns the removed band, or None if index is out of bounds.
    pub fn remove_band(&mut self, index: usize) -> Option<EQBand> {
        if index >= self.bands.len() {
            return None;
        }
        let band = self.bands.remove(index);
        self.recalculate_coefficients();
        Some(band)
    }

    /// Get a reference to a band
    pub fn get_band(&self, index: usize) -> Option<&EQBand> {
        self.bands.get(index)
    }

    /// Update a band's parameters
    pub fn set_band(&mut self, index: usize, band: EQBand) -> Result<()> {
        if index >= self.bands.len() {
            return Err(NuevaError::ProcessingError {
                reason: format!("Band index {} out of range (0-{})", index, self.bands.len()),
            });
        }
        band.validate()?;
        self.bands[index] = band;
        self.recalculate_coefficients();
        Ok(())
    }

    /// Get the number of bands
    pub fn num_bands(&self) -> usize {
        self.bands.len()
    }

    /// Recalculate filter coefficients for all bands
    ///
    /// Should be called after any parameter change.
    pub fn recalculate_coefficients(&mut self) {
        // Ensure we have the right number of biquads
        let needed_biquads = self.bands.len() * self.num_channels;
        self.biquads.resize_with(needed_biquads, BiquadFilter::new);

        // Calculate coefficients for each band
        for (band_idx, band) in self.bands.iter().enumerate() {
            for ch in 0..self.num_channels {
                let biquad_idx = band_idx * self.num_channels + ch;
                if let Some(biquad) = self.biquads.get_mut(biquad_idx) {
                    biquad.calculate_from_band(band, self.sample_rate);
                }
            }
        }
    }

    /// Get all bands
    pub fn bands(&self) -> &[EQBand] {
        &self.bands
    }

    /// Get mutable access to all bands
    ///
    /// Note: Call recalculate_coefficients() after modifying bands.
    pub fn bands_mut(&mut self) -> &mut Vec<EQBand> {
        &mut self.bands
    }
}

impl Effect for ParametricEQ {
    impl_effect_common!(ParametricEQ, "parametric_eq", "Parametric EQ");

    fn process(&mut self, buffer: &mut AudioBuffer) {
        if !self.params.enabled || self.bands.is_empty() {
            return;
        }

        let num_channels = buffer.num_channels();
        let num_samples = buffer.num_samples();

        // Ensure biquads are set up for the right number of channels
        if num_channels != self.num_channels {
            self.num_channels = num_channels;
            self.recalculate_coefficients();
        }

        // Process each sample through each enabled band's filter
        for sample_idx in 0..num_samples {
            for ch in 0..num_channels {
                let mut sample = buffer.samples[ch][sample_idx];

                // Process through each enabled band
                for (band_idx, band) in self.bands.iter().enumerate() {
                    if band.enabled {
                        let biquad_idx = band_idx * self.num_channels + ch;
                        if let Some(biquad) = self.biquads.get_mut(biquad_idx) {
                            sample = biquad.process_sample(sample);
                        }
                    }
                }

                buffer.samples[ch][sample_idx] = sample;
            }
        }
    }

    fn prepare(&mut self, sample_rate: u32, _max_block_size: usize) {
        self.sample_rate = sample_rate as f32;
        self.recalculate_coefficients();
    }

    fn reset(&mut self) {
        for biquad in &mut self.biquads {
            biquad.reset();
        }
    }

    fn to_json(&self) -> Result<Value> {
        serde_json::to_value(self).map_err(|e| NuevaError::Serialization(e))
    }

    fn from_json(&mut self, json: &Value) -> Result<()> {
        let eq: ParametricEQ =
            serde_json::from_value(json.clone()).map_err(|e| NuevaError::Serialization(e))?;
        self.params = eq.params;
        self.bands = eq.bands;
        self.recalculate_coefficients();
        Ok(())
    }

    fn get_params(&self) -> Value {
        json!({
            "id": self.params.id,
            "enabled": self.params.enabled,
            "bands": self.bands.iter().enumerate().map(|(i, band)| {
                json!({
                    "index": i,
                    "frequency": band.frequency,
                    "gain_db": band.gain_db,
                    "q": band.q,
                    "filter_type": format!("{:?}", band.filter_type),
                    "enabled": band.enabled
                })
            }).collect::<Vec<_>>()
        })
    }

    fn set_param(&mut self, name: &str, value: &Value) -> Result<()> {
        match name {
            "enabled" => {
                if let Some(enabled) = value.as_bool() {
                    self.params.enabled = enabled;
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: "enabled must be a boolean".to_string(),
                    })
                }
            }
            "bands" => {
                if let Some(bands_arr) = value.as_array() {
                    self.bands.clear();
                    for band_json in bands_arr {
                        let band: EQBand = serde_json::from_value(band_json.clone())
                            .map_err(|e| NuevaError::Serialization(e))?;
                        self.bands.push(band);
                    }
                    self.recalculate_coefficients();
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: "bands must be an array".to_string(),
                    })
                }
            }
            _ => {
                // Try to parse band parameter (e.g., "band.0.frequency")
                if name.starts_with("band.") {
                    let parts: Vec<&str> = name.split('.').collect();
                    if parts.len() == 3 {
                        if let Ok(band_idx) = parts[1].parse::<usize>() {
                            if band_idx < self.bands.len() {
                                match parts[2] {
                                    "frequency" => {
                                        if let Some(freq) = value.as_f64() {
                                            self.bands[band_idx].frequency =
                                                (freq as f32).clamp(MIN_FREQUENCY, MAX_FREQUENCY);
                                            self.recalculate_coefficients();
                                            return Ok(());
                                        }
                                    }
                                    "gain_db" => {
                                        if let Some(gain) = value.as_f64() {
                                            self.bands[band_idx].gain_db =
                                                (gain as f32).clamp(MIN_GAIN_DB, MAX_GAIN_DB);
                                            self.recalculate_coefficients();
                                            return Ok(());
                                        }
                                    }
                                    "q" => {
                                        if let Some(q) = value.as_f64() {
                                            self.bands[band_idx].q = (q as f32).clamp(MIN_Q, MAX_Q);
                                            self.recalculate_coefficients();
                                            return Ok(());
                                        }
                                    }
                                    "enabled" => {
                                        if let Some(enabled) = value.as_bool() {
                                            self.bands[band_idx].enabled = enabled;
                                            return Ok(());
                                        }
                                    }
                                    "filter_type" => {
                                        if let Some(type_str) = value.as_str() {
                                            self.bands[band_idx].filter_type = match type_str {
                                                "Peak" => FilterType::Peak,
                                                "LowShelf" => FilterType::LowShelf,
                                                "HighShelf" => FilterType::HighShelf,
                                                "LowPass" => FilterType::LowPass,
                                                "HighPass" => FilterType::HighPass,
                                                _ => {
                                                    return Err(NuevaError::ProcessingError {
                                                        reason: format!(
                                                            "Unknown filter type: {}",
                                                            type_str
                                                        ),
                                                    })
                                                }
                                            };
                                            self.recalculate_coefficients();
                                            return Ok(());
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                Err(NuevaError::ProcessingError {
                    reason: format!("Unknown parameter: {}", name),
                })
            }
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

    // Helper to create a test buffer with a sine wave
    fn create_sine_buffer(
        frequency: f32,
        amplitude: f32,
        duration_secs: f32,
        sample_rate: u32,
    ) -> AudioBuffer {
        let num_samples = (duration_secs * sample_rate as f32) as usize;
        let mut buffer = AudioBuffer::new(num_samples, ChannelLayout::Stereo);

        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let sample = amplitude * (2.0 * PI * frequency * t).sin();
            buffer.samples[0][i] = sample;
            buffer.samples[1][i] = sample;
        }

        buffer
    }

    // Helper to calculate RMS of a buffer
    fn calculate_rms(buffer: &AudioBuffer) -> f32 {
        let sum: f32 = buffer.samples[0].iter().map(|s| s * s).sum();
        (sum / buffer.num_samples() as f32).sqrt()
    }

    #[test]
    fn test_eq_band_default() {
        let band = EQBand::default();
        assert_eq!(band.frequency, 1000.0);
        assert_eq!(band.gain_db, 0.0);
        assert_eq!(band.q, 1.0);
        assert_eq!(band.filter_type, FilterType::Peak);
        assert!(band.enabled);
    }

    #[test]
    fn test_eq_band_validation() {
        // Valid band
        let band = EQBand::new(1000.0, 6.0, 1.0, FilterType::Peak);
        assert!(band.validate().is_ok());

        // Invalid frequency (too low)
        let band = EQBand {
            frequency: 10.0,
            ..EQBand::default()
        };
        assert!(band.validate().is_err());

        // Invalid frequency (too high)
        let band = EQBand {
            frequency: 25000.0,
            ..EQBand::default()
        };
        assert!(band.validate().is_err());

        // Invalid gain
        let band = EQBand {
            gain_db: 30.0,
            ..EQBand::default()
        };
        assert!(band.validate().is_err());

        // Invalid Q
        let band = EQBand {
            q: 0.05,
            ..EQBand::default()
        };
        assert!(band.validate().is_err());
    }

    #[test]
    fn test_eq_band_constructors() {
        let peak = EQBand::peak(1000.0, 6.0, 1.0);
        assert_eq!(peak.filter_type, FilterType::Peak);

        let low_shelf = EQBand::low_shelf(100.0, 3.0, 0.7);
        assert_eq!(low_shelf.filter_type, FilterType::LowShelf);

        let high_shelf = EQBand::high_shelf(8000.0, -3.0, 0.7);
        assert_eq!(high_shelf.filter_type, FilterType::HighShelf);

        let low_pass = EQBand::low_pass(5000.0, 0.7);
        assert_eq!(low_pass.filter_type, FilterType::LowPass);

        let high_pass = EQBand::high_pass(80.0, 0.7);
        assert_eq!(high_pass.filter_type, FilterType::HighPass);
    }

    #[test]
    fn test_parametric_eq_new() {
        let eq = ParametricEQ::new();
        assert_eq!(eq.num_bands(), 0);
        assert!(eq.is_enabled());
    }

    #[test]
    fn test_parametric_eq_with_bands() {
        let bands = vec![
            EQBand::peak(1000.0, 6.0, 1.0),
            EQBand::low_shelf(100.0, 3.0, 0.7),
        ];
        let eq = ParametricEQ::with_bands(bands);
        assert_eq!(eq.num_bands(), 2);
    }

    #[test]
    fn test_add_band() {
        let mut eq = ParametricEQ::new();

        // Add bands up to max
        for i in 0..MAX_BANDS {
            let result = eq.add_band(EQBand::peak(1000.0 + i as f32 * 100.0, 3.0, 1.0));
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), i);
        }

        // Try to add one more - should fail
        let result = eq.add_band(EQBand::peak(5000.0, 3.0, 1.0));
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_band() {
        let mut eq = ParametricEQ::with_bands(vec![
            EQBand::peak(500.0, 3.0, 1.0),
            EQBand::peak(1000.0, 6.0, 1.0),
            EQBand::peak(2000.0, -3.0, 1.0),
        ]);

        assert_eq!(eq.num_bands(), 3);

        // Remove middle band
        let removed = eq.remove_band(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().frequency, 1000.0);
        assert_eq!(eq.num_bands(), 2);

        // Remove out of bounds
        let removed = eq.remove_band(10);
        assert!(removed.is_none());
    }

    #[test]
    fn test_get_set_band() {
        let mut eq = ParametricEQ::with_bands(vec![EQBand::peak(1000.0, 6.0, 1.0)]);

        // Get band
        let band = eq.get_band(0);
        assert!(band.is_some());
        assert_eq!(band.unwrap().frequency, 1000.0);

        // Get out of bounds
        assert!(eq.get_band(5).is_none());

        // Set band
        let result = eq.set_band(0, EQBand::peak(2000.0, 3.0, 0.5));
        assert!(result.is_ok());
        assert_eq!(eq.get_band(0).unwrap().frequency, 2000.0);

        // Set out of bounds
        let result = eq.set_band(5, EQBand::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_biquad_passthrough() {
        // With 0 dB gain, peak filter should be unity
        let mut biquad = BiquadFilter::new();
        biquad.calculate_peak(1000.0, 0.0, 1.0, 48000.0);

        let input = 0.5;
        let output = biquad.process_sample(input);

        // Should be very close to input
        assert!((output - input).abs() < 0.01);
    }

    #[test]
    fn test_process_passthrough_empty_eq() {
        let mut eq = ParametricEQ::new();
        eq.prepare(48000, 512);

        let mut buffer = create_sine_buffer(1000.0, 0.5, 0.1, 48000);
        let original_rms = calculate_rms(&buffer);

        eq.process(&mut buffer);

        let processed_rms = calculate_rms(&buffer);
        assert!((original_rms - processed_rms).abs() < 0.001);
    }

    #[test]
    fn test_process_disabled() {
        let mut eq = ParametricEQ::with_bands(vec![EQBand::peak(1000.0, 12.0, 1.0)]);
        eq.prepare(48000, 512);
        eq.set_enabled(false);

        let mut buffer = create_sine_buffer(1000.0, 0.5, 0.1, 48000);
        let original_rms = calculate_rms(&buffer);

        eq.process(&mut buffer);

        let processed_rms = calculate_rms(&buffer);
        assert!((original_rms - processed_rms).abs() < 0.001);
    }

    #[test]
    fn test_process_boost() {
        let mut eq = ParametricEQ::with_bands(vec![EQBand::peak(1000.0, 12.0, 1.0)]);
        eq.prepare(48000, 512);

        let mut buffer = create_sine_buffer(1000.0, 0.25, 0.1, 48000);
        let original_rms = calculate_rms(&buffer);

        eq.process(&mut buffer);

        let processed_rms = calculate_rms(&buffer);
        // 12 dB boost should increase RMS significantly
        assert!(processed_rms > original_rms * 2.0);
    }

    #[test]
    fn test_process_cut() {
        let mut eq = ParametricEQ::with_bands(vec![EQBand::peak(1000.0, -12.0, 1.0)]);
        eq.prepare(48000, 512);

        let mut buffer = create_sine_buffer(1000.0, 0.5, 0.1, 48000);
        let original_rms = calculate_rms(&buffer);

        eq.process(&mut buffer);

        let processed_rms = calculate_rms(&buffer);
        // 12 dB cut should decrease RMS significantly
        assert!(processed_rms < original_rms * 0.5);
    }

    #[test]
    fn test_high_pass_filter() {
        let mut eq = ParametricEQ::with_bands(vec![EQBand::high_pass(500.0, 0.7)]);
        eq.prepare(48000, 512);

        // Low frequency signal (100 Hz) should be attenuated
        let mut buffer = create_sine_buffer(100.0, 0.5, 0.1, 48000);
        let original_rms = calculate_rms(&buffer);

        eq.process(&mut buffer);

        let processed_rms = calculate_rms(&buffer);
        // High pass at 500 Hz should significantly attenuate 100 Hz
        assert!(processed_rms < original_rms * 0.5);
    }

    #[test]
    fn test_low_pass_filter() {
        let mut eq = ParametricEQ::with_bands(vec![EQBand::low_pass(500.0, 0.7)]);
        eq.prepare(48000, 512);

        // High frequency signal (2000 Hz) should be attenuated
        let mut buffer = create_sine_buffer(2000.0, 0.5, 0.1, 48000);
        let original_rms = calculate_rms(&buffer);

        eq.process(&mut buffer);

        let processed_rms = calculate_rms(&buffer);
        // Low pass at 500 Hz should significantly attenuate 2000 Hz
        assert!(processed_rms < original_rms * 0.5);
    }

    #[test]
    fn test_reset() {
        let mut eq = ParametricEQ::with_bands(vec![EQBand::peak(1000.0, 6.0, 1.0)]);
        eq.prepare(48000, 512);

        // Process some audio
        let mut buffer = create_sine_buffer(1000.0, 0.5, 0.01, 48000);
        eq.process(&mut buffer);

        // Reset
        eq.reset();

        // Filter state should be cleared (tested implicitly by processing again)
        let mut buffer2 = create_sine_buffer(1000.0, 0.5, 0.01, 48000);
        eq.process(&mut buffer2);
        // No assertion needed - just verify it doesn't crash
    }

    #[test]
    fn test_effect_trait_methods() {
        let mut eq = ParametricEQ::with_bands(vec![EQBand::peak(1000.0, 6.0, 1.0)]);

        // Test effect type
        assert_eq!(eq.effect_type(), "parametric_eq");

        // Test display name
        assert_eq!(eq.display_name(), "Parametric EQ");

        // Test enabled state
        assert!(eq.is_enabled());
        eq.set_enabled(false);
        assert!(!eq.is_enabled());
        eq.set_enabled(true);

        // Test ID
        let id = eq.id().to_string();
        assert!(!id.is_empty());
        eq.set_id("custom-id".to_string());
        assert_eq!(eq.id(), "custom-id");
    }

    #[test]
    fn test_serialization() {
        let eq = ParametricEQ::with_bands(vec![
            EQBand::peak(1000.0, 6.0, 1.0),
            EQBand::low_shelf(100.0, 3.0, 0.7),
        ]);

        // Serialize to JSON
        let json = eq.to_json().unwrap();

        // Deserialize
        let mut eq2 = ParametricEQ::new();
        eq2.from_json(&json).unwrap();

        assert_eq!(eq2.num_bands(), 2);
        assert_eq!(eq2.get_band(0).unwrap().frequency, 1000.0);
        assert_eq!(eq2.get_band(1).unwrap().filter_type, FilterType::LowShelf);
    }

    #[test]
    fn test_get_params() {
        let eq = ParametricEQ::with_bands(vec![EQBand::peak(1000.0, 6.0, 1.0)]);
        let params = eq.get_params();

        assert!(params.get("id").is_some());
        assert!(params.get("enabled").is_some());
        assert!(params.get("bands").is_some());

        let bands = params.get("bands").unwrap().as_array().unwrap();
        assert_eq!(bands.len(), 1);
        assert_eq!(bands[0].get("frequency").unwrap().as_f64().unwrap(), 1000.0);
    }

    #[test]
    fn test_set_param() {
        let mut eq = ParametricEQ::with_bands(vec![EQBand::peak(1000.0, 6.0, 1.0)]);

        // Set enabled
        eq.set_param("enabled", &json!(false)).unwrap();
        assert!(!eq.is_enabled());

        // Set band frequency
        eq.set_param("band.0.frequency", &json!(2000.0)).unwrap();
        assert_eq!(eq.get_band(0).unwrap().frequency, 2000.0);

        // Set band gain
        eq.set_param("band.0.gain_db", &json!(-6.0)).unwrap();
        assert_eq!(eq.get_band(0).unwrap().gain_db, -6.0);

        // Set band Q
        eq.set_param("band.0.q", &json!(2.0)).unwrap();
        assert_eq!(eq.get_band(0).unwrap().q, 2.0);

        // Set band enabled
        eq.set_param("band.0.enabled", &json!(false)).unwrap();
        assert!(!eq.get_band(0).unwrap().enabled);

        // Set filter type
        eq.set_param("band.0.filter_type", &json!("HighShelf"))
            .unwrap();
        assert_eq!(eq.get_band(0).unwrap().filter_type, FilterType::HighShelf);

        // Invalid parameter
        let result = eq.set_param("invalid", &json!(true));
        assert!(result.is_err());
    }

    #[test]
    fn test_box_clone() {
        let eq = ParametricEQ::with_bands(vec![EQBand::peak(1000.0, 6.0, 1.0)]);
        let boxed: Box<dyn Effect> = Box::new(eq);
        let cloned = boxed.box_clone();

        assert_eq!(cloned.effect_type(), "parametric_eq");
    }

    #[test]
    fn test_band_disabled() {
        let mut eq = ParametricEQ::with_bands(vec![EQBand::peak(1000.0, 12.0, 1.0)]);
        eq.prepare(48000, 512);

        // Disable the band
        eq.bands_mut()[0].enabled = false;

        let mut buffer = create_sine_buffer(1000.0, 0.5, 0.1, 48000);
        let original_rms = calculate_rms(&buffer);

        eq.process(&mut buffer);

        let processed_rms = calculate_rms(&buffer);
        // Disabled band should pass through
        assert!((original_rms - processed_rms).abs() < 0.001);
    }

    #[test]
    fn test_multiple_bands() {
        let mut eq = ParametricEQ::with_bands(vec![
            EQBand::high_pass(80.0, 0.7),
            EQBand::peak(250.0, -3.0, 1.0),
            EQBand::peak(1000.0, 2.0, 1.0),
            EQBand::peak(4000.0, 3.0, 1.5),
            EQBand::high_shelf(8000.0, 2.0, 0.7),
        ]);
        eq.prepare(48000, 512);

        let mut buffer = create_sine_buffer(1000.0, 0.5, 0.1, 48000);
        eq.process(&mut buffer);

        // Just verify it processes without error
        assert!(buffer.is_finite());
    }

    #[test]
    fn test_stereo_processing() {
        let mut eq = ParametricEQ::with_bands(vec![EQBand::peak(1000.0, 6.0, 1.0)]);
        eq.prepare(48000, 512);

        let mut buffer = create_sine_buffer(1000.0, 0.5, 0.1, 48000);
        // Make channels different
        for i in 0..buffer.num_samples() {
            buffer.samples[1][i] *= 0.5;
        }

        let left_rms_before = (buffer.samples[0].iter().map(|s| s * s).sum::<f32>()
            / buffer.num_samples() as f32)
            .sqrt();
        let right_rms_before = (buffer.samples[1].iter().map(|s| s * s).sum::<f32>()
            / buffer.num_samples() as f32)
            .sqrt();

        eq.process(&mut buffer);

        let left_rms_after = (buffer.samples[0].iter().map(|s| s * s).sum::<f32>()
            / buffer.num_samples() as f32)
            .sqrt();
        let right_rms_after = (buffer.samples[1].iter().map(|s| s * s).sum::<f32>()
            / buffer.num_samples() as f32)
            .sqrt();

        // Both channels should be boosted
        assert!(left_rms_after > left_rms_before);
        assert!(right_rms_after > right_rms_before);
        // The ratio should be preserved (approximately)
        let ratio_before = left_rms_before / right_rms_before;
        let ratio_after = left_rms_after / right_rms_after;
        assert!((ratio_before - ratio_after).abs() < 0.5);
    }

    #[test]
    fn test_low_shelf() {
        let mut eq = ParametricEQ::with_bands(vec![EQBand::low_shelf(200.0, 6.0, 0.7)]);
        eq.prepare(48000, 512);

        // Low frequency should be boosted
        let mut buffer_low = create_sine_buffer(50.0, 0.3, 0.1, 48000);
        let rms_low_before = calculate_rms(&buffer_low);
        eq.process(&mut buffer_low);
        eq.reset();
        let rms_low_after = calculate_rms(&buffer_low);

        // High frequency should pass through mostly unchanged
        let mut buffer_high = create_sine_buffer(2000.0, 0.3, 0.1, 48000);
        let rms_high_before = calculate_rms(&buffer_high);
        eq.process(&mut buffer_high);
        let rms_high_after = calculate_rms(&buffer_high);

        // Low shelf should boost low frequencies more than high frequencies
        let low_boost_ratio = rms_low_after / rms_low_before;
        let high_boost_ratio = rms_high_after / rms_high_before;
        assert!(low_boost_ratio > high_boost_ratio);
    }

    #[test]
    fn test_high_shelf() {
        let mut eq = ParametricEQ::with_bands(vec![EQBand::high_shelf(2000.0, 6.0, 0.7)]);
        eq.prepare(48000, 512);

        // High frequency should be boosted
        let mut buffer_high = create_sine_buffer(8000.0, 0.3, 0.1, 48000);
        let rms_high_before = calculate_rms(&buffer_high);
        eq.process(&mut buffer_high);
        eq.reset();
        let rms_high_after = calculate_rms(&buffer_high);

        // Low frequency should pass through mostly unchanged
        let mut buffer_low = create_sine_buffer(100.0, 0.3, 0.1, 48000);
        let rms_low_before = calculate_rms(&buffer_low);
        eq.process(&mut buffer_low);
        let rms_low_after = calculate_rms(&buffer_low);

        // High shelf should boost high frequencies more than low frequencies
        let high_boost_ratio = rms_high_after / rms_high_before;
        let low_boost_ratio = rms_low_after / rms_low_before;
        assert!(high_boost_ratio > low_boost_ratio);
    }
}
