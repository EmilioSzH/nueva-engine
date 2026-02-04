//! Audio verification utilities
//!
//! Provides measurements for audio quality testing without requiring
//! manual listening. All measurements are objective and automated.
//!
//! # Measurements
//! - RMS (Root Mean Square) level
//! - Peak level
//! - Crest factor (peak/RMS ratio)
//! - Clipping detection
//! - DC offset detection
//! - Stereo correlation (phase coherence)
//! - Spectral analysis (FFT)

use crate::audio::AudioBuffer;
use rustfft::{num_complex::Complex, FftPlanner};

/// Threshold for considering a sample as clipped (at digital maximum)
const CLIP_THRESHOLD: f32 = 0.9999;

/// Minimum correlation before phase issues are flagged
const MIN_PHASE_CORRELATION: f32 = 0.2;

/// Convert linear amplitude to decibels
pub fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * linear.log10()
    }
}

/// Convert decibels to linear amplitude
pub fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Complete audio analysis results
#[derive(Debug, Clone)]
pub struct AudioAnalysis {
    /// RMS level in linear scale (0.0 to 1.0+)
    pub rms_linear: f32,
    /// RMS level in decibels (relative to 0 dBFS)
    pub rms_db: f32,
    /// Peak level in linear scale
    pub peak_linear: f32,
    /// Peak level in decibels
    pub peak_db: f32,
    /// Crest factor (peak/RMS) in dB
    pub crest_factor_db: f32,
    /// Number of clipped samples
    pub clipped_samples: usize,
    /// Percentage of samples that are clipped
    pub clip_percentage: f32,
    /// DC offset (mean of all samples)
    pub dc_offset: f32,
    /// Duration in seconds
    pub duration: f32,
    /// Sample rate
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u16,
    /// Stereo correlation (-1.0 to 1.0), None for mono
    pub stereo_correlation: Option<f32>,
}

impl AudioAnalysis {
    /// Analyze an audio buffer and return comprehensive measurements
    pub fn analyze(buffer: &AudioBuffer) -> Self {
        let samples = buffer.samples();
        let num_samples = samples.len();

        // Calculate RMS
        let rms_linear = calculate_rms(samples);
        let rms_db = linear_to_db(rms_linear);

        // Calculate peak
        let peak_linear = calculate_peak(samples);
        let peak_db = linear_to_db(peak_linear);

        // Calculate crest factor
        let crest_factor_db = if rms_linear > 0.0 {
            peak_db - rms_db
        } else {
            0.0
        };

        // Count clipped samples
        let clipped_samples = count_clipped_samples(samples);
        let clip_percentage = (clipped_samples as f32 / num_samples as f32) * 100.0;

        // Calculate DC offset
        let dc_offset = calculate_dc_offset(samples);

        // Calculate stereo correlation if stereo
        let stereo_correlation = if buffer.channels() == 2 {
            Some(calculate_stereo_correlation(buffer))
        } else {
            None
        };

        Self {
            rms_linear,
            rms_db,
            peak_linear,
            peak_db,
            crest_factor_db,
            clipped_samples,
            clip_percentage,
            dc_offset,
            duration: buffer.duration(),
            sample_rate: buffer.sample_rate(),
            channels: buffer.channels(),
            stereo_correlation,
        }
    }

    /// Check if audio is silent (RMS below threshold)
    pub fn is_silent(&self, threshold_db: f32) -> bool {
        self.rms_db < threshold_db
    }

    /// Check if audio is clipping
    pub fn is_clipping(&self) -> bool {
        self.clip_percentage > 1.0
    }

    /// Check if audio would clip (peak >= 0 dBFS)
    pub fn would_clip(&self) -> bool {
        self.peak_db >= 0.0
    }

    /// Check if DC offset is significant
    pub fn has_dc_offset(&self) -> bool {
        self.dc_offset.abs() > 0.01
    }

    /// Check if stereo has phase issues
    pub fn has_phase_issues(&self) -> bool {
        self.stereo_correlation
            .map(|c| c < MIN_PHASE_CORRELATION)
            .unwrap_or(false)
    }

    /// Generate a summary string for display
    pub fn summary(&self) -> String {
        let mut s = format!(
            "Duration: {:.2}s | {} ch @ {} Hz\n\
             RMS: {:.1} dBFS | Peak: {:.1} dBFS | Crest: {:.1} dB\n\
             DC Offset: {:.4}",
            self.duration,
            self.channels,
            self.sample_rate,
            self.rms_db,
            self.peak_db,
            self.crest_factor_db,
            self.dc_offset
        );

        if self.clipped_samples > 0 {
            s.push_str(&format!(
                "\nClipping: {} samples ({:.2}%)",
                self.clipped_samples, self.clip_percentage
            ));
        }

        if let Some(corr) = self.stereo_correlation {
            s.push_str(&format!("\nStereo Correlation: {:.2}", corr));
        }

        s
    }
}

/// Calculate RMS (Root Mean Square) of samples
pub fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares: f32 = samples.iter().map(|s| s * s).sum();
    (sum_squares / samples.len() as f32).sqrt()
}

