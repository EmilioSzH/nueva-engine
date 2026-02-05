//! Audio Buffer Management
//!
//! Provides the core audio buffer type and validation utilities for Nueva.
//! All internal processing uses 48kHz/32-bit float format per spec 3.2.

use crate::error::{NuevaError, Result};

// ============================================================================
// Constants (spec 3.2)
// ============================================================================

/// Internal sample rate for all processing (48kHz)
pub const INTERNAL_SAMPLE_RATE: u32 = 48000;

/// Minimum audio duration in seconds (100ms)
pub const MIN_DURATION_SECS: f64 = 0.1;

/// Maximum audio duration in seconds (2 hours)
pub const MAX_DURATION_SECS: f64 = 7200.0;

/// Threshold below which audio is considered silent (-80dBFS)
pub const SILENCE_THRESHOLD_DB: f32 = -80.0;

/// Maximum acceptable DC offset (mean sample value)
pub const DC_OFFSET_THRESHOLD: f32 = 0.01;

/// Maximum acceptable ratio of clipped samples (1%)
pub const CLIP_RATIO_THRESHOLD: f32 = 0.01;

/// Clipping detection threshold (samples at or above this are clipped)
pub const CLIP_SAMPLE_THRESHOLD: f32 = 1.0;

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert decibels to linear amplitude
///
/// # Arguments
/// * `db` - Value in decibels
///
/// # Returns
/// Linear amplitude (0.0 to 1.0+ range)
#[inline]
pub fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Convert linear amplitude to decibels
///
/// # Arguments
/// * `linear` - Linear amplitude value
///
/// # Returns
/// Value in decibels. Returns -f32::INFINITY for zero input.
#[inline]
pub fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * linear.log10()
    }
}

/// Calculate the RMS (Root Mean Square) level of an audio buffer in dB
///
/// # Arguments
/// * `buffer` - Reference to the AudioBuffer to analyze
///
/// # Returns
/// RMS level in dB. Returns -f32::INFINITY for empty or silent buffers.
pub fn calculate_rms(buffer: &AudioBuffer) -> f32 {
    let total_samples = buffer.num_channels() * buffer.num_samples();
    if total_samples == 0 {
        return f32::NEG_INFINITY;
    }

    let sum_squares: f64 = buffer
        .samples
        .iter()
        .flat_map(|channel| channel.iter())
        .map(|&s| (s as f64) * (s as f64))
        .sum();

    let rms = (sum_squares / total_samples as f64).sqrt() as f32;
    linear_to_db(rms)
}

/// Calculate the peak level of an audio buffer in dB
///
/// # Arguments
/// * `buffer` - Reference to the AudioBuffer to analyze
///
/// # Returns
/// Peak level in dB. Returns -f32::INFINITY for empty buffers.
pub fn calculate_peak(buffer: &AudioBuffer) -> f32 {
    let peak = buffer
        .samples
        .iter()
        .flat_map(|channel| channel.iter())
        .map(|&s| s.abs())
        .fold(0.0_f32, f32::max);

    linear_to_db(peak)
}

/// Calculate the mean (average) sample value of an audio buffer
///
/// Used for DC offset detection.
///
/// # Arguments
/// * `buffer` - Reference to the AudioBuffer to analyze
///
/// # Returns
/// Mean sample value. Returns 0.0 for empty buffers.
pub fn calculate_mean(buffer: &AudioBuffer) -> f32 {
    let total_samples = buffer.num_channels() * buffer.num_samples();
    if total_samples == 0 {
        return 0.0;
    }

    let sum: f64 = buffer
        .samples
        .iter()
        .flat_map(|channel| channel.iter())
        .map(|&s| s as f64)
        .sum();

    (sum / total_samples as f64) as f32
}

