//! Delay effect implementation (spec section 4.2.5)
//!
//! Features:
//! - Circular buffer with fractional delay via cubic interpolation
//! - Feedback with low-pass filter in feedback path
//! - Ping-pong mode for stereo
//! - Wet/dry mixing

use super::effect::{Effect, EffectMetadata};
use super::AudioBuffer;
use crate::error::{NuevaError, Result};
use serde::{Deserialize, Serialize};

/// Maximum delay time in milliseconds (2 seconds)
const MAX_DELAY_MS: f32 = 2000.0;

/// Minimum delay time in milliseconds
const MIN_DELAY_MS: f32 = 1.0;

/// Maximum feedback (less than 1.0 to prevent infinite buildup)
const MAX_FEEDBACK: f32 = 0.95;

/// Delay effect parameters (spec section 4.2.5)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelayParams {
    /// Delay time in milliseconds (1 to 2000 ms)
    pub delay_time_ms: f32,
    /// Feedback amount (0 to 0.95)
    pub feedback: f32,
    /// Wet signal level (0 to 1)
    pub wet_level: f32,
    /// Dry signal level (0 to 1)
    pub dry_level: f32,
    /// Enable stereo ping-pong mode
    pub ping_pong: bool,
    /// Low-pass filter frequency in feedback path (Hz)
    pub filter_freq: f32,
}

impl Default for DelayParams {
    fn default() -> Self {
        Self {
            delay_time_ms: 250.0,
            feedback: 0.3,
            wet_level: 0.3,
            dry_level: 1.0,
            ping_pong: false,
            filter_freq: 8000.0,
        }
    }
}

impl DelayParams {
    /// Validate all parameters are within spec ranges
    pub fn validate(&self) -> Result<()> {
        if self.delay_time_ms < MIN_DELAY_MS || self.delay_time_ms > MAX_DELAY_MS {
            return Err(NuevaError::InvalidParameter {
                param: "delay_time_ms".to_string(),
                value: self.delay_time_ms.to_string(),
                expected: format!("{} to {} ms", MIN_DELAY_MS, MAX_DELAY_MS),
            });
        }
        if self.feedback < 0.0 || self.feedback > MAX_FEEDBACK {
            return Err(NuevaError::InvalidParameter {
                param: "feedback".to_string(),
                value: self.feedback.to_string(),
                expected: format!("0.0 to {}", MAX_FEEDBACK),
            });
        }
        if self.wet_level < 0.0 || self.wet_level > 1.0 {
            return Err(NuevaError::InvalidParameter {
                param: "wet_level".to_string(),
                value: self.wet_level.to_string(),
                expected: "0.0 to 1.0".to_string(),
            });
        }
        if self.dry_level < 0.0 || self.dry_level > 1.0 {
            return Err(NuevaError::InvalidParameter {
                param: "dry_level".to_string(),
                value: self.dry_level.to_string(),
                expected: "0.0 to 1.0".to_string(),
            });
        }
        if self.filter_freq < 20.0 || self.filter_freq > 20000.0 {
            return Err(NuevaError::InvalidParameter {
                param: "filter_freq".to_string(),
                value: self.filter_freq.to_string(),
                expected: "20 to 20000 Hz".to_string(),
            });
        }
        Ok(())
    }
}

/// Circular delay buffer with interpolation
#[derive(Debug, Clone)]
struct DelayBuffer {
    /// Sample storage
    buffer: Vec<f32>,
    /// Current write position
    write_pos: usize,
    /// Buffer size (must be power of 2 for efficient masking)
    mask: usize,
}

impl DelayBuffer {
    /// Create a new delay buffer with the given size
    fn new(size: usize) -> Self {
        // Round up to next power of 2 for efficient wrapping
        let size = size.next_power_of_two();
        Self {
            buffer: vec![0.0; size],
            write_pos: 0,
            mask: size - 1,
        }
    }

    /// Write a sample to the buffer and advance write position
    fn write(&mut self, sample: f32) {
        self.buffer[self.write_pos] = sample;
        self.write_pos = (self.write_pos + 1) & self.mask;
    }

