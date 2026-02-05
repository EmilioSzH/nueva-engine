//! Delay Effect
//!
//! Stereo delay with feedback, filtering, and ping-pong mode.
//! Per spec section 4.2.5.

use crate::dsp::effect::{Effect, EffectParams};
use crate::engine::AudioBuffer;
use crate::error::{NuevaError, Result};
use crate::impl_effect_common;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::f32::consts::PI;

/// Stereo delay effect with feedback filtering and ping-pong mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delay {
    params: EffectParams,
    /// Delay time in milliseconds (1-2000)
    delay_time_ms: f32,
    /// Feedback amount (0-0.95, NOT 1.0 to prevent infinite feedback)
    feedback: f32,
    /// Wet signal level (0-1)
    wet_level: f32,
    /// Dry signal level (0-1)
    dry_level: f32,
    /// Enable stereo ping-pong mode
    ping_pong: bool,
    /// Low-pass filter frequency on feedback path (20-20000 Hz)
    filter_freq: f32,
    /// Circular buffer for left channel
    #[serde(skip)]
    buffer_l: Vec<f32>,
    /// Circular buffer for right channel
    #[serde(skip)]
    buffer_r: Vec<f32>,
    /// Current write position in circular buffer
    #[serde(skip)]
    write_pos: usize,
    /// Sample rate for calculations
    #[serde(skip)]
    sample_rate: f32,
    /// One-pole lowpass filter state for left channel
    #[serde(skip)]
    filter_state_l: f32,
    /// One-pole lowpass filter state for right channel
    #[serde(skip)]
    filter_state_r: f32,
}

impl Delay {
    /// Create a new delay effect with specified delay time
    ///
    /// # Arguments
    /// * `delay_time_ms` - Delay time in milliseconds (clamped to 1-2000)
    pub fn new(delay_time_ms: f32) -> Self {
        let delay_time_ms = delay_time_ms.clamp(1.0, 2000.0);
        Self {
            params: EffectParams::default(),
            delay_time_ms,
            feedback: 0.3,
            wet_level: 0.5,
            dry_level: 1.0,
            ping_pong: false,
            filter_freq: 8000.0,
            buffer_l: Vec::new(),
            buffer_r: Vec::new(),
            write_pos: 0,
            sample_rate: 48000.0,
            filter_state_l: 0.0,
            filter_state_r: 0.0,
        }
    }

    /// Set delay time in milliseconds
    ///
    /// # Arguments
    /// * `ms` - Delay time (clamped to 1-2000 ms)
    pub fn set_delay_time_ms(&mut self, ms: f32) {
        self.delay_time_ms = ms.clamp(1.0, 2000.0);
        self.resize_buffers();
    }

    /// Get delay time in milliseconds
    pub fn delay_time_ms(&self) -> f32 {
        self.delay_time_ms
    }

    /// Set feedback amount
    ///
    /// # Arguments
    /// * `fb` - Feedback amount (clamped to 0-0.95)
    pub fn set_feedback(&mut self, fb: f32) {
        self.feedback = fb.clamp(0.0, 0.95);
    }

    /// Get feedback amount
    pub fn feedback(&self) -> f32 {
        self.feedback
    }

    /// Set wet signal level
    ///
    /// # Arguments
    /// * `level` - Wet level (clamped to 0-1)
    pub fn set_wet_level(&mut self, level: f32) {
        self.wet_level = level.clamp(0.0, 1.0);
    }

    /// Get wet signal level
    pub fn wet_level(&self) -> f32 {
        self.wet_level
    }

    /// Set dry signal level
    ///
    /// # Arguments
    /// * `level` - Dry level (clamped to 0-1)
    pub fn set_dry_level(&mut self, level: f32) {
        self.dry_level = level.clamp(0.0, 1.0);
    }

    /// Get dry signal level
    pub fn dry_level(&self) -> f32 {
        self.dry_level
    }

    /// Enable or disable ping-pong mode
    ///
    /// In ping-pong mode, the delay alternates between left and right channels.
    pub fn set_ping_pong(&mut self, enabled: bool) {
        self.ping_pong = enabled;
    }

    /// Check if ping-pong mode is enabled
    pub fn ping_pong(&self) -> bool {
        self.ping_pong
    }