/// Calculate the ratio of clipped samples in an audio buffer
///
/// A sample is considered clipped if its absolute value is exactly 1.0 or greater.
///
/// # Arguments
/// * `buffer` - Reference to the AudioBuffer to analyze
///
/// # Returns
/// Ratio of clipped samples (0.0 to 1.0). Returns 0.0 for empty buffers.
pub fn calculate_clip_ratio(buffer: &AudioBuffer) -> f32 {
    let total_samples = buffer.num_channels() * buffer.num_samples();
    if total_samples == 0 {
        return 0.0;
    }

    let clipped_count = buffer
        .samples
        .iter()
        .flat_map(|channel| channel.iter())
        .filter(|&&s| s.abs() >= 1.0)
        .count();

    clipped_count as f32 / total_samples as f32
}

// ============================================================================
// Channel Layout
// ============================================================================

/// Audio channel configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChannelLayout {
    /// Single channel (mono)
    Mono,
    /// Two channels (stereo: left, right)
    #[default]
    Stereo,
}

impl ChannelLayout {
    /// Returns the number of channels for this layout
    pub fn num_channels(&self) -> usize {
        match self {
            ChannelLayout::Mono => 1,
            ChannelLayout::Stereo => 2,
        }
    }

    /// Create a ChannelLayout from a channel count
    pub fn from_count(count: usize) -> Option<Self> {
        match count {
            1 => Some(ChannelLayout::Mono),
            2 => Some(ChannelLayout::Stereo),
            _ => None,
        }
    }
}

// ============================================================================
// Audio Validation
// ============================================================================

/// Results of audio validation checks (spec 3.6)
///
/// Each field represents a validation criterion. All must be true for
/// audio to be considered valid for processing.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioValidation {
    /// File contains actual audio data (not empty)
    pub has_samples: bool,
    /// Duration is within limits (0.1s to 2hr)
    pub reasonable_length: bool,
    /// Can decode without errors (always true for in-memory buffers)
    pub not_corrupt: bool,
    /// RMS level is above silence threshold (-80dBFS)
    pub not_silent: bool,
    /// No significant DC offset (mean < 0.01)
    pub not_dc_offset: bool,
    /// Less than 1% of samples are clipped
    pub not_clipped: bool,
}

impl AudioValidation {
    /// Check if all validation criteria pass
    pub fn is_valid(&self) -> bool {
        self.has_samples
            && self.reasonable_length
            && self.not_corrupt
            && self.not_silent
            && self.not_dc_offset
            && self.not_clipped
    }

    /// Get a list of failed validation criteria
    pub fn failed_checks(&self) -> Vec<&'static str> {
        let mut failures = Vec::new();
        if !self.has_samples {
            failures.push("no samples");
        }
        if !self.reasonable_length {
            failures.push("duration out of range");
        }
        if !self.not_corrupt {
            failures.push("corrupt data");
        }
        if !self.not_silent {
            failures.push("audio is silent");
        }
        if !self.not_dc_offset {
            failures.push("DC offset detected");
        }
        if !self.not_clipped {
            failures.push("excessive clipping");
        }
        failures
    }
}

impl Default for AudioValidation {
    fn default() -> Self {
        Self {
            has_samples: false,
            reasonable_length: false,
            not_corrupt: true,
            not_silent: false,
            not_dc_offset: true,
            not_clipped: true,
        }
    }
}

// ============================================================================
// Audio Buffer
// ============================================================================

/// Core audio buffer type for all audio processing in Nueva
///
/// Stores audio as non-interleaved 32-bit floating point samples.
/// Each channel is a separate Vec<f32>.
///
/// # Internal Format (spec 3.2)
/// - Sample Rate: 48,000 Hz
/// - Bit Depth: 32-bit float (f32)
/// - Channels: Mono or Stereo
///
/// # Example
/// ```
/// use nueva::engine::buffer::{AudioBuffer, ChannelLayout, INTERNAL_SAMPLE_RATE};
///
/// // Create a 1-second stereo buffer
/// let buffer = AudioBuffer::new(INTERNAL_SAMPLE_RATE as usize, ChannelLayout::Stereo);
/// assert_eq!(buffer.channels(), 2);
/// assert_eq!(buffer.len(), 48000);
/// ```
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    /// Sample data: outer Vec is channels, inner Vec is samples
    pub samples: Vec<Vec<f32>>,
    /// Sample rate in Hz (default: 48000)
    pub sample_rate: u32,
}

