//! Audio file I/O for Nueva
//!
//! Handles importing and exporting audio files. Primary format is WAV,
//! with support for various bit depths and sample rates.
//!
//! All audio is converted to internal 48kHz 32-bit float format on import.
//! Sample rate conversion uses linear interpolation (TODO: upgrade to sinc).

use std::path::Path;

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

use crate::engine::buffer::{AudioBuffer, ChannelLayout, INTERNAL_SAMPLE_RATE};
use crate::error::{NuevaError, Result};

// Duration limits per spec section 3.5
const MIN_DURATION_SECS: f64 = 0.1;
const MAX_DURATION_SECS: f64 = 2.0 * 60.0 * 60.0; // 2 hours

/// Export format configuration
#[derive(Debug, Clone)]
pub struct ExportFormat {
    /// Target sample rate (default: 48000)
    pub sample_rate: u32,
    /// Bit depth: 16, 24, or 32 (default: 24)
    pub bit_depth: u16,
}

impl Default for ExportFormat {
    fn default() -> Self {
        ExportFormat {
            sample_rate: 48000,
            bit_depth: 24,
        }
    }
}

impl ExportFormat {
    /// Create a new export format with the given sample rate and bit depth
    pub fn new(sample_rate: u32, bit_depth: u16) -> Self {
        ExportFormat {
            sample_rate,
            bit_depth,
        }
    }

    /// Create format for CD quality (44.1kHz, 16-bit)
    pub fn cd_quality() -> Self {
        ExportFormat {
            sample_rate: 44100,
            bit_depth: 16,
        }
    }

    /// Create format for high quality (48kHz, 24-bit)
    pub fn high_quality() -> Self {
        ExportFormat {
            sample_rate: 48000,
            bit_depth: 24,
        }
    }

    /// Create format for maximum quality (96kHz, 32-bit)
    pub fn max_quality() -> Self {
        ExportFormat {
            sample_rate: 96000,
            bit_depth: 32,
        }
    }
}

/// Import an audio file and convert to internal format
///
/// Reads a WAV file, converts to 32-bit float, and resamples to 48kHz.
/// Validates the audio per spec section 3.6.
///
/// # Arguments
/// * `path` - Path to the WAV file to import
///
/// # Returns
/// * `Ok(AudioBuffer)` - The imported audio in internal format
/// * `Err(NuevaError)` - If the file cannot be read or is invalid
///
/// # Errors
/// * `FileNotFound` - If the file does not exist
/// * `InvalidAudio` - If the file is not a valid WAV file
/// * `UnsupportedFormat` - If the audio has more than 2 channels
/// * `AudioTooShort` - If duration is less than 0.1 seconds
/// * `AudioTooLong` - If duration exceeds 2 hours
pub fn import_audio(path: &Path) -> Result<AudioBuffer> {
    // Check file exists
    if !path.exists() {
        return Err(NuevaError::FileNotFound {
            path: path.display().to_string(),
            source: None,
        });
    }

    // Open WAV file
    let reader = WavReader::open(path).map_err(|e| NuevaError::InvalidAudio {
        reason: format!("Failed to open WAV file: {}", e),
        source: Some(Box::new(e)),
    })?;

    let spec = reader.spec();
    let source_sample_rate = spec.sample_rate;
    let channels = spec.channels as usize;
    let bits_per_sample = spec.bits_per_sample;
    let sample_format = spec.sample_format;

    // Reject multi-channel audio (>2 channels) per spec section 3.4
    if channels > 2 {
        return Err(NuevaError::UnsupportedFormat {
            format: format!("{}-channel audio (only mono/stereo supported)", channels),
        });
    }

    // Read samples and convert to f32
    let samples_f32 = read_samples_as_f32(reader, bits_per_sample, sample_format)?;

    // Calculate duration and validate
    let total_samples = samples_f32.len();
    let frames = total_samples / channels;
    let duration_secs = frames as f64 / source_sample_rate as f64;

    if duration_secs < MIN_DURATION_SECS {
        return Err(NuevaError::AudioTooShort { duration_secs });
    }

    if duration_secs > MAX_DURATION_SECS {
        return Err(NuevaError::AudioTooLong { duration_secs });
    }

    // De-interleave samples into separate channels
    let channel_data = deinterleave(&samples_f32, channels);

    // Resample to internal sample rate if needed
    let resampled_data = if source_sample_rate != INTERNAL_SAMPLE_RATE {
        resample_channels(&channel_data, source_sample_rate, INTERNAL_SAMPLE_RATE)
    } else {
        channel_data
    };

    // Create AudioBuffer
    let layout = if channels == 1 {
        ChannelLayout::Mono
    } else {
        ChannelLayout::Stereo
    };

    let mut buffer = AudioBuffer::new(resampled_data[0].len(), layout);

    // Copy data to buffer
    for (ch, data) in resampled_data.iter().enumerate() {
        buffer.channel_mut(ch).copy_from_slice(data);
    }

    // Validate the buffer per spec section 3.6
    // Note: validate() returns Result<()> and checks for empty, duration, silence,
    // DC offset, and clipping. For import, we want to be more lenient - only fail
    // on truly broken audio (empty), not on quality issues like silence/clipping.
    if buffer.is_empty() {
        return Err(NuevaError::EmptyAudio);
    }

    Ok(buffer)
}