/// Calculate RMS in decibels
pub fn calculate_rms_db(samples: &[f32]) -> f32 {
    linear_to_db(calculate_rms(samples))
}

/// Calculate peak (maximum absolute value) of samples
pub fn calculate_peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .map(|s| s.abs())
        .fold(0.0_f32, f32::max)
}

/// Calculate peak in decibels
pub fn calculate_peak_db(samples: &[f32]) -> f32 {
    linear_to_db(calculate_peak(samples))
}

/// Calculate crest factor (peak/RMS ratio) in dB
pub fn calculate_crest_factor(samples: &[f32]) -> f32 {
    let rms = calculate_rms(samples);
    let peak = calculate_peak(samples);
    if rms > 0.0 {
        linear_to_db(peak / rms)
    } else {
        0.0
    }
}

/// Count samples that are clipped (at or near digital maximum)
pub fn count_clipped_samples(samples: &[f32]) -> usize {
    samples.iter().filter(|s| s.abs() >= CLIP_THRESHOLD).count()
}

/// Detect if audio is clipping (more than 1% of samples at maximum)
pub fn detect_clipping(samples: &[f32]) -> bool {
    let clipped = count_clipped_samples(samples);
    let percentage = clipped as f32 / samples.len() as f32;
    percentage > 0.01
}

/// Calculate DC offset (mean of samples)
pub fn calculate_dc_offset(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().sum();
    sum / samples.len() as f32
}

/// Check if DC offset is significant (> 0.01)
pub fn has_significant_dc_offset(samples: &[f32]) -> bool {
    calculate_dc_offset(samples).abs() > 0.01
}

/// Calculate stereo correlation (-1.0 = opposite phase, 0.0 = uncorrelated, 1.0 = identical)
pub fn calculate_stereo_correlation(buffer: &AudioBuffer) -> f32 {
    if buffer.channels() != 2 {
        return 1.0; // Mono is perfectly correlated with itself
    }

    let left = buffer.channel_samples(0);
    let right = buffer.channel_samples(1);

    if left.is_empty() || right.is_empty() {
        return 0.0;
    }

    // Calculate correlation coefficient
    let n = left.len() as f32;
    let sum_l: f32 = left.iter().sum();
    let sum_r: f32 = right.iter().sum();
    let sum_ll: f32 = left.iter().map(|x| x * x).sum();
    let sum_rr: f32 = right.iter().map(|x| x * x).sum();
    let sum_lr: f32 = left.iter().zip(right.iter()).map(|(l, r)| l * r).sum();

    let numerator = n * sum_lr - sum_l * sum_r;
    let denominator = ((n * sum_ll - sum_l * sum_l) * (n * sum_rr - sum_r * sum_r)).sqrt();

    if denominator.abs() < 1e-10 {
        0.0
    } else {
        numerator / denominator
    }
}

/// Spectral analysis result at a specific frequency
#[derive(Debug, Clone)]
pub struct SpectralPeak {
    pub frequency: f32,
    pub magnitude_db: f32,
}

/// Perform FFT analysis and return spectral peaks
pub fn analyze_spectrum(buffer: &AudioBuffer, num_bins: usize) -> Vec<SpectralPeak> {
    let samples = if buffer.channels() == 2 {
        // Mix to mono for spectral analysis
        buffer.channel_samples(0)
            .iter()
            .zip(buffer.channel_samples(1).iter())
            .map(|(l, r)| (l + r) / 2.0)
            .collect::<Vec<_>>()
    } else {
        buffer.samples().to_vec()
    };

    if samples.len() < num_bins {
        return Vec::new();
    }

    // Prepare FFT
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(num_bins);

    // Take first num_bins samples and apply Hann window
    let mut complex_samples: Vec<Complex<f32>> = samples
        .iter()
        .take(num_bins)
        .enumerate()
        .map(|(i, &s)| {
            let window = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / num_bins as f32).cos());
            Complex::new(s * window, 0.0)
        })
        .collect();

    // Run FFT
    fft.process(&mut complex_samples);

    // Convert to magnitude spectrum
    let bin_hz = buffer.sample_rate() as f32 / num_bins as f32;

    complex_samples
        .iter()
        .take(num_bins / 2) // Only positive frequencies
        .enumerate()
        .map(|(i, c)| {
            let magnitude = c.norm() / (num_bins as f32 / 2.0);
            SpectralPeak {
                frequency: i as f32 * bin_hz,
                magnitude_db: linear_to_db(magnitude),
            }
        })
        .collect()
}

/// Get magnitude at a specific frequency (nearest bin)
pub fn magnitude_at_frequency(buffer: &AudioBuffer, frequency: f32, fft_size: usize) -> f32 {
    let spectrum = analyze_spectrum(buffer, fft_size);
    let bin_hz = buffer.sample_rate() as f32 / fft_size as f32;
    let target_bin = (frequency / bin_hz).round() as usize;

    spectrum
        .get(target_bin)
        .map(|p| p.magnitude_db)
        .unwrap_or(f32::NEG_INFINITY)
}