impl AudioBuffer {
    /// Create a new audio buffer with the specified number of samples and layout
    ///
    /// All samples are initialized to 0.0 (silence).
    ///
    /// # Arguments
    /// * `num_samples` - Number of samples per channel
    /// * `layout` - Channel configuration (Mono or Stereo)
    ///
    /// # Returns
    /// A new AudioBuffer with zeroed samples
    pub fn new(num_samples: usize, layout: ChannelLayout) -> Self {
        let num_channels = layout.num_channels();
        let samples = vec![vec![0.0_f32; num_samples]; num_channels];
        Self {
            samples,
            sample_rate: INTERNAL_SAMPLE_RATE,
        }
    }

    /// Create an audio buffer from interleaved sample data
    ///
    /// # Arguments
    /// * `interleaved` - Interleaved sample data (L, R, L, R, ... for stereo)
    /// * `layout` - Channel configuration
    /// * `sample_rate` - Sample rate in Hz
    ///
    /// # Returns
    /// Result containing the AudioBuffer, or error if data length doesn't match layout
    pub fn from_interleaved(
        interleaved: &[f32],
        layout: ChannelLayout,
        sample_rate: u32,
    ) -> Result<Self> {
        let num_channels = layout.num_channels();

        if interleaved.is_empty() {
            return Ok(Self {
                samples: vec![Vec::new(); num_channels],
                sample_rate,
            });
        }

        if interleaved.len() % num_channels != 0 {
            return Err(NuevaError::InvalidAudio {
                reason: format!(
                    "Interleaved data length {} is not divisible by channel count {}",
                    interleaved.len(),
                    num_channels
                ),
                source: None,
            });
        }

        let num_samples = interleaved.len() / num_channels;
        let mut samples = vec![Vec::with_capacity(num_samples); num_channels];

        for frame in interleaved.chunks_exact(num_channels) {
            for (ch, &sample) in frame.iter().enumerate() {
                samples[ch].push(sample);
            }
        }

        Ok(Self {
            samples,
            sample_rate,
        })
    }

    /// Convert the buffer to interleaved format
    ///
    /// # Returns
    /// A Vec<f32> with samples in interleaved order (L, R, L, R, ... for stereo)
    pub fn to_interleaved(&self) -> Vec<f32> {
        let num_channels = self.channels();
        let num_samples = self.len();

        if num_channels == 0 || num_samples == 0 {
            return Vec::new();
        }

        let mut interleaved = Vec::with_capacity(num_channels * num_samples);

        for sample_idx in 0..num_samples {
            for channel in &self.samples {
                interleaved.push(channel[sample_idx]);
            }
        }

        interleaved
    }

    /// Get the number of channels
    #[inline]
    pub fn channels(&self) -> usize {
        self.samples.len()
    }

    /// Alias for channels() - returns the number of channels
    #[inline]
    pub fn num_channels(&self) -> usize {
        self.channels()
    }

    /// Get the number of samples per channel
    #[inline]
    pub fn len(&self) -> usize {
        self.samples.first().map(|ch| ch.len()).unwrap_or(0)
    }

    /// Check if the buffer is empty (no samples)
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Alias for len() - returns the number of samples per channel
    #[inline]
    pub fn num_samples(&self) -> usize {
        self.len()
    }