    /// Read a sample with cubic interpolation at a fractional delay
    ///
    /// A delay of N means reading the sample that was written N samples ago.
    /// - delay of 1 = the most recently written sample
    /// - delay of 2 = the sample written 2 samples ago
    fn read_cubic(&self, delay_samples: f32) -> f32 {
        let delay_int = delay_samples as usize;
        let frac = delay_samples - delay_int as f32;

        // The buffer size
        let size = self.mask + 1;

        // Calculate read position for delay_int samples ago
        // write_pos points to where the NEXT sample will be written
        // The most recently written sample is at (write_pos - 1)
        // A delay of 1 should read that sample
        // A delay of N should read (write_pos - N)
        // We add size to handle negative wraparound
        let idx0 = (self.write_pos + size - delay_int) & self.mask;

        // For cubic interpolation, we need samples at:
        // idx_m1: one sample newer than idx0 (delay - 1)
        // idx0: the main sample (delay)
        // idx_1: one sample older than idx0 (delay + 1)
        // idx_2: two samples older than idx0 (delay + 2)
        let idx_m1 = (idx0 + 1) & self.mask;
        let idx_1 = (idx0 + size - 1) & self.mask;
        let idx_2 = (idx_1 + size - 1) & self.mask;

        let y_m1 = self.buffer[idx_m1];
        let y0 = self.buffer[idx0];
        let y1 = self.buffer[idx_1];
        let y2 = self.buffer[idx_2];

        // Cubic Hermite interpolation
        let c0 = y0;
        let c1 = 0.5 * (y1 - y_m1);
        let c2 = y_m1 - 2.5 * y0 + 2.0 * y1 - 0.5 * y2;
        let c3 = 0.5 * (y2 - y_m1) + 1.5 * (y0 - y1);

        ((c3 * frac + c2) * frac + c1) * frac + c0
    }

    /// Read a sample with linear interpolation at a fractional delay
    #[allow(dead_code)]
    fn read_linear(&self, delay_samples: f32) -> f32 {
        let delay_int = delay_samples as usize;
        let frac = delay_samples - delay_int as f32;

        let size = self.mask + 1;

        // Calculate read position: (write_pos - delay) with wraparound
        let idx0 = (self.write_pos + size - delay_int) & self.mask;
        let idx1 = (idx0 + size - 1) & self.mask;

        let y0 = self.buffer[idx0];
        let y1 = self.buffer[idx1];

        y0 + frac * (y1 - y0)
    }

    /// Clear the buffer
    fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }
}

/// Simple one-pole low-pass filter for feedback path
#[derive(Debug, Clone)]
struct OnePoleFilter {
    /// Filter coefficient
    coeff: f32,
    /// Previous output
    z1: f32,
}

impl OnePoleFilter {
    fn new() -> Self {
        Self {
            coeff: 0.5,
            z1: 0.0,
        }
    }

    /// Update filter coefficient based on cutoff frequency and sample rate
    fn set_frequency(&mut self, freq: f32, sample_rate: f64) {
        // Simple one-pole coefficient calculation
        let w = (2.0 * std::f64::consts::PI * freq as f64 / sample_rate) as f32;
        self.coeff = w / (1.0 + w);
    }

    /// Process a single sample
    fn process(&mut self, input: f32) -> f32 {
        self.z1 = self.z1 + self.coeff * (input - self.z1);
        self.z1
    }

    /// Reset filter state
    fn reset(&mut self) {
        self.z1 = 0.0;
    }
}

/// Delay effect (spec section 4.2.5)
///
/// Implements a digital delay with feedback, low-pass filtering,
/// and optional stereo ping-pong mode.
#[derive(Debug, Clone)]
pub struct Delay {
    /// Effect parameters
    params: DelayParams,
    /// Unique instance ID
    id: String,
    /// Whether the effect is enabled
    enabled: bool,
    /// Current sample rate
    sample_rate: f64,
    /// Delay buffer for left channel
    delay_left: DelayBuffer,
    /// Delay buffer for right channel
    delay_right: DelayBuffer,
    /// Low-pass filter for left channel feedback
    filter_left: OnePoleFilter,
    /// Low-pass filter for right channel feedback
    filter_right: OnePoleFilter,
    /// Feedback sample for left channel (for ping-pong)
    feedback_left: f32,
    /// Feedback sample for right channel (for ping-pong)
    feedback_right: f32,
}

impl Delay {
    /// Create a new Delay effect with default parameters
    pub fn new() -> Self {
        Self::with_params(DelayParams::default())
    }