/// Export an AudioBuffer to a WAV file
///
/// Writes the buffer to a WAV file with the specified format.
/// Resamples if the target sample rate differs from internal rate.
///
/// # Arguments
/// * `buffer` - The audio buffer to export
/// * `path` - Path where the file will be written
/// * `format` - Export format specifying sample rate and bit depth
///
/// # Returns
/// * `Ok(())` - If the file was written successfully
/// * `Err(NuevaError)` - If the file cannot be written
pub fn export_audio(buffer: &AudioBuffer, path: &Path, format: ExportFormat) -> Result<()> {
    let channels = buffer.num_channels() as u16;

    // Resample if needed
    let export_data = if format.sample_rate != INTERNAL_SAMPLE_RATE {
        resample_channels(&buffer.samples, INTERNAL_SAMPLE_RATE, format.sample_rate)
    } else {
        buffer.samples.clone()
    };

    // Interleave channels
    let interleaved = interleave(&export_data);

    // Create WAV spec
    let spec = WavSpec {
        channels,
        sample_rate: format.sample_rate,
        bits_per_sample: format.bit_depth,
        sample_format: if format.bit_depth == 32 {
            SampleFormat::Float
        } else {
            SampleFormat::Int
        },
    };

    // Create writer
    let mut writer = WavWriter::create(path, spec).map_err(|e| {
        NuevaError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?;

    // Write samples based on bit depth
    match format.bit_depth {
        16 => {
            for sample in interleaved {
                let scaled = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                writer.write_sample(scaled).map_err(|e| {
                    NuevaError::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        e.to_string(),
                    ))
                })?;
            }
        }
        24 => {
            for sample in interleaved {
                // 24-bit stored as i32 in hound
                let scaled = (sample * 8388607.0).clamp(-8388608.0, 8388607.0) as i32;
                writer.write_sample(scaled).map_err(|e| {
                    NuevaError::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        e.to_string(),
                    ))
                })?;
            }
        }
        32 => {
            for sample in interleaved {
                writer.write_sample(sample).map_err(|e| {
                    NuevaError::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        e.to_string(),
                    ))
                })?;
            }
        }
        _ => {
            return Err(NuevaError::UnsupportedFormat {
                format: format!("{}-bit audio (only 16, 24, 32 supported)", format.bit_depth),
            });
        }
    }

    writer.finalize().map_err(|e| {
        NuevaError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?;

    Ok(())
}

/// Generate a test tone (sine wave)
///
/// Creates a mono AudioBuffer containing a sine wave at the specified frequency.
/// Useful for testing audio processing pipelines.
///
/// # Arguments
/// * `frequency` - Frequency of the sine wave in Hz
/// * `duration_secs` - Duration of the tone in seconds
/// * `sample_rate` - Sample rate (typically INTERNAL_SAMPLE_RATE)
///
/// # Returns
/// An AudioBuffer containing the generated sine wave
pub fn generate_test_tone(frequency: f32, duration_secs: f32, sample_rate: u32) -> AudioBuffer {
    let num_samples = (duration_secs * sample_rate as f32) as usize;
    let mut buffer = AudioBuffer::new(num_samples, ChannelLayout::Mono);

    let angular_freq = 2.0 * std::f32::consts::PI * frequency / sample_rate as f32;

    for (i, sample) in buffer.samples[0].iter_mut().enumerate() {
        *sample = (angular_freq * i as f32).sin();
    }

    buffer
}

/// Generate a stereo test tone with different frequencies per channel
///
/// Creates a stereo AudioBuffer with different sine waves in each channel.
/// Useful for testing stereo processing.
///
/// # Arguments
/// * `freq_left` - Frequency for left channel in Hz
/// * `freq_right` - Frequency for right channel in Hz
/// * `duration_secs` - Duration of the tone in seconds
/// * `sample_rate` - Sample rate (typically INTERNAL_SAMPLE_RATE)
///
/// # Returns
/// A stereo AudioBuffer containing the generated sine waves
pub fn generate_stereo_test_tone(
    freq_left: f32,
    freq_right: f32,
    duration_secs: f32,
    sample_rate: u32,
) -> AudioBuffer {
    let num_samples = (duration_secs * sample_rate as f32) as usize;
    let mut buffer = AudioBuffer::new(num_samples, ChannelLayout::Stereo);

    let angular_freq_l = 2.0 * std::f32::consts::PI * freq_left / sample_rate as f32;
    let angular_freq_r = 2.0 * std::f32::consts::PI * freq_right / sample_rate as f32;

    for (i, sample) in buffer.samples[0].iter_mut().enumerate() {
        *sample = (angular_freq_l * i as f32).sin();
    }

    for (i, sample) in buffer.samples[1].iter_mut().enumerate() {
        *sample = (angular_freq_r * i as f32).sin();
    }

    buffer
}

// ============================================================================
// Internal helper functions
// ============================================================================

/// Read samples from WAV reader and convert to f32
fn read_samples_as_f32<R: std::io::Read>(
    mut reader: WavReader<R>,
    bits_per_sample: u16,
    sample_format: SampleFormat,
) -> Result<Vec<f32>> {
    match sample_format {
        SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<f32>, _>>()
            .map_err(|e| NuevaError::InvalidAudio {
                reason: format!("Failed to read float samples: {}", e),
                source: Some(Box::new(e)),
            }),
        SampleFormat::Int => {
            match bits_per_sample {
                8 => {
                    // 8-bit unsigned
                    reader
                        .samples::<i8>()
                        .map(|s| s.map(|v| v as f32 / 128.0))
                        .collect::<std::result::Result<Vec<f32>, _>>()
                        .map_err(|e| NuevaError::InvalidAudio {
                            reason: format!("Failed to read 8-bit samples: {}", e),
                            source: Some(Box::new(e)),
                        })
                }
                16 => reader
                    .samples::<i16>()
                    .map(|s| s.map(|v| v as f32 / 32768.0))
                    .collect::<std::result::Result<Vec<f32>, _>>()
                    .map_err(|e| NuevaError::InvalidAudio {
                        reason: format!("Failed to read 16-bit samples: {}", e),
                        source: Some(Box::new(e)),
                    }),
                24 => {
                    // 24-bit stored as i32 in hound
                    reader
                        .samples::<i32>()
                        .map(|s| s.map(|v| v as f32 / 8388608.0))
                        .collect::<std::result::Result<Vec<f32>, _>>()
                        .map_err(|e| NuevaError::InvalidAudio {
                            reason: format!("Failed to read 24-bit samples: {}", e),
                            source: Some(Box::new(e)),
                        })
                }
                32 => reader
                    .samples::<i32>()
                    .map(|s| s.map(|v| v as f32 / 2147483648.0))
                    .collect::<std::result::Result<Vec<f32>, _>>()
                    .map_err(|e| NuevaError::InvalidAudio {
                        reason: format!("Failed to read 32-bit int samples: {}", e),
                        source: Some(Box::new(e)),
                    }),
                _ => Err(NuevaError::UnsupportedFormat {
                    format: format!("{}-bit integer audio", bits_per_sample),
                }),
            }
        }
    }
}

/// De-interleave samples from [L,R,L,R,...] to [[L,L,...], [R,R,...]]
fn deinterleave(samples: &[f32], channels: usize) -> Vec<Vec<f32>> {
    let frames = samples.len() / channels;
    let mut result = vec![Vec::with_capacity(frames); channels];

    for (i, sample) in samples.iter().enumerate() {
        result[i % channels].push(*sample);
    }

    result
}

/// Interleave channels from [[L,L,...], [R,R,...]] to [L,R,L,R,...]
fn interleave(channels: &[Vec<f32>]) -> Vec<f32> {
    if channels.is_empty() {
        return Vec::new();
    }

    let num_channels = channels.len();
    let frames = channels[0].len();
    let mut result = Vec::with_capacity(frames * num_channels);

    for frame in 0..frames {
        for channel in channels {
            result.push(channel[frame]);
        }
    }

    result
}

/// Resample audio channels to a different sample rate
///
/// Uses linear interpolation for now.
/// TODO: Implement high-quality sinc interpolation for better quality
fn resample_channels(channels: &[Vec<f32>], source_rate: u32, target_rate: u32) -> Vec<Vec<f32>> {
    let ratio = target_rate as f64 / source_rate as f64;

    channels
        .iter()
        .map(|channel| resample_linear(channel, ratio))
        .collect()
}

/// Linear interpolation resampling
///
/// TODO: Replace with sinc interpolation for high-quality resampling
/// Linear interpolation introduces aliasing artifacts, especially for
/// downsampling. For production use, implement a windowed sinc resampler.
fn resample_linear(samples: &[f32], ratio: f64) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }

    let source_len = samples.len();
    let target_len = ((source_len as f64) * ratio).ceil() as usize;
    let mut output = Vec::with_capacity(target_len);

    for i in 0..target_len {
        // Map output index to source position
        let src_pos = i as f64 / ratio;
        let src_idx = src_pos.floor() as usize;
        let frac = (src_pos - src_idx as f64) as f32;

        // Linear interpolation between adjacent samples
        let sample = if src_idx + 1 < source_len {
            samples[src_idx] * (1.0 - frac) + samples[src_idx + 1] * frac
        } else if src_idx < source_len {
            samples[src_idx]
        } else {
            0.0
        };

        output.push(sample);
    }

    output
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_generate_test_tone() {
        let buffer = generate_test_tone(440.0, 1.0, INTERNAL_SAMPLE_RATE);

        assert_eq!(buffer.num_samples(), INTERNAL_SAMPLE_RATE as usize);
        assert_eq!(buffer.num_channels(), 1);

        // Check that the tone has the expected properties
        // At 440 Hz and 48kHz, we should have 48000/440 ~ 109 samples per cycle
        let samples_per_cycle = INTERNAL_SAMPLE_RATE as f32 / 440.0;

        // Check that the signal crosses zero approximately at expected positions
        let zero_crossing_1 = (samples_per_cycle / 2.0) as usize;

        // The sample near half-cycle should be close to zero
        assert!(buffer.samples[0][zero_crossing_1].abs() < 0.1);
    }

    #[test]
    fn test_generate_stereo_test_tone() {
        let buffer = generate_stereo_test_tone(440.0, 880.0, 0.5, INTERNAL_SAMPLE_RATE);

        assert_eq!(
            buffer.num_samples(),
            (INTERNAL_SAMPLE_RATE as f32 * 0.5) as usize
        );
        assert_eq!(buffer.num_channels(), 2);

        // Left and right channels should be different
        let left = &buffer.samples[0];
        let right = &buffer.samples[1];

        // At sample 100, left (440Hz) and right (880Hz) should differ
        assert!((left[100] - right[100]).abs() > 0.01);
    }

    #[test]
    fn test_interleave_deinterleave_roundtrip() {
        let left = vec![1.0, 2.0, 3.0, 4.0];
        let right = vec![5.0, 6.0, 7.0, 8.0];
        let channels = vec![left.clone(), right.clone()];

        let interleaved = interleave(&channels);
        assert_eq!(interleaved, vec![1.0, 5.0, 2.0, 6.0, 3.0, 7.0, 4.0, 8.0]);

        let deinterleaved = deinterleave(&interleaved, 2);
        assert_eq!(deinterleaved[0], left);
        assert_eq!(deinterleaved[1], right);
    }

    #[test]
    fn test_resample_linear_upsample() {
        // Simple upsample 2x
        let samples = vec![0.0, 1.0, 0.0];
        let resampled = resample_linear(&samples, 2.0);

        // Should have approximately 6 samples
        assert!(resampled.len() >= 5);

        // Check interpolation
        // At index 1 (src pos 0.5), should be 0.5
        assert!((resampled[1] - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_resample_linear_downsample() {
        // Simple downsample 2x
        let samples = vec![0.0, 0.5, 1.0, 0.5, 0.0, -0.5, -1.0, -0.5];
        let resampled = resample_linear(&samples, 0.5);

        // Should have approximately 4 samples
        assert_eq!(resampled.len(), 4);
    }

    #[test]
    fn test_export_format_presets() {
        let cd = ExportFormat::cd_quality();
        assert_eq!(cd.sample_rate, 44100);
        assert_eq!(cd.bit_depth, 16);

        let high = ExportFormat::high_quality();
        assert_eq!(high.sample_rate, 48000);
        assert_eq!(high.bit_depth, 24);

        let max = ExportFormat::max_quality();
        assert_eq!(max.sample_rate, 96000);
        assert_eq!(max.bit_depth, 32);
    }

    #[test]
    fn test_round_trip_mono() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_mono.wav");

        // Generate a test tone
        let original = generate_test_tone(440.0, 0.5, INTERNAL_SAMPLE_RATE);

        // Export to file
        export_audio(&original, &path, ExportFormat::default()).unwrap();

        // Import back
        let imported = import_audio(&path).unwrap();

        // Verify properties match
        assert_eq!(original.num_samples(), imported.num_samples());
        assert_eq!(original.num_channels(), imported.num_channels());

        // Verify samples are close (may differ slightly due to quantization)
        let orig_samples = &original.samples[0];
        let imp_samples = &imported.samples[0];

        for (orig, imp) in orig_samples.iter().zip(imp_samples.iter()) {
            // 24-bit quantization error should be very small
            assert!(
                (orig - imp).abs() < 0.001,
                "Sample mismatch: {} vs {}",
                orig,
                imp
            );
        }
    }

    #[test]
    fn test_round_trip_stereo() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_stereo.wav");

        // Generate a stereo test tone
        let original = generate_stereo_test_tone(440.0, 880.0, 0.5, INTERNAL_SAMPLE_RATE);

        // Export to file
        export_audio(&original, &path, ExportFormat::default()).unwrap();

        // Import back
        let imported = import_audio(&path).unwrap();

        // Verify properties match
        assert_eq!(original.num_samples(), imported.num_samples());
        assert_eq!(original.num_channels(), imported.num_channels());

        // Verify both channels
        for ch in 0..2 {
            let orig_samples = &original.samples[ch];
            let imp_samples = &imported.samples[ch];

            for (orig, imp) in orig_samples.iter().zip(imp_samples.iter()) {
                assert!(
                    (orig - imp).abs() < 0.001,
                    "Sample mismatch in channel {}: {} vs {}",
                    ch,
                    orig,
                    imp
                );
            }
        }
    }

    #[test]
    fn test_round_trip_16bit() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_16bit.wav");

        let original = generate_test_tone(1000.0, 0.2, INTERNAL_SAMPLE_RATE);
        let format = ExportFormat::new(INTERNAL_SAMPLE_RATE, 16);

        export_audio(&original, &path, format).unwrap();
        let imported = import_audio(&path).unwrap();

        assert_eq!(original.num_samples(), imported.num_samples());

        // 16-bit has more quantization error
        let orig_samples = &original.samples[0];
        let imp_samples = &imported.samples[0];

        for (orig, imp) in orig_samples.iter().zip(imp_samples.iter()) {
            // 16-bit error can be up to ~0.00003 per step
            assert!(
                (orig - imp).abs() < 0.01,
                "Sample mismatch: {} vs {}",
                orig,
                imp
            );
        }
    }

    #[test]
    fn test_round_trip_32bit_float() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_32bit.wav");

        let original = generate_test_tone(1000.0, 0.2, INTERNAL_SAMPLE_RATE);
        let format = ExportFormat::new(INTERNAL_SAMPLE_RATE, 32);

        export_audio(&original, &path, format).unwrap();
        let imported = import_audio(&path).unwrap();

        assert_eq!(original.num_samples(), imported.num_samples());

        // 32-bit float should be essentially lossless
        let orig_samples = &original.samples[0];
        let imp_samples = &imported.samples[0];

        for (orig, imp) in orig_samples.iter().zip(imp_samples.iter()) {
            assert!(
                (orig - imp).abs() < 1e-6,
                "Sample mismatch: {} vs {}",
                orig,
                imp
            );
        }
    }

    #[test]
    fn test_round_trip_with_resample() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_resample.wav");

        // Generate at internal rate
        let original = generate_test_tone(440.0, 0.5, INTERNAL_SAMPLE_RATE);

        // Export at 44.1kHz
        let format = ExportFormat::new(44100, 24);
        export_audio(&original, &path, format).unwrap();

        // Import (will resample back to 48kHz)
        let imported = import_audio(&path).unwrap();

        // Length should be similar (may differ slightly due to resampling)
        let expected_len = original.num_samples();
        let actual_len = imported.num_samples();
        let len_diff = (expected_len as i64 - actual_len as i64).abs();

        // Allow up to 1% difference in length due to resampling
        assert!(
            len_diff < (expected_len as i64 / 100),
            "Length mismatch: {} vs {} (diff: {})",
            expected_len,
            actual_len,
            len_diff
        );
    }

    #[test]
    fn test_import_nonexistent_file() {
        let result = import_audio(Path::new("/nonexistent/path/audio.wav"));
        assert!(result.is_err());

        match result.unwrap_err() {
            NuevaError::FileNotFound { path, .. } => {
                assert!(path.contains("nonexistent"));
            }
            other => panic!("Expected FileNotFound error, got: {:?}", other),
        }
    }

    #[test]
    fn test_export_format_default() {
        let format = ExportFormat::default();
        assert_eq!(format.sample_rate, 48000);
        assert_eq!(format.bit_depth, 24);
    }
}