    /// Get the duration in seconds
    #[inline]
    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.len() as f64 / self.sample_rate as f64
    }

    /// Get the channel layout
    pub fn channel_layout(&self) -> Option<ChannelLayout> {
        ChannelLayout::from_count(self.channels())
    }

    /// Get immutable access to a channel's samples
    ///
    /// # Arguments
    /// * `index` - Channel index (0 for mono/left, 1 for right)
    ///
    /// # Returns
    /// A slice of samples for the specified channel
    ///
    /// # Panics
    /// Panics if the channel index is out of bounds
    #[inline]
    pub fn channel(&self, index: usize) -> &[f32] {
        &self.samples[index]
    }

    /// Get mutable access to a channel's samples
    ///
    /// # Arguments
    /// * `index` - Channel index (0 for mono/left, 1 for right)
    ///
    /// # Returns
    /// A mutable slice of samples for the specified channel
    ///
    /// # Panics
    /// Panics if the channel index is out of bounds
    #[inline]
    pub fn channel_mut(&mut self, index: usize) -> &mut [f32] {
        &mut self.samples[index]
    }

    /// Get a sample at the specified channel and index
    ///
    /// # Arguments
    /// * `channel` - Channel index (0 for mono/left, 1 for right)
    /// * `index` - Sample index
    ///
    /// # Returns
    /// The sample value, or None if indices are out of bounds
    #[inline]
    pub fn get_sample(&self, channel: usize, index: usize) -> Option<f32> {
        self.samples
            .get(channel)
            .and_then(|ch| ch.get(index).copied())
    }

    /// Set a sample at the specified channel and index
    ///
    /// # Arguments
    /// * `channel` - Channel index (0 for mono/left, 1 for right)
    /// * `index` - Sample index
    /// * `value` - New sample value
    ///
    /// # Returns
    /// true if the sample was set, false if indices are out of bounds
    #[inline]
    pub fn set_sample(&mut self, channel: usize, index: usize, value: f32) -> bool {
        if let Some(ch) = self.samples.get_mut(channel) {
            if let Some(sample) = ch.get_mut(index) {
                *sample = value;
                return true;
            }
        }
        false
    }

    /// Perform audio validation checks and return Result (spec 3.6)
    ///
    /// This is the primary validation method that returns an error if validation fails.
    ///
    /// # Returns
    /// * `Ok(())` if all validation checks pass
    /// * `Err(NuevaError)` describing the first failed check
    pub fn validate(&self) -> Result<()> {
        let validation = self.get_validation();

        if !validation.has_samples {
            return Err(NuevaError::EmptyAudio);
        }

        let duration = self.duration_secs();
        if duration < MIN_DURATION_SECS {
            return Err(NuevaError::AudioTooShort {
                duration_secs: duration,
            });
        }
        if duration > MAX_DURATION_SECS {
            return Err(NuevaError::AudioTooLong {
                duration_secs: duration,
            });
        }

        if !validation.not_silent {
            return Err(NuevaError::InvalidAudio {
                reason: "Audio is silent (RMS below -80dBFS)".to_string(),
                source: None,
            });
        }

        if !validation.not_dc_offset {
            return Err(NuevaError::InvalidAudio {
                reason: format!(
                    "DC offset detected (mean sample value: {:.4})",
                    calculate_mean(self)
                ),
                source: None,
            });
        }

        if !validation.not_clipped {
            return Err(NuevaError::InvalidAudio {
                reason: format!(
                    "Excessive clipping detected ({:.1}% of samples clipped)",
                    calculate_clip_ratio(self) * 100.0
                ),
                source: None,
            });
        }

        Ok(())
    }

    /// Perform audio validation checks and return detailed results (spec 3.6)
    ///
    /// Unlike validate(), this returns all validation results rather than
    /// failing on the first error.
    ///
    /// # Returns
    /// AudioValidation struct with results of all checks
    pub fn get_validation(&self) -> AudioValidation {
        let duration = self.duration_secs();
        let rms_db = calculate_rms(self);
        let mean = calculate_mean(self);
        let clip_ratio = calculate_clip_ratio(self);

        AudioValidation {
            has_samples: !self.is_empty(),
            reasonable_length: (MIN_DURATION_SECS..=MAX_DURATION_SECS).contains(&duration),
            not_corrupt: true, // In-memory buffers are assumed valid
            not_silent: rms_db > SILENCE_THRESHOLD_DB,
            not_dc_offset: mean.abs() < DC_OFFSET_THRESHOLD,
            not_clipped: clip_ratio < CLIP_RATIO_THRESHOLD,
        }
    }

    /// Check if all samples are finite (not NaN or Infinity)
    ///
    /// Used for DSP overflow detection.
    pub fn is_finite(&self) -> bool {
        self.samples
            .iter()
            .flat_map(|ch| ch.iter())
            .all(|s| s.is_finite())
    }

    /// Clamp all samples to the valid range [-1.0, 1.0]
    ///
    /// Useful for preventing clipping after processing.
    pub fn clamp(&mut self) {
        for channel in &mut self.samples {
            for sample in channel.iter_mut() {
                *sample = sample.clamp(-1.0, 1.0);
            }
        }
    }

    /// Apply gain to all samples
    ///
    /// # Arguments
    /// * `gain_db` - Gain in decibels
    pub fn apply_gain(&mut self, gain_db: f32) {
        let gain_linear = db_to_linear(gain_db);
        for channel in &mut self.samples {
            for sample in channel.iter_mut() {
                *sample *= gain_linear;
            }
        }
    }
}