/// Calculate spectral centroid (brightness indicator)
pub fn calculate_spectral_centroid(buffer: &AudioBuffer, fft_size: usize) -> f32 {
    let spectrum = analyze_spectrum(buffer, fft_size);

    if spectrum.is_empty() {
        return 0.0;
    }

    let mut weighted_sum = 0.0;
    let mut magnitude_sum = 0.0;

    for peak in &spectrum {
        let linear_mag = db_to_linear(peak.magnitude_db);
        weighted_sum += peak.frequency * linear_mag;
        magnitude_sum += linear_mag;
    }

    if magnitude_sum > 0.0 {
        weighted_sum / magnitude_sum
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rms_sine_wave() {
        // A sine wave with amplitude 1.0 should have RMS of ~0.707
        let buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);
        let rms = calculate_rms(buffer.samples());
        assert!((rms - 0.707).abs() < 0.01);
    }

    #[test]
    fn test_rms_silence() {
        let buffer = AudioBuffer::silence(1.0, 1, 44100);
        let rms = calculate_rms(buffer.samples());
        assert_eq!(rms, 0.0);
    }

    #[test]
    fn test_peak_sine_wave() {
        let buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);
        let peak = calculate_peak(buffer.samples());
        assert!((peak - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_crest_factor_sine() {
        // Sine wave crest factor should be ~3 dB
        let buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);
        let crest = calculate_crest_factor(buffer.samples());
        assert!((crest - 3.01).abs() < 0.1);
    }

    #[test]
    fn test_dc_offset_detection() {
        let samples = vec![0.1; 44100]; // All samples at 0.1 (DC offset)
        let dc = calculate_dc_offset(&samples);
        assert!((dc - 0.1).abs() < 0.001);
        assert!(has_significant_dc_offset(&samples));

        // Sine wave should have ~0 DC offset
        let buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);
        let dc_sine = calculate_dc_offset(buffer.samples());
        assert!(dc_sine.abs() < 0.01);
    }

    #[test]
    fn test_clipping_detection() {
        // Create clipped audio
        let samples: Vec<f32> = (0..44100)
            .map(|i| {
                let t = i as f32 / 44100.0;
                (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 2.0 // Over-amplified
            })
            .map(|s| s.clamp(-1.0, 1.0))
            .collect();

        assert!(detect_clipping(&samples));

        // Normal audio should not clip
        let normal = AudioBuffer::sine_wave(440.0, 1.0, 44100);
        assert!(!detect_clipping(normal.samples()));
    }

    #[test]
    fn test_stereo_correlation() {
        // Identical channels should have correlation of 1.0
        let samples = vec![0.5, 0.5, -0.5, -0.5, 0.3, 0.3]; // L=R
        let buffer = AudioBuffer::new(samples, 2, 44100).unwrap();
        let corr = calculate_stereo_correlation(&buffer);
        assert!((corr - 1.0).abs() < 0.01);

        // Opposite phase should have correlation of -1.0
        let samples_opposite = vec![0.5, -0.5, -0.5, 0.5, 0.3, -0.3]; // L=-R
        let buffer_opposite = AudioBuffer::new(samples_opposite, 2, 44100).unwrap();
        let corr_opposite = calculate_stereo_correlation(&buffer_opposite);
        assert!((corr_opposite - (-1.0)).abs() < 0.01);
    }

    #[test]
    fn test_db_conversion() {
        assert!((linear_to_db(1.0) - 0.0).abs() < 0.001);
        assert!((linear_to_db(0.5) - (-6.02)).abs() < 0.1);
        assert!((db_to_linear(0.0) - 1.0).abs() < 0.001);
        assert!((db_to_linear(-6.0) - 0.501).abs() < 0.01);
    }

    #[test]
    fn test_audio_analysis() {
        // Use 0.9 amplitude to avoid triggering clip detection
        let mut buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);
        buffer.apply_gain(0.9); // Reduce to 90% amplitude
        let analysis = AudioAnalysis::analyze(&buffer);

        // RMS of 0.9 amplitude sine: 0.9 * 0.707 = 0.636, or about -3.9 dB
        assert!((analysis.rms_db - (-3.9)).abs() < 0.2);
        // Peak of 0.9 amplitude: about -0.9 dB
        assert!((analysis.peak_db - (-0.9)).abs() < 0.2);
        assert!((analysis.crest_factor_db - 3.01).abs() < 0.1);
        assert_eq!(analysis.clipped_samples, 0);
        assert!(analysis.dc_offset.abs() < 0.01);
    }

    #[test]
    fn test_spectral_analysis() {
        // Create 440 Hz sine wave
        let buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);

        // Get magnitude at 440 Hz
        let mag_440 = magnitude_at_frequency(&buffer, 440.0, 4096);

        // Get magnitude at 1000 Hz (should be much lower)
        let mag_1000 = magnitude_at_frequency(&buffer, 1000.0, 4096);

        // 440 Hz should be significantly stronger than 1000 Hz
        assert!(mag_440 > mag_1000 + 20.0); // At least 20 dB difference
    }
}
