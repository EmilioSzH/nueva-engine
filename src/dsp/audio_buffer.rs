//! Audio buffer type for DSP processing

use crate::error::{NuevaError, Result};

/// Interleaved audio buffer for DSP processing
///
/// Samples are stored in interleaved format: [L0, R0, L1, R1, ...]
/// This matches common audio file formats and simplifies I/O.
#[derive(Clone, Debug)]
pub struct AudioBuffer {
    /// Interleaved sample data
    samples: Vec<f32>,
    /// Number of channels (1 = mono, 2 = stereo)
    num_channels: usize,
    /// Sample rate in Hz
    sample_rate: f64,
}

impl AudioBuffer {
    /// Create a new audio buffer with the given parameters
    pub fn new(num_channels: usize, num_samples: usize, sample_rate: f64) -> Self {
        Self {
            samples: vec![0.0; num_channels * num_samples],
            num_channels,
            sample_rate,
        }
    }

    /// Create a buffer from existing interleaved samples
    pub fn from_interleaved(
        samples: Vec<f32>,
        num_channels: usize,
        sample_rate: f64,
    ) -> Result<Self> {
        if samples.len() % num_channels != 0 {
            return Err(NuevaError::InvalidAudioFile {
                details: format!(
                    "Sample count {} is not divisible by channel count {}",
                    samples.len(),
                    num_channels
                ),
            });
        }
        Ok(Self {
            samples,
            num_channels,
            sample_rate,
        })
    }

    /// Number of channels
    pub fn num_channels(&self) -> usize {
        self.num_channels
    }

    /// Number of samples per channel
    pub fn num_samples(&self) -> usize {
        self.samples.len() / self.num_channels
    }

    /// Sample rate in Hz
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Duration in seconds
    pub fn duration(&self) -> f64 {
        self.num_samples() as f64 / self.sample_rate
    }

    /// Get a reference to all interleaved samples
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// Get a mutable reference to all interleaved samples
    pub fn samples_mut(&mut self) -> &mut [f32] {
        &mut self.samples
    }

    /// Get a sample at the given frame and channel
    pub fn get(&self, frame: usize, channel: usize) -> Option<f32> {
        if frame < self.num_samples() && channel < self.num_channels {
            Some(self.samples[frame * self.num_channels + channel])
        } else {
            None
        }
    }

    /// Set a sample at the given frame and channel
    pub fn set(&mut self, frame: usize, channel: usize, value: f32) {
        if frame < self.num_samples() && channel < self.num_channels {
            self.samples[frame * self.num_channels + channel] = value;
        }
    }

    /// Create a copy of this buffer (for rollback support per spec §9.4)
    pub fn create_copy(&self) -> Self {
        self.clone()
    }

    /// Check if buffer contains valid audio (no NaN/Inf) - spec §9.4
    pub fn is_valid(&self) -> bool {
        self.samples
            .iter()
            .all(|&s| s.is_finite() && s.abs() <= 16.0)
    }

    /// Calculate RMS level in dB for a channel
    pub fn rms_db(&self, channel: usize) -> f64 {
        if channel >= self.num_channels {
            return f64::NEG_INFINITY;
        }

        let sum_sq: f64 = self
            .samples
            .iter()
            .skip(channel)
            .step_by(self.num_channels)
            .map(|&s| (s as f64).powi(2))
            .sum();

        let rms = (sum_sq / self.num_samples() as f64).sqrt();

        if rms > 0.0 {
            20.0 * rms.log10()
        } else {
            f64::NEG_INFINITY
        }
    }

    /// Calculate peak level in dB for a channel
    pub fn peak_db(&self, channel: usize) -> f64 {
        if channel >= self.num_channels {
            return f64::NEG_INFINITY;
        }

        let peak: f32 = self
            .samples
            .iter()
            .skip(channel)
            .step_by(self.num_channels)
            .map(|&s| s.abs())
            .fold(0.0f32, f32::max);

        if peak > 0.0 {
            20.0 * (peak as f64).log10()
        } else {
            f64::NEG_INFINITY
        }
    }

    /// Check for clipping (spec §10.1: >1% samples at ±1.0)
    pub fn clipping_ratio(&self) -> f64 {
        let clipped = self.samples.iter().filter(|&&s| s.abs() >= 1.0).count();
        clipped as f64 / self.samples.len() as f64
    }

    /// Check for DC offset (spec §10.1: Mean > 0.01)
    pub fn dc_offset(&self, channel: usize) -> f64 {
        if channel >= self.num_channels {
            return 0.0;
        }

        let sum: f64 = self
            .samples
            .iter()
            .skip(channel)
            .step_by(self.num_channels)
            .map(|&s| s as f64)
            .sum();

        sum / self.num_samples() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_buffer() {
        let buf = AudioBuffer::new(2, 1000, 44100.0);
        assert_eq!(buf.num_channels(), 2);
        assert_eq!(buf.num_samples(), 1000);
        assert_eq!(buf.sample_rate(), 44100.0);
    }

    #[test]
    fn test_get_set() {
        let mut buf = AudioBuffer::new(2, 100, 44100.0);
        buf.set(0, 0, 0.5);
        buf.set(0, 1, -0.5);
        assert_eq!(buf.get(0, 0), Some(0.5));
        assert_eq!(buf.get(0, 1), Some(-0.5));
    }

    #[test]
    fn test_rms_db() {
        let mut buf = AudioBuffer::new(1, 1000, 44100.0);
        // Fill with sine wave at unity amplitude
        for i in 0..1000 {
            let t = i as f32 / 44100.0;
            buf.set(i, 0, (2.0 * std::f32::consts::PI * 440.0 * t).sin());
        }
        // RMS of sine wave is 1/sqrt(2) = -3.01 dB
        let rms = buf.rms_db(0);
        assert!((rms - (-3.01)).abs() < 0.1);
    }

    #[test]
    fn test_is_valid() {
        let mut buf = AudioBuffer::new(1, 100, 44100.0);
        assert!(buf.is_valid());

        buf.set(50, 0, f32::NAN);
        assert!(!buf.is_valid());
    }
}