impl Default for AudioBuffer {
    fn default() -> Self {
        Self::new(0, ChannelLayout::Stereo)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a buffer with known content
    fn create_test_buffer(samples: Vec<Vec<f32>>) -> AudioBuffer {
        AudioBuffer {
            samples,
            sample_rate: INTERNAL_SAMPLE_RATE,
        }
    }

    // ------------------------------------------------------------------------
    // Unit conversion tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_db_to_linear() {
        // 0 dB = 1.0 linear
        assert!((db_to_linear(0.0) - 1.0).abs() < 1e-6);
        // -6 dB ~= 0.5 linear
        assert!((db_to_linear(-6.0206) - 0.5).abs() < 1e-4);
        // -20 dB = 0.1 linear
        assert!((db_to_linear(-20.0) - 0.1).abs() < 1e-6);
        // -inf dB approaches 0
        assert!(db_to_linear(-120.0) < 1e-5);
    }

    #[test]
    fn test_linear_to_db() {
        // 1.0 linear = 0 dB
        assert!((linear_to_db(1.0) - 0.0).abs() < 1e-6);
        // 0.5 linear ~= -6 dB
        assert!((linear_to_db(0.5) - (-6.0206)).abs() < 1e-3);
        // 0.1 linear = -20 dB
        assert!((linear_to_db(0.1) - (-20.0)).abs() < 1e-4);
        // 0 linear = -inf dB
        assert!(linear_to_db(0.0).is_infinite() && linear_to_db(0.0).is_sign_negative());
    }

    #[test]
    fn test_db_linear_roundtrip() {
        let values = [0.1, 0.5, 1.0, 0.001];
        for &val in &values {
            let roundtrip = db_to_linear(linear_to_db(val));
            assert!(
                (roundtrip - val).abs() < 1e-6,
                "Roundtrip failed for {}",
                val
            );
        }
    }