    /// Create a new Delay effect with the given parameters
    pub fn with_params(params: DelayParams) -> Self {
        // Initialize with reasonable default buffer size (will be resized in prepare)
        let buffer_size = 88200; // ~2 seconds at 44.1kHz
        Self {
            params,
            id: String::new(),
            enabled: true,
            sample_rate: 44100.0,
            delay_left: DelayBuffer::new(buffer_size),
            delay_right: DelayBuffer::new(buffer_size),
            filter_left: OnePoleFilter::new(),
            filter_right: OnePoleFilter::new(),
            feedback_left: 0.0,
            feedback_right: 0.0,
        }
    }

    /// Get a reference to the current parameters
    pub fn params(&self) -> &DelayParams {
        &self.params
    }

    /// Set parameters with validation
    pub fn set_params(&mut self, params: DelayParams) -> Result<()> {
        params.validate()?;
        self.params = params;
        self.update_filters();
        Ok(())
    }

    /// Set delay time in milliseconds
    pub fn set_delay_time(&mut self, ms: f32) -> Result<()> {
        let mut params = self.params.clone();
        params.delay_time_ms = ms;
        self.set_params(params)
    }

    /// Set feedback amount
    pub fn set_feedback(&mut self, feedback: f32) -> Result<()> {
        let mut params = self.params.clone();
        params.feedback = feedback;
        self.set_params(params)
    }

    /// Set wet level
    pub fn set_wet_level(&mut self, level: f32) -> Result<()> {
        let mut params = self.params.clone();
        params.wet_level = level;
        self.set_params(params)
    }

    /// Set dry level
    pub fn set_dry_level(&mut self, level: f32) -> Result<()> {
        let mut params = self.params.clone();
        params.dry_level = level;
        self.set_params(params)
    }

    /// Enable or disable ping-pong mode
    pub fn set_ping_pong(&mut self, enabled: bool) {
        self.params.ping_pong = enabled;
    }

    /// Set filter frequency
    pub fn set_filter_freq(&mut self, freq: f32) -> Result<()> {
        let mut params = self.params.clone();
        params.filter_freq = freq;
        self.set_params(params)?;
        self.update_filters();
        Ok(())
    }

    /// Update filter coefficients
    fn update_filters(&mut self) {
        self.filter_left
            .set_frequency(self.params.filter_freq, self.sample_rate);
        self.filter_right
            .set_frequency(self.params.filter_freq, self.sample_rate);
    }

    /// Calculate delay in samples
    fn delay_samples(&self) -> f32 {
        (self.params.delay_time_ms / 1000.0) * self.sample_rate as f32
    }

    /// Process mono audio
    fn process_mono(&mut self, buffer: &mut AudioBuffer) {
        let delay_samples = self.delay_samples();
        let num_samples = buffer.num_samples();

        for i in 0..num_samples {
            let input = buffer.get(i, 0).unwrap_or(0.0);

            // Read from delay line with interpolation
            let delayed = self.delay_left.read_cubic(delay_samples);

            // Apply feedback filter
            let filtered_feedback = self.filter_left.process(delayed);

            // Write input plus filtered feedback to delay line
            self.delay_left
                .write(input + filtered_feedback * self.params.feedback);

            // Mix dry and wet
            let output = input * self.params.dry_level + delayed * self.params.wet_level;
            buffer.set(i, 0, output);
        }
    }

    /// Process stereo audio (standard mode)
    fn process_stereo(&mut self, buffer: &mut AudioBuffer) {
        let delay_samples = self.delay_samples();
        let num_samples = buffer.num_samples();

        for i in 0..num_samples {
            let input_left = buffer.get(i, 0).unwrap_or(0.0);
            let input_right = buffer.get(i, 1).unwrap_or(0.0);

            // Read from delay lines
            let delayed_left = self.delay_left.read_cubic(delay_samples);
            let delayed_right = self.delay_right.read_cubic(delay_samples);

            // Apply feedback filters
            let filtered_left = self.filter_left.process(delayed_left);
            let filtered_right = self.filter_right.process(delayed_right);

            // Write to delay lines
            self.delay_left
                .write(input_left + filtered_left * self.params.feedback);
            self.delay_right
                .write(input_right + filtered_right * self.params.feedback);

            // Mix dry and wet
            let output_left =
                input_left * self.params.dry_level + delayed_left * self.params.wet_level;
            let output_right =
                input_right * self.params.dry_level + delayed_right * self.params.wet_level;

            buffer.set(i, 0, output_left);
            buffer.set(i, 1, output_right);
        }
    }

