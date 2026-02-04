//! Audio file I/O operations
//!
//! Handles loading and saving WAV files using the hound crate.

use crate::audio::AudioBuffer;
use crate::error::{NuevaError, Result};
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use std::path::Path;

/// Load a WAV file into an AudioBuffer
pub fn load_wav<P: AsRef<Path>>(path: P) -> Result<AudioBuffer> {
    let path = path.as_ref();
    let reader = WavReader::open(path).map_err(|e| NuevaError::AudioReadError {
        path: path.display().to_string(),
        source: e,
    })?;

    let spec = reader.spec();
    let channels = spec.channels;
    let sample_rate = spec.sample_rate;

    let samples: Vec<f32> = match spec.sample_format {
        SampleFormat::Float => reader
            .into_samples::<f32>()
            .map(|s| s.map_err(|e| NuevaError::AudioReadError {
                path: path.display().to_string(),
                source: e,
            }))
            .collect::<Result<Vec<f32>>>()?,
        SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            let max_val = (1u32 << (bits - 1)) as f32;
            reader
                .into_samples::<i32>()
                .map(|s| {
                    s.map(|v| v as f32 / max_val)
                        .map_err(|e| NuevaError::AudioReadError {
                            path: path.display().to_string(),
                            source: e,
                        })
                })
                .collect::<Result<Vec<f32>>>()?
        }
    };

    AudioBuffer::new(samples, channels, sample_rate)
}

/// Save an AudioBuffer to a WAV file (32-bit float)
pub fn save_wav<P: AsRef<Path>>(buffer: &AudioBuffer, path: P) -> Result<()> {
    let path = path.as_ref();
    let spec = WavSpec {
        channels: buffer.channels(),
        sample_rate: buffer.sample_rate(),
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };

    let mut writer = WavWriter::create(path, spec).map_err(|e| NuevaError::AudioWriteError {
        path: path.display().to_string(),
        source: e,
    })?;

    for &sample in buffer.samples() {
        writer
            .write_sample(sample)
            .map_err(|e| NuevaError::AudioWriteError {
                path: path.display().to_string(),
                source: e,
            })?;
    }

    writer.finalize().map_err(|e| NuevaError::AudioWriteError {
        path: path.display().to_string(),
        source: e,
    })?;

    Ok(())
}

/// Save an AudioBuffer to a WAV file with specific bit depth
pub fn save_wav_with_depth<P: AsRef<Path>>(
    buffer: &AudioBuffer,
    path: P,
    bits: u16,
) -> Result<()> {
    let path = path.as_ref();

    if bits == 32 {
        return save_wav(buffer, path);
    }

    let spec = WavSpec {
        channels: buffer.channels(),
        sample_rate: buffer.sample_rate(),
        bits_per_sample: bits,
        sample_format: SampleFormat::Int,
    };

    let mut writer = WavWriter::create(path, spec).map_err(|e| NuevaError::AudioWriteError {
        path: path.display().to_string(),
        source: e,
    })?;

    let max_val = ((1u32 << (bits - 1)) - 1) as f32;

    for &sample in buffer.samples() {
        let clamped = sample.clamp(-1.0, 1.0);
        let int_sample = (clamped * max_val) as i32;
        writer
            .write_sample(int_sample)
            .map_err(|e| NuevaError::AudioWriteError {
                path: path.display().to_string(),
                source: e,
            })?;
    }

    writer.finalize().map_err(|e| NuevaError::AudioWriteError {
        path: path.display().to_string(),
        source: e,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_wav_round_trip_float() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.wav");

        let original = AudioBuffer::sine_wave(440.0, 0.5, 44100);
        save_wav(&original, &path).unwrap();

        let loaded = load_wav(&path).unwrap();

        assert_eq!(original.channels(), loaded.channels());
        assert_eq!(original.sample_rate(), loaded.sample_rate());
        assert_eq!(original.num_frames(), loaded.num_frames());
        assert!(original.is_approx_equal(&loaded, 1e-6));
    }

    #[test]
    fn test_wav_round_trip_16bit() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_16bit.wav");

        let original = AudioBuffer::sine_wave(440.0, 0.5, 44100);
        save_wav_with_depth(&original, &path, 16).unwrap();

        let loaded = load_wav(&path).unwrap();

        assert_eq!(original.channels(), loaded.channels());
        assert_eq!(original.sample_rate(), loaded.sample_rate());
        // 16-bit has less precision, allow larger tolerance
        assert!(original.is_approx_equal(&loaded, 1e-4));
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = load_wav("nonexistent_file.wav");
        assert!(matches!(result, Err(NuevaError::AudioReadError { .. })));
    }
}
