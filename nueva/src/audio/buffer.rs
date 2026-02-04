//! Audio buffer implementation
//!
//! AudioBuffer is the core data structure for holding audio samples.

use crate::error::{NuevaError, Result};

/// Audio sample data with metadata
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    /// Interleaved audio samples normalized to -1.0..1.0
    samples: Vec<f32>,
    /// Number of audio channels (1 = mono, 2 = stereo)
    channels: u16,
    /// Sample rate in Hz
    sample_rate: u32,
}

impl AudioBuffer {
    /// Create a new audio buffer with the given parameters
    pub fn new(samples: Vec<f32>, channels: u16, sample_rate: u32) -> Result<Self> {
        if samples.is_empty() {
            return Err(NuevaError::EmptyBuffer);
        }
        if samples.len() % channels as usize != 0 {
            return Err(NuevaError::UnsupportedFormat {
                details: format!(
                    "Sample count {} is not divisible by channel count {}",
                    samples.len(),
                    channels
                ),
            });
        }
        Ok(Self {
            samples,
            channels,
            sample_rate,
        })
    }

    /// Create a silent buffer with the given duration
    pub fn silence(duration_secs: f32, channels: u16, sample_rate: u32) -> Self {
        let num_samples = (duration_secs * sample_rate as f32) as usize * channels as usize;
        Self {
            samples: vec![0.0; num_samples],
            channels,
            sample_rate,
        }
    }

    /// Create a sine wave test tone
    pub fn sine_wave(frequency: f32, duration_secs: f32, sample_rate: u32) -> Self {
        let num_samples = (duration_secs * sample_rate as f32) as usize;
        let mut samples = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let sample = (2.0 * std::f32::consts::PI * frequency * t).sin();
            samples.push(sample);
        }

        Self {
            samples,
            channels: 1,
            sample_rate,
        }
    }

    /// Get a reference to the samples
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// Get a mutable reference to the samples
    pub fn samples_mut(&mut self) -> &mut [f32] {
        &mut self.samples
    }

    /// Get the number of channels
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Get the sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get the number of frames (samples per channel)
    pub fn num_frames(&self) -> usize {
        self.samples.len() / self.channels as usize
    }

    /// Get the duration in seconds
    pub fn duration(&self) -> f32 {
        self.num_frames() as f32 / self.sample_rate as f32
    }

    /// Get samples for a specific channel (0-indexed)
    pub fn channel_samples(&self, channel: u16) -> Vec<f32> {
        if channel >= self.channels {
            return Vec::new();
        }
        self.samples
            .iter()
            .skip(channel as usize)
            .step_by(self.channels as usize)
            .copied()
            .collect()
    }

    /// Apply gain in linear scale
    pub fn apply_gain(&mut self, gain: f32) {
        for sample in &mut self.samples {
            *sample *= gain;
        }
    }

    /// Apply gain in decibels
    pub fn apply_gain_db(&mut self, gain_db: f32) {
        let gain_linear = 10.0_f32.powf(gain_db / 20.0);
        self.apply_gain(gain_linear);
    }

    /// Check if buffers are identical (bit-perfect comparison)
    pub fn is_identical_to(&self, other: &AudioBuffer) -> bool {
        self.channels == other.channels
            && self.sample_rate == other.sample_rate
            && self.samples == other.samples
    }

    /// Check if buffers are approximately equal within tolerance
    pub fn is_approx_equal(&self, other: &AudioBuffer, tolerance: f32) -> bool {
        if self.channels != other.channels || self.sample_rate != other.sample_rate {
            return false;
        }
        if self.samples.len() != other.samples.len() {
            return false;
        }
        self.samples
            .iter()
            .zip(other.samples.iter())
            .all(|(a, b)| (a - b).abs() <= tolerance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sine_wave_generation() {
        let buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);
        assert_eq!(buffer.channels(), 1);
        assert_eq!(buffer.sample_rate(), 44100);
        assert_eq!(buffer.num_frames(), 44100);
        assert!((buffer.duration() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_silence_generation() {
        let buffer = AudioBuffer::silence(2.0, 2, 48000);
        assert_eq!(buffer.channels(), 2);
        assert_eq!(buffer.sample_rate(), 48000);
        assert_eq!(buffer.num_frames(), 96000);
        assert!(buffer.samples().iter().all(|&s| s == 0.0));
    }

    #[test]
    fn test_gain_application() {
        let mut buffer = AudioBuffer::sine_wave(440.0, 0.1, 44100);
        let original_peak: f32 = buffer.samples().iter().map(|s| s.abs()).fold(0.0, f32::max);

        buffer.apply_gain(0.5);
        let new_peak: f32 = buffer.samples().iter().map(|s| s.abs()).fold(0.0, f32::max);

        assert!((new_peak - original_peak * 0.5).abs() < 0.001);
    }

    #[test]
    fn test_gain_db_application() {
        let mut buffer = AudioBuffer::sine_wave(440.0, 0.1, 44100);
        let original_peak: f32 = buffer.samples().iter().map(|s| s.abs()).fold(0.0, f32::max);

        buffer.apply_gain_db(-6.0);
        let new_peak: f32 = buffer.samples().iter().map(|s| s.abs()).fold(0.0, f32::max);

        // -6dB should be approximately half amplitude
        let expected = original_peak * 0.5012; // 10^(-6/20) ≈ 0.5012
        assert!((new_peak - expected).abs() < 0.01);
    }

    #[test]
    fn test_channel_extraction() {
        // Create stereo buffer with different values per channel
        let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]; // L, R, L, R, L, R
        let buffer = AudioBuffer::new(samples, 2, 44100).unwrap();

        let left = buffer.channel_samples(0);
        let right = buffer.channel_samples(1);

        assert_eq!(left, vec![1.0, 3.0, 5.0]);
        assert_eq!(right, vec![2.0, 4.0, 6.0]);
    }

    #[test]
    fn test_empty_buffer_error() {
        let result = AudioBuffer::new(vec![], 1, 44100);
        assert!(matches!(result, Err(NuevaError::EmptyBuffer)));
    }
}