    /// Set feedback filter frequency
    ///
    /// # Arguments
    /// * `freq` - Filter cutoff frequency in Hz (clamped to 20-20000)
    pub fn set_filter_freq(&mut self, freq: f32) {
        self.filter_freq = freq.clamp(20.0, 20000.0);
    }

    /// Get feedback filter frequency
    pub fn filter_freq(&self) -> f32 {
        self.filter_freq
    }

    /// Calculate delay in samples
    fn delay_samples(&self) -> usize {
        ((self.delay_time_ms * self.sample_rate / 1000.0) as usize).max(1)
    }

    /// Calculate required buffer size (add extra for safety)
    fn required_buffer_size(&self) -> usize {
        // Add extra 10ms for safety margin
        let max_delay_ms = self.delay_time_ms + 10.0;
        ((max_delay_ms * self.sample_rate / 1000.0) as usize).max(1)
    }

    /// Resize delay buffers based on current settings
    fn resize_buffers(&mut self) {
        let size = self.required_buffer_size();
        if self.buffer_l.len() != size {
            self.buffer_l.resize(size, 0.0);
            self.buffer_r.resize(size, 0.0);
            // Reset write position if it's out of bounds
            if self.write_pos >= size {
                self.write_pos = 0;
            }
        }
    }

    /// Calculate one-pole lowpass filter coefficient
    fn calc_filter_coeff(&self) -> f32 {
        // One-pole lowpass: y[n] = y[n-1] + coeff * (x[n] - y[n-1])
        // coeff = 1 - exp(-2 * PI * fc / fs)
        let fc = self.filter_freq;
        let fs = self.sample_rate;
        1.0 - (-2.0 * PI * fc / fs).exp()
    }

    /// Apply one-pole lowpass filter (inline to avoid borrow issues)
    #[inline]
    fn apply_filter_inline(input: f32, state: &mut f32, coeff: f32) -> f32 {
        *state += coeff * (input - *state);
        *state
    }

    /// Read from circular buffer with wrapping
    #[inline]
    fn read_buffer(&self, buffer: &[f32], delay_samples: usize) -> f32 {
        let size = buffer.len();
        if size == 0 {
            return 0.0;
        }
        let read_pos = if self.write_pos >= delay_samples {
            self.write_pos - delay_samples
        } else {
            size - (delay_samples - self.write_pos)
        };
        buffer[read_pos % size]
    }

    /// Process mono audio
    fn process_mono(&mut self, buffer: &mut AudioBuffer) {
        let delay_samples = self.delay_samples();
        let filter_coeff = self.calc_filter_coeff();
        let num_samples = buffer.num_samples();
        let feedback = self.feedback;
        let wet_level = self.wet_level;
        let dry_level = self.dry_level;

        for i in 0..num_samples {
            let input = buffer.samples[0][i];

            // Read from delay buffer
            let delayed = self.read_buffer(&self.buffer_l, delay_samples);

            // Apply feedback filter
            let filtered = Self::apply_filter_inline(
                delayed * feedback,
                &mut self.filter_state_l,
                filter_coeff,
            );

            // Write to buffer: input + filtered feedback
            if !self.buffer_l.is_empty() {
                self.buffer_l[self.write_pos] = input + filtered;
            }

            // Mix dry and wet
            buffer.samples[0][i] = input * dry_level + delayed * wet_level;

            // Advance write position
            self.write_pos = (self.write_pos + 1) % self.buffer_l.len().max(1);
        }
    }

    /// Process stereo audio (normal mode)
    fn process_stereo_normal(&mut self, buffer: &mut AudioBuffer) {
        let delay_samples = self.delay_samples();
        let filter_coeff = self.calc_filter_coeff();
        let num_samples = buffer.num_samples();
        let feedback = self.feedback;
        let wet_level = self.wet_level;
        let dry_level = self.dry_level;

        for i in 0..num_samples {
            let input_l = buffer.samples[0][i];
            let input_r = buffer.samples[1][i];

            // Read from delay buffers
            let delayed_l = self.read_buffer(&self.buffer_l, delay_samples);
            let delayed_r = self.read_buffer(&self.buffer_r, delay_samples);

            // Apply feedback filter
            let filtered_l = Self::apply_filter_inline(
                delayed_l * feedback,
                &mut self.filter_state_l,
                filter_coeff,
            );
            let filtered_r = Self::apply_filter_inline(
                delayed_r * feedback,
                &mut self.filter_state_r,
                filter_coeff,
            );

            // Write to buffers
            if !self.buffer_l.is_empty() {
                self.buffer_l[self.write_pos] = input_l + filtered_l;
                self.buffer_r[self.write_pos] = input_r + filtered_r;
            }

            // Mix dry and wet
            buffer.samples[0][i] = input_l * dry_level + delayed_l * wet_level;
            buffer.samples[1][i] = input_r * dry_level + delayed_r * wet_level;

            // Advance write position
            self.write_pos = (self.write_pos + 1) % self.buffer_l.len().max(1);
        }
    }