    // ------------------------------------------------------------------------
    // RMS calculation tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_calculate_rms_silence() {
        let buffer = create_test_buffer(vec![vec![0.0; 1000]]);
        let rms = calculate_rms(&buffer);
        assert!(rms.is_infinite() && rms.is_sign_negative());
    }

    #[test]
    fn test_calculate_rms_unity() {
        // DC signal at 1.0 should have RMS of 1.0 = 0 dB
        let buffer = create_test_buffer(vec![vec![1.0; 1000]]);
        let rms = calculate_rms(&buffer);
        assert!((rms - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_calculate_rms_sine() {
        // Sine wave with amplitude 1.0 has RMS of 1/sqrt(2) ~= -3.01 dB
        let num_samples = INTERNAL_SAMPLE_RATE as usize;
        let freq = 1000.0;
        let samples: Vec<f32> = (0..num_samples)
            .map(|i| {
                let t = i as f32 / INTERNAL_SAMPLE_RATE as f32;
                (2.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect();
        let buffer = create_test_buffer(vec![samples]);
        let rms = calculate_rms(&buffer);
        // Expected: -3.01 dB
        assert!((rms - (-3.01)).abs() < 0.1);
    }

    #[test]
    fn test_calculate_rms_empty() {
        let buffer = create_test_buffer(vec![]);
        let rms = calculate_rms(&buffer);
        assert!(rms.is_infinite() && rms.is_sign_negative());
    }

    // ------------------------------------------------------------------------
    // Peak calculation tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_calculate_peak_silence() {
        let buffer = create_test_buffer(vec![vec![0.0; 1000]]);
        let peak = calculate_peak(&buffer);
        assert!(peak.is_infinite() && peak.is_sign_negative());
    }

    #[test]
    fn test_calculate_peak_unity() {
        let mut samples = vec![0.0; 1000];
        samples[500] = 1.0;
        let buffer = create_test_buffer(vec![samples]);
        let peak = calculate_peak(&buffer);
        assert!((peak - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_calculate_peak_negative() {
        let mut samples = vec![0.0; 1000];
        samples[500] = -0.5;
        let buffer = create_test_buffer(vec![samples]);
        let peak = calculate_peak(&buffer);
        // -0.5 linear = -6.02 dB
        assert!((peak - (-6.02)).abs() < 0.1);
    }

    // ------------------------------------------------------------------------
    // Mean calculation tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_calculate_mean_zero() {
        // Symmetric signal has mean of 0
        let samples = vec![-0.5, 0.5, -0.5, 0.5];
        let buffer = create_test_buffer(vec![samples]);
        let mean = calculate_mean(&buffer);
        assert!(mean.abs() < 1e-6);
    }

    #[test]
    fn test_calculate_mean_dc_offset() {
        // All 0.5 has mean of 0.5
        let buffer = create_test_buffer(vec![vec![0.5; 1000]]);
        let mean = calculate_mean(&buffer);
        assert!((mean - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_calculate_mean_empty() {
        let buffer = create_test_buffer(vec![]);
        let mean = calculate_mean(&buffer);
        assert!((mean - 0.0).abs() < 1e-6);
    }

    // ------------------------------------------------------------------------
    // Clip ratio tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_calculate_clip_ratio_none() {
        let buffer = create_test_buffer(vec![vec![0.5; 1000]]);
        let ratio = calculate_clip_ratio(&buffer);
        assert!((ratio - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_calculate_clip_ratio_all() {
        let buffer = create_test_buffer(vec![vec![1.0; 1000]]);
        let ratio = calculate_clip_ratio(&buffer);
        assert!((ratio - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_calculate_clip_ratio_partial() {
        // 10 out of 1000 samples clipped = 1%
        let mut samples = vec![0.5; 1000];
        for i in 0..10 {
            samples[i] = 1.0;
        }
        let buffer = create_test_buffer(vec![samples]);
        let ratio = calculate_clip_ratio(&buffer);
        assert!((ratio - 0.01).abs() < 1e-6);
    }

    // ------------------------------------------------------------------------
    // ChannelLayout tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_channel_layout() {
        assert_eq!(ChannelLayout::Mono.num_channels(), 1);
        assert_eq!(ChannelLayout::Stereo.num_channels(), 2);
        assert_eq!(ChannelLayout::from_count(1), Some(ChannelLayout::Mono));
        assert_eq!(ChannelLayout::from_count(2), Some(ChannelLayout::Stereo));
        assert_eq!(ChannelLayout::from_count(6), None);
    }

    // ------------------------------------------------------------------------
    // AudioBuffer tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_buffer_new() {
        let buffer = AudioBuffer::new(1000, ChannelLayout::Stereo);
        assert_eq!(buffer.channels(), 2);
        assert_eq!(buffer.len(), 1000);
        assert_eq!(buffer.sample_rate, INTERNAL_SAMPLE_RATE);
    }

    #[test]
    fn test_buffer_duration() {
        let buffer = AudioBuffer::new(INTERNAL_SAMPLE_RATE as usize, ChannelLayout::Mono);
        assert!((buffer.duration_secs() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_buffer_from_interleaved_stereo() {
        let interleaved = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
        let buffer = AudioBuffer::from_interleaved(
            &interleaved,
            ChannelLayout::Stereo,
            INTERNAL_SAMPLE_RATE,
        )
        .unwrap();

        assert_eq!(buffer.channels(), 2);
        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.get_sample(0, 0), Some(0.1)); // Left
        assert_eq!(buffer.get_sample(1, 0), Some(0.2)); // Right
        assert_eq!(buffer.get_sample(0, 1), Some(0.3)); // Left
        assert_eq!(buffer.get_sample(1, 1), Some(0.4)); // Right
    }

    #[test]
    fn test_buffer_from_interleaved_mono() {
        let interleaved = vec![0.1, 0.2, 0.3];
        let buffer =
            AudioBuffer::from_interleaved(&interleaved, ChannelLayout::Mono, INTERNAL_SAMPLE_RATE)
                .unwrap();

        assert_eq!(buffer.channels(), 1);
        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.get_sample(0, 0), Some(0.1));
        assert_eq!(buffer.get_sample(0, 2), Some(0.3));
    }

    #[test]
    fn test_buffer_from_interleaved_invalid() {
        // 5 samples can't be evenly split into stereo
        let interleaved = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let result = AudioBuffer::from_interleaved(
            &interleaved,
            ChannelLayout::Stereo,
            INTERNAL_SAMPLE_RATE,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_buffer_to_interleaved() {
        let buffer = create_test_buffer(vec![
            vec![0.1, 0.3, 0.5], // Left
            vec![0.2, 0.4, 0.6], // Right
        ]);
        let interleaved = buffer.to_interleaved();
        assert_eq!(interleaved, vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
    }

    #[test]
    fn test_buffer_interleaved_roundtrip() {
        let original = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
        let buffer =
            AudioBuffer::from_interleaved(&original, ChannelLayout::Stereo, INTERNAL_SAMPLE_RATE)
                .unwrap();
        let roundtrip = buffer.to_interleaved();
        assert_eq!(original, roundtrip);
    }

    #[test]
    fn test_buffer_get_set_sample() {
        let mut buffer = AudioBuffer::new(100, ChannelLayout::Stereo);

        // Set and get
        assert!(buffer.set_sample(0, 50, 0.5));
        assert_eq!(buffer.get_sample(0, 50), Some(0.5));

        // Out of bounds
        assert_eq!(buffer.get_sample(2, 0), None);
        assert_eq!(buffer.get_sample(0, 100), None);
        assert!(!buffer.set_sample(2, 0, 0.5));
        assert!(!buffer.set_sample(0, 100, 0.5));
    }

    #[test]
    fn test_buffer_channel_access() {
        let mut buffer = AudioBuffer::new(100, ChannelLayout::Stereo);

        // Write using channel_mut
        let left = buffer.channel_mut(0);
        left[0] = 0.5;
        left[50] = 0.75;

        // Read using channel
        let left_read = buffer.channel(0);
        assert_eq!(left_read[0], 0.5);
        assert_eq!(left_read[50], 0.75);
    }

    #[test]
    fn test_buffer_is_empty() {
        let empty = AudioBuffer::new(0, ChannelLayout::Mono);
        assert!(empty.is_empty());

        let not_empty = AudioBuffer::new(100, ChannelLayout::Mono);
        assert!(!not_empty.is_empty());
    }

    // ------------------------------------------------------------------------
    // AudioValidation tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_validation_valid_audio() {
        // Create a 1-second sine wave
        let num_samples = INTERNAL_SAMPLE_RATE as usize;
        let freq = 440.0;
        let samples: Vec<f32> = (0..num_samples)
            .map(|i| {
                let t = i as f32 / INTERNAL_SAMPLE_RATE as f32;
                0.5 * (2.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect();
        let buffer = create_test_buffer(vec![samples]);
        let validation = buffer.get_validation();

        assert!(
            validation.is_valid(),
            "Failed checks: {:?}",
            validation.failed_checks()
        );

        // Also test the Result-returning validate()
        assert!(buffer.validate().is_ok());
    }

    #[test]
    fn test_validation_empty() {
        let buffer = create_test_buffer(vec![]);
        let validation = buffer.get_validation();

        assert!(!validation.has_samples);
        assert!(!validation.is_valid());

        // Result-returning validate() should return EmptyAudio error
        let result = buffer.validate();
        assert!(matches!(result, Err(NuevaError::EmptyAudio)));
    }

    #[test]
    fn test_validation_too_short() {
        // 0.05 seconds = less than MIN_DURATION_SECS
        let num_samples = (INTERNAL_SAMPLE_RATE as f64 * 0.05) as usize;
        let buffer = AudioBuffer::new(num_samples, ChannelLayout::Mono);
        let validation = buffer.get_validation();

        assert!(!validation.reasonable_length);

        // Result-returning validate() should return AudioTooShort error
        let result = buffer.validate();
        assert!(matches!(result, Err(NuevaError::AudioTooShort { .. })));
    }

    #[test]
    fn test_validation_silent() {
        // All zeros
        let buffer = AudioBuffer::new(INTERNAL_SAMPLE_RATE as usize, ChannelLayout::Mono);
        let validation = buffer.get_validation();

        assert!(!validation.not_silent);
    }

    #[test]
    fn test_validation_dc_offset() {
        // Buffer with significant DC offset
        let buffer = create_test_buffer(vec![vec![0.5; INTERNAL_SAMPLE_RATE as usize]]);
        let validation = buffer.get_validation();

        assert!(!validation.not_dc_offset);
    }

    #[test]
    fn test_validation_clipped() {
        // 5% clipped samples (above threshold)
        let num_samples = 1000;
        let mut samples = vec![0.5; num_samples];
        for i in 0..50 {
            samples[i] = 1.0;
        }
        let buffer = create_test_buffer(vec![samples]);
        let validation = buffer.get_validation();

        assert!(!validation.not_clipped);
    }

    #[test]
    fn test_validation_failed_checks() {
        let buffer = create_test_buffer(vec![]);
        let validation = buffer.get_validation();
        let failures = validation.failed_checks();

        assert!(failures.contains(&"no samples"));
    }

    // ------------------------------------------------------------------------
    // Buffer utility tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_buffer_is_finite() {
        let buffer = create_test_buffer(vec![vec![0.5; 100]]);
        assert!(buffer.is_finite());

        let buffer_nan = create_test_buffer(vec![vec![f32::NAN; 100]]);
        assert!(!buffer_nan.is_finite());

        let buffer_inf = create_test_buffer(vec![vec![f32::INFINITY; 100]]);
        assert!(!buffer_inf.is_finite());
    }

    #[test]
    fn test_buffer_clamp() {
        let mut buffer = create_test_buffer(vec![vec![-2.0, -0.5, 0.0, 0.5, 2.0]]);
        buffer.clamp();

        assert_eq!(buffer.get_sample(0, 0), Some(-1.0));
        assert_eq!(buffer.get_sample(0, 1), Some(-0.5));
        assert_eq!(buffer.get_sample(0, 2), Some(0.0));
        assert_eq!(buffer.get_sample(0, 3), Some(0.5));
        assert_eq!(buffer.get_sample(0, 4), Some(1.0));
    }

    #[test]
    fn test_buffer_apply_gain() {
        let mut buffer = create_test_buffer(vec![vec![0.5; 100]]);
        buffer.apply_gain(-6.0206); // -6 dB ~= 0.5x

        // 0.5 * 0.5 = 0.25
        let sample = buffer.get_sample(0, 0).unwrap();
        assert!((sample - 0.25).abs() < 0.01);
    }
}