    /// Process stereo audio in ping-pong mode
    fn process_ping_pong(&mut self, buffer: &mut AudioBuffer) {
        let delay_samples = self.delay_samples();
        let num_samples = buffer.num_samples();

        for i in 0..num_samples {
            let input_left = buffer.get(i, 0).unwrap_or(0.0);
            let input_right = buffer.get(i, 1).unwrap_or(0.0);

            // Read from delay lines
            let delayed_left = self.delay_left.read_cubic(delay_samples);
            let delayed_right = self.delay_right.read_cubic(delay_samples);

            // Apply feedback filters
            let filtered_left = self.filter_left.process(delayed_left);
            let filtered_right = self.filter_right.process(delayed_right);

            // In ping-pong mode:
            // - Left delay feeds from: mono input + right delay feedback
            // - Right delay feeds from: left delay feedback
            let mono_input = (input_left + input_right) * 0.5;

            // Write to delay lines with cross-feedback (ping-pong)
            self.delay_left
                .write(mono_input + filtered_right * self.params.feedback);
            self.delay_right.write(filtered_left * self.params.feedback);

            // Mix dry and wet
            let output_left =
                input_left * self.params.dry_level + delayed_left * self.params.wet_level;
            let output_right =
                input_right * self.params.dry_level + delayed_right * self.params.wet_level;

            buffer.set(i, 0, output_left);
            buffer.set(i, 1, output_right);
        }
    }
}

impl Default for Delay {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Delay {
    fn process(&mut self, buffer: &mut AudioBuffer) {
        if !self.enabled {
            return;
        }

        match buffer.num_channels() {
            1 => self.process_mono(buffer),
            2 => {
                if self.params.ping_pong {
                    self.process_ping_pong(buffer);
                } else {
                    self.process_stereo(buffer);
                }
            }
            _ => {
                // For multichannel, process first two channels as stereo
                // and leave others unchanged (reasonable fallback)
                if self.params.ping_pong {
                    self.process_ping_pong(buffer);
                } else {
                    self.process_stereo(buffer);
                }
            }
        }
    }

    fn prepare(&mut self, sample_rate: f64, _samples_per_block: usize) {
        self.sample_rate = sample_rate;

        // Resize delay buffers to accommodate maximum delay time
        let max_delay_samples = (MAX_DELAY_MS / 1000.0 * sample_rate as f32) as usize + 4;
        self.delay_left = DelayBuffer::new(max_delay_samples);
        self.delay_right = DelayBuffer::new(max_delay_samples);

        // Update filter coefficients
        self.update_filters();
    }

    fn reset(&mut self) {
        self.delay_left.clear();
        self.delay_right.clear();
        self.filter_left.reset();
        self.filter_right.reset();
        self.feedback_left = 0.0;
        self.feedback_right = 0.0;
    }