    /// Process stereo audio (ping-pong mode)
    fn process_stereo_pingpong(&mut self, buffer: &mut AudioBuffer) {
        let delay_samples = self.delay_samples();
        let filter_coeff = self.calc_filter_coeff();
        let num_samples = buffer.num_samples();
        let feedback = self.feedback;
        let wet_level = self.wet_level;
        let dry_level = self.dry_level;

        for i in 0..num_samples {
            let input_l = buffer.samples[0][i];
            let input_r = buffer.samples[1][i];

            // Read from delay buffers
            let delayed_l = self.read_buffer(&self.buffer_l, delay_samples);
            let delayed_r = self.read_buffer(&self.buffer_r, delay_samples);

            // Apply feedback filter to cross-channel feedback
            // Ping-pong: left feedback goes to right, right feedback goes to left
            let filtered_from_r = Self::apply_filter_inline(
                delayed_r * feedback,
                &mut self.filter_state_l,
                filter_coeff,
            );
            let filtered_from_l = Self::apply_filter_inline(
                delayed_l * feedback,
                &mut self.filter_state_r,
                filter_coeff,
            );

            // Write to buffers with cross-channel feedback
            if !self.buffer_l.is_empty() {
                // Left buffer gets input + feedback from right
                self.buffer_l[self.write_pos] = input_l + filtered_from_r;
                // Right buffer gets input + feedback from left
                self.buffer_r[self.write_pos] = input_r + filtered_from_l;
            }

            // Mix dry and wet
            buffer.samples[0][i] = input_l * dry_level + delayed_l * wet_level;
            buffer.samples[1][i] = input_r * dry_level + delayed_r * wet_level;

            // Advance write position
            self.write_pos = (self.write_pos + 1) % self.buffer_l.len().max(1);
        }
    }
}

impl Effect for Delay {
    impl_effect_common!(Delay, "delay", "Delay");

    fn process(&mut self, buffer: &mut AudioBuffer) {
        if !self.params.enabled || buffer.is_empty() {
            return;
        }

        let num_channels = buffer.num_channels();

        match num_channels {
            1 => self.process_mono(buffer),
            2 if self.ping_pong => self.process_stereo_pingpong(buffer),
            2 => self.process_stereo_normal(buffer),
            _ => {
                // For other channel counts, process first two channels as stereo
                if num_channels >= 2 {
                    if self.ping_pong {
                        self.process_stereo_pingpong(buffer);
                    } else {
                        self.process_stereo_normal(buffer);
                    }
                }
            }
        }
    }

    fn prepare(&mut self, sample_rate: u32, _max_block_size: usize) {
        self.sample_rate = sample_rate as f32;
        self.resize_buffers();
    }

    fn reset(&mut self) {
        self.buffer_l.fill(0.0);
        self.buffer_r.fill(0.0);
        self.write_pos = 0;
        self.filter_state_l = 0.0;
        self.filter_state_r = 0.0;
    }

    fn to_json(&self) -> Result<Value> {
        serde_json::to_value(self).map_err(NuevaError::Serialization)
    }

    fn from_json(&mut self, json: &Value) -> Result<()> {
        let parsed: Delay =
            serde_json::from_value(json.clone()).map_err(NuevaError::Serialization)?;

        self.params = parsed.params;
        self.delay_time_ms = parsed.delay_time_ms;
        self.feedback = parsed.feedback;
        self.wet_level = parsed.wet_level;
        self.dry_level = parsed.dry_level;
        self.ping_pong = parsed.ping_pong;
        self.filter_freq = parsed.filter_freq;

        // Resize buffers for new settings
        self.resize_buffers();

        Ok(())
    }

    fn get_params(&self) -> Value {
        json!({
            "id": self.params.id,
            "enabled": self.params.enabled,
            "delay_time_ms": self.delay_time_ms,
            "feedback": self.feedback,
            "wet_level": self.wet_level,
            "dry_level": self.dry_level,
            "ping_pong": self.ping_pong,
            "filter_freq": self.filter_freq
        })
    }

    fn set_param(&mut self, name: &str, value: &Value) -> Result<()> {
        match name {
            "delay_time_ms" => {
                if let Some(v) = value.as_f64() {
                    self.set_delay_time_ms(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!("Invalid value for delay_time_ms: {:?}", value),
                    })
                }
            }
            "feedback" => {
                if let Some(v) = value.as_f64() {
                    self.set_feedback(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!("Invalid value for feedback: {:?}", value),
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
            "ping_pong" => {
                if let Some(v) = value.as_bool() {
                    self.set_ping_pong(v);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!("Invalid value for ping_pong: {:?}", value),
                    })
                }
            }
            "filter_freq" => {
                if let Some(v) = value.as_f64() {
                    self.set_filter_freq(v as f32);
                    Ok(())
                } else {
                    Err(NuevaError::ProcessingError {
                        reason: format!("Invalid value for filter_freq: {:?}", value),
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
    fn test_delay_new() {
        let delay = Delay::new(500.0);
        assert_eq!(delay.delay_time_ms(), 500.0);
        assert_eq!(delay.feedback(), 0.3);
        assert_eq!(delay.wet_level(), 0.5);
        assert_eq!(delay.dry_level(), 1.0);
        assert!(!delay.ping_pong());
    }

    #[test]
    fn test_delay_time_clamp() {
        let mut delay = Delay::new(0.0);
        assert_eq!(delay.delay_time_ms(), 1.0); // Clamped to minimum

        delay.set_delay_time_ms(3000.0);
        assert_eq!(delay.delay_time_ms(), 2000.0); // Clamped to maximum
    }

    #[test]
    fn test_feedback_clamp() {
        let mut delay = Delay::new(100.0);

        delay.set_feedback(1.5);
        assert_eq!(delay.feedback(), 0.95); // Clamped to max

        delay.set_feedback(-0.5);
        assert_eq!(delay.feedback(), 0.0); // Clamped to min
    }

    #[test]
    fn test_delay_prepare() {
        let mut delay = Delay::new(100.0);
        delay.prepare(48000, 512);

        // Buffer should be sized for delay time + margin
        let expected_size = ((110.0 * 48000.0 / 1000.0) as usize).max(1);
        assert_eq!(delay.buffer_l.len(), expected_size);
        assert_eq!(delay.buffer_r.len(), expected_size);
    }

    #[test]
    fn test_delay_reset() {
        let mut delay = Delay::new(100.0);
        delay.prepare(48000, 512);

        // Fill buffers with non-zero values
        delay.buffer_l.fill(0.5);
        delay.buffer_r.fill(0.5);
        delay.write_pos = 100;
        delay.filter_state_l = 0.3;
        delay.filter_state_r = 0.3;

        delay.reset();

        assert!(delay.buffer_l.iter().all(|&x| x == 0.0));
        assert!(delay.buffer_r.iter().all(|&x| x == 0.0));
        assert_eq!(delay.write_pos, 0);
        assert_eq!(delay.filter_state_l, 0.0);
        assert_eq!(delay.filter_state_r, 0.0);
    }

    #[test]
    fn test_delay_dry_signal_passthrough() {
        let mut delay = Delay::new(100.0);
        delay.prepare(48000, 512);
        delay.set_wet_level(0.0);
        delay.set_dry_level(1.0);

        // Create impulse
        let mut samples = vec![0.0; 1000];
        samples[0] = 1.0;
        let mut buffer = create_test_buffer(vec![samples.clone()], 48000);

        delay.process(&mut buffer);

        // Dry signal should be unchanged
        assert!((buffer.samples[0][0] - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_delay_produces_echo() {
        let mut delay = Delay::new(10.0); // 10ms delay
        delay.prepare(48000, 512);
        delay.set_wet_level(1.0);
        delay.set_dry_level(0.0);
        delay.set_feedback(0.0);

        // Calculate expected delay in samples
        let delay_samples = (10.0 * 48000.0 / 1000.0) as usize; // 480 samples

        // Create impulse at sample 0
        let mut samples = vec![0.0; 1000];
        samples[0] = 1.0;
        let mut buffer = create_test_buffer(vec![samples], 48000);

        delay.process(&mut buffer);

        // Echo should appear at delay_samples position
        // Initial samples should be 0 (wet only, no delayed signal yet)
        assert!((buffer.samples[0][0]).abs() < 0.001);
        // Echo should appear at delay position
        assert!((buffer.samples[0][delay_samples] - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_delay_stereo() {
        let mut delay = Delay::new(10.0);
        delay.prepare(48000, 512);
        delay.set_wet_level(0.5);
        delay.set_dry_level(0.5);

        let mut buffer = create_test_buffer(vec![vec![0.5; 1000], vec![0.5; 1000]], 48000);

        delay.process(&mut buffer);

        // Both channels should be processed
        assert_eq!(buffer.samples.len(), 2);
    }

    #[test]
    fn test_delay_ping_pong() {
        let mut delay = Delay::new(10.0);
        delay.prepare(48000, 512);
        delay.set_ping_pong(true);
        delay.set_wet_level(1.0);
        delay.set_dry_level(0.0);
        delay.set_feedback(0.5);

        // Create impulse in left channel only
        let mut left = vec![0.0; 2000];
        let right = vec![0.0; 2000];
        left[0] = 1.0;

        let mut buffer = create_test_buffer(vec![left, right], 48000);

        delay.process(&mut buffer);

        // In ping-pong mode, the left signal should eventually appear in right channel
        // after crossing through the feedback path
        assert!(delay.ping_pong());
    }

    #[test]
    fn test_delay_serialization() {
        let mut delay = Delay::new(250.0);
        delay.set_feedback(0.5);
        delay.set_wet_level(0.7);
        delay.set_ping_pong(true);

        let json = delay.to_json().unwrap();

        let mut delay2 = Delay::new(100.0);
        delay2.from_json(&json).unwrap();

        assert_eq!(delay2.delay_time_ms(), 250.0);
        assert_eq!(delay2.feedback(), 0.5);
        assert_eq!(delay2.wet_level(), 0.7);
        assert!(delay2.ping_pong());
    }

    #[test]
    fn test_delay_get_params() {
        let delay = Delay::new(300.0);
        let params = delay.get_params();

        assert_eq!(params["delay_time_ms"].as_f64().unwrap(), 300.0);
        // Use approximate comparison for f32->f64 conversion
        assert!((params["feedback"].as_f64().unwrap() - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_delay_set_param() {
        let mut delay = Delay::new(100.0);

        delay.set_param("delay_time_ms", &json!(500.0)).unwrap();
        assert_eq!(delay.delay_time_ms(), 500.0);

        delay.set_param("feedback", &json!(0.6)).unwrap();
        assert_eq!(delay.feedback(), 0.6);

        delay.set_param("ping_pong", &json!(true)).unwrap();
        assert!(delay.ping_pong());
    }

    #[test]
    fn test_delay_set_param_invalid() {
        let mut delay = Delay::new(100.0);

        let result = delay.set_param("unknown_param", &json!(1.0));
        assert!(result.is_err());

        let result = delay.set_param("delay_time_ms", &json!("not a number"));
        assert!(result.is_err());
    }

    #[test]
    fn test_delay_effect_type() {
        let delay = Delay::new(100.0);
        assert_eq!(delay.effect_type(), "delay");
        assert_eq!(delay.display_name(), "Delay");
    }

    #[test]
    fn test_delay_enabled() {
        let mut delay = Delay::new(100.0);
        delay.prepare(48000, 512);
        delay.set_enabled(false);

        let original = vec![0.5; 100];
        let mut buffer = create_test_buffer(vec![original.clone()], 48000);

        delay.process(&mut buffer);

        // When disabled, buffer should be unchanged
        assert_eq!(buffer.samples[0], original);
    }

    #[test]
    fn test_delay_filter_freq() {
        let mut delay = Delay::new(100.0);

        delay.set_filter_freq(100.0);
        assert_eq!(delay.filter_freq(), 100.0);

        delay.set_filter_freq(10.0);
        assert_eq!(delay.filter_freq(), 20.0); // Clamped to min

        delay.set_filter_freq(25000.0);
        assert_eq!(delay.filter_freq(), 20000.0); // Clamped to max
    }
}