    fn to_json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "effect_type": self.effect_type(),
            "id": self.id,
            "enabled": self.enabled,
            "params": {
                "delay_time_ms": self.params.delay_time_ms,
                "feedback": self.params.feedback,
                "wet_level": self.params.wet_level,
                "dry_level": self.params.dry_level,
                "ping_pong": self.params.ping_pong,
                "filter_freq": self.params.filter_freq,
            }
        }))
    }

    fn from_json(&mut self, json: &serde_json::Value) -> Result<()> {
        if let Some(id) = json.get("id").and_then(|v| v.as_str()) {
            self.id = id.to_string();
        }

        if let Some(enabled) = json.get("enabled").and_then(|v| v.as_bool()) {
            self.enabled = enabled;
        }

        if let Some(params) = json.get("params") {
            let mut new_params = self.params.clone();

            if let Some(v) = params.get("delay_time_ms").and_then(|v| v.as_f64()) {
                new_params.delay_time_ms = v as f32;
            }
            if let Some(v) = params.get("feedback").and_then(|v| v.as_f64()) {
                new_params.feedback = v as f32;
            }
            if let Some(v) = params.get("wet_level").and_then(|v| v.as_f64()) {
                new_params.wet_level = v as f32;
            }
            if let Some(v) = params.get("dry_level").and_then(|v| v.as_f64()) {
                new_params.dry_level = v as f32;
            }
            if let Some(v) = params.get("ping_pong").and_then(|v| v.as_bool()) {
                new_params.ping_pong = v;
            }
            if let Some(v) = params.get("filter_freq").and_then(|v| v.as_f64()) {
                new_params.filter_freq = v as f32;
            }

            self.set_params(new_params)?;
        }

        Ok(())
    }

    fn effect_type(&self) -> &'static str {
        "delay"
    }

    fn display_name(&self) -> &'static str {
        "Delay"
    }

    fn metadata(&self) -> EffectMetadata {
        EffectMetadata {
            effect_type: "delay".to_string(),
            display_name: "Delay".to_string(),
            category: "time".to_string(),
            order_priority: 5, // After saturation, before reverb
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
    fn test_delay_default_params() {
        let delay = Delay::new();
        let params = delay.params();

        assert_eq!(params.delay_time_ms, 250.0);
        assert_eq!(params.feedback, 0.3);
        assert_eq!(params.wet_level, 0.3);
        assert_eq!(params.dry_level, 1.0);
        assert!(!params.ping_pong);
        assert_eq!(params.filter_freq, 8000.0);
    }

    #[test]
    fn test_delay_param_validation() {
        // Valid params
        let params = DelayParams::default();
        assert!(params.validate().is_ok());

        // Invalid delay time (too short)
        let mut params = DelayParams::default();
        params.delay_time_ms = 0.5;
        assert!(params.validate().is_err());

        // Invalid delay time (too long)
        params.delay_time_ms = 3000.0;
        assert!(params.validate().is_err());

        // Invalid feedback (too high)
        params = DelayParams::default();
        params.feedback = 1.0;
        assert!(params.validate().is_err());

        // Invalid feedback (negative)
        params.feedback = -0.1;
        assert!(params.validate().is_err());

        // Invalid wet level
        params = DelayParams::default();
        params.wet_level = 1.5;
        assert!(params.validate().is_err());

        // Invalid filter frequency
        params = DelayParams::default();
        params.filter_freq = 10.0;
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_delay_buffer_circular() {
        let mut buffer = DelayBuffer::new(8);

        // Write some samples
        for i in 0..10 {
            buffer.write(i as f32);
        }

        // The buffer should have wrapped around
        // Last 8 samples should be 2,3,4,5,6,7,8,9
        // Reading with delay of 1 should give the most recent sample (9)
        let sample = buffer.read_cubic(1.0);
        assert!((sample - 9.0).abs() < 0.1);
    }

    #[test]
    fn test_delay_buffer_interpolation() {
        let mut buffer = DelayBuffer::new(16);

        // Write a ramp
        for i in 0..16 {
            buffer.write(i as f32);
        }

        // After writing 0..16, write_pos = 0 (wrapped), buffer = [0,1,2,...,15]
        // A delay of 4 reads from (0 + 16 - 4) & 15 = 12, which has value 12
        let sample = buffer.read_cubic(4.0);
        assert!((sample - 12.0).abs() < 0.1);

        // Fractional position should interpolate between adjacent samples
        // delay of 4.5 interpolates between samples at delay 4 (value 12) and delay 5 (value 11)
        let sample = buffer.read_cubic(4.5);
        assert!((sample - 11.5).abs() < 0.2);
    }

    #[test]
    fn test_delay_process_mono() {
        let mut delay = Delay::new();
        delay.prepare(44100.0, 512);

        // Create a buffer with an impulse
        let mut buffer = AudioBuffer::new(1, 1000, 44100.0);
        buffer.set(0, 0, 1.0);

        // Process
        delay.process(&mut buffer);

        // First sample should have dry signal
        let first = buffer.get(0, 0).unwrap();
        assert!((first - 1.0).abs() < 0.01);

        // Sample at delay time should have wet signal
        // delay_time_ms = 250, so delay in samples = 250/1000 * 44100 = 11025
        // But our buffer is only 1000 samples, so we won't see the delayed impulse
        // in this test. Let's use a shorter delay for testing.
    }

    #[test]
    fn test_delay_short_delay() {
        let mut delay = Delay::with_params(DelayParams {
            delay_time_ms: 10.0, // 10ms = ~441 samples at 44.1kHz
            feedback: 0.0,       // No feedback for simple test
            wet_level: 1.0,
            dry_level: 0.0, // Only wet signal
            ping_pong: false,
            filter_freq: 20000.0, // High frequency = minimal filtering
        });
        delay.prepare(44100.0, 512);

        // Create a buffer with an impulse
        let mut buffer = AudioBuffer::new(1, 1000, 44100.0);
        buffer.set(0, 0, 1.0);

        // Process
        delay.process(&mut buffer);

        // The impulse should appear at approximately sample 441 (10ms delay)
        let delay_sample = (10.0 / 1000.0 * 44100.0) as usize;

        // First sample should be near zero (only wet, which is delayed)
        let first = buffer.get(0, 0).unwrap();
        assert!(first.abs() < 0.01);

        // Sample at delay position should have the impulse
        let delayed = buffer.get(delay_sample, 0).unwrap();
        assert!(delayed.abs() > 0.5);
    }

    #[test]
    fn test_delay_feedback() {
        let mut delay = Delay::with_params(DelayParams {
            delay_time_ms: 10.0,
            feedback: 0.5,
            wet_level: 1.0,
            dry_level: 0.0,
            ping_pong: false,
            filter_freq: 20000.0,
        });
        delay.prepare(44100.0, 512);

        // Create a buffer with an impulse
        let mut buffer = AudioBuffer::new(1, 2000, 44100.0);
        buffer.set(0, 0, 1.0);

        // Process
        delay.process(&mut buffer);

        let delay_sample = (10.0 / 1000.0 * 44100.0) as usize;

        // First echo
        let echo1 = buffer.get(delay_sample, 0).unwrap();
        // Second echo (at 2x delay time) should be ~0.5x the first
        let echo2 = buffer.get(delay_sample * 2, 0).unwrap();

        // The ratio should be approximately the feedback amount
        assert!(echo1.abs() > 0.5);
        assert!(echo2.abs() < echo1.abs());
        assert!((echo2.abs() / echo1.abs() - 0.5).abs() < 0.2);
    }

    #[test]
    fn test_delay_stereo() {
        let mut delay = Delay::with_params(DelayParams {
            delay_time_ms: 10.0,
            feedback: 0.0,
            wet_level: 1.0,
            dry_level: 0.0,
            ping_pong: false,
            filter_freq: 20000.0,
        });
        delay.prepare(44100.0, 512);

        // Create a stereo buffer with different impulses on L/R
        let mut buffer = AudioBuffer::new(2, 1000, 44100.0);
        buffer.set(0, 0, 1.0); // Left impulse
        buffer.set(0, 1, 0.5); // Right impulse (different amplitude)

        // Process
        delay.process(&mut buffer);

        let delay_sample = (10.0 / 1000.0 * 44100.0) as usize;

        // Check that left and right are processed independently
        let left_echo = buffer.get(delay_sample, 0).unwrap();
        let right_echo = buffer.get(delay_sample, 1).unwrap();

        assert!(left_echo.abs() > 0.5);
        assert!(right_echo.abs() > 0.2);
        // Ratio should be preserved
        assert!((left_echo.abs() / right_echo.abs() - 2.0).abs() < 0.3);
    }

    #[test]
    fn test_delay_ping_pong() {
        let mut delay = Delay::with_params(DelayParams {
            delay_time_ms: 10.0,
            feedback: 0.5,
            wet_level: 1.0,
            dry_level: 0.0,
            ping_pong: true,
            filter_freq: 20000.0,
        });
        delay.prepare(44100.0, 512);

        // Create a stereo buffer with an impulse
        let mut buffer = AudioBuffer::new(2, 2000, 44100.0);
        buffer.set(0, 0, 1.0);
        buffer.set(0, 1, 1.0);

        // Process
        delay.process(&mut buffer);

        let delay_sample = (10.0 / 1000.0 * 44100.0) as usize;

        // In ping-pong mode, first echo should be on left
        let left_echo1 = buffer.get(delay_sample, 0).unwrap();
        let _right_echo1 = buffer.get(delay_sample, 1).unwrap();

        // Second echo should be on right
        let _left_echo2 = buffer.get(delay_sample * 2, 0).unwrap();
        let right_echo2 = buffer.get(delay_sample * 2, 1).unwrap();

        // Verify ping-pong behavior: the echoes alternate between channels
        assert!(left_echo1.abs() > 0.3); // First echo on left (from mono sum)
        assert!(right_echo2.abs() > 0.1); // Second echo appears on right
    }

    #[test]
    fn test_delay_reset() {
        let mut delay = Delay::new();
        delay.prepare(44100.0, 512);

        // Process some audio
        let mut buffer = AudioBuffer::new(1, 100, 44100.0);
        buffer.set(0, 0, 1.0);
        delay.process(&mut buffer);

        // Reset
        delay.reset();

        // Create a silent buffer
        let mut buffer = AudioBuffer::new(1, 100, 44100.0);
        delay.process(&mut buffer);

        // All samples should be near zero after reset
        for i in 0..100 {
            let sample = buffer.get(i, 0).unwrap();
            assert!(sample.abs() < 0.01);
        }
    }

    #[test]
    fn test_delay_json_serialization() {
        let mut delay = Delay::new();
        delay.set_id("delay-1".to_string());
        delay
            .set_params(DelayParams {
                delay_time_ms: 500.0,
                feedback: 0.6,
                wet_level: 0.5,
                dry_level: 0.8,
                ping_pong: true,
                filter_freq: 5000.0,
            })
            .unwrap();

        // Serialize
        let json = delay.to_json().unwrap();

        // Create a new delay and deserialize
        let mut delay2 = Delay::new();
        delay2.from_json(&json).unwrap();

        assert_eq!(delay2.id(), "delay-1");
        assert_eq!(delay2.params().delay_time_ms, 500.0);
        assert_eq!(delay2.params().feedback, 0.6);
        assert_eq!(delay2.params().wet_level, 0.5);
        assert_eq!(delay2.params().dry_level, 0.8);
        assert!(delay2.params().ping_pong);
        assert_eq!(delay2.params().filter_freq, 5000.0);
    }

    #[test]
    fn test_delay_effect_trait() {
        let delay = Delay::new();

        assert_eq!(delay.effect_type(), "delay");
        assert_eq!(delay.display_name(), "Delay");
        assert!(delay.is_enabled());

        let metadata = delay.metadata();
        assert_eq!(metadata.effect_type, "delay");
        assert_eq!(metadata.category, "time");
    }

    #[test]
    fn test_delay_enabled_disabled() {
        let mut delay = Delay::with_params(DelayParams {
            delay_time_ms: 10.0,
            feedback: 0.0,
            wet_level: 1.0,
            dry_level: 0.0,
            ping_pong: false,
            filter_freq: 20000.0,
        });
        delay.prepare(44100.0, 512);

        // Disable the effect
        delay.set_enabled(false);

        // Create a buffer with an impulse
        let mut buffer = AudioBuffer::new(1, 1000, 44100.0);
        buffer.set(0, 0, 1.0);

        // Process
        delay.process(&mut buffer);

        // When disabled, the buffer should be unchanged
        let first = buffer.get(0, 0).unwrap();
        assert!((first - 1.0).abs() < 0.01);

        // No delayed signal should appear
        let delay_sample = (10.0 / 1000.0 * 44100.0) as usize;
        let delayed = buffer.get(delay_sample, 0).unwrap();
        assert!(delayed.abs() < 0.01);
    }

    #[test]
    fn test_one_pole_filter() {
        let mut filter = OnePoleFilter::new();
        filter.set_frequency(1000.0, 44100.0);

        // Process an impulse
        let out1 = filter.process(1.0);
        let out2 = filter.process(0.0);
        let out3 = filter.process(0.0);

        // Filter should smooth the impulse
        assert!(out1 > 0.0 && out1 < 1.0);
        assert!(out2 > 0.0 && out2 < out1);
        assert!(out3 > 0.0 && out3 < out2);
    }

    #[test]
    fn test_delay_dry_wet_mix() {
        let mut delay = Delay::with_params(DelayParams {
            delay_time_ms: 100.0, // Far enough that we won't see the echo
            feedback: 0.0,
            wet_level: 0.5,
            dry_level: 0.5,
            ping_pong: false,
            filter_freq: 20000.0,
        });
        delay.prepare(44100.0, 512);

        // Create a buffer with a constant signal
        let mut buffer = AudioBuffer::new(1, 100, 44100.0);
        for i in 0..100 {
            buffer.set(i, 0, 1.0);
        }

        // Process
        delay.process(&mut buffer);

        // First sample should be 0.5 (dry only, wet is delayed)
        let first = buffer.get(0, 0).unwrap();
        assert!((first - 0.5).abs() < 0.01);
    }
}
