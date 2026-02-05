//! Layer 0 - Immutable Source Storage
//!
//! Layer 0 holds the original audio. NEVER modified after creation
//! except via explicit "bake" operation.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{NuevaError, Result};

/// Audio format information for the original source
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioFormat {
    /// Sample rate in Hz (e.g., 44100, 48000, 96000)
    pub sample_rate: u32,
    /// Bits per sample (e.g., 16, 24, 32)
    pub bits_per_sample: u16,
    /// Number of channels (1 = mono, 2 = stereo)
    pub channels: u16,
    /// Total number of samples per channel
    pub num_samples: u64,
    /// Duration in seconds
    pub duration_secs: f64,
}

impl AudioFormat {
    /// Create AudioFormat from a WAV file
    pub fn from_wav(path: &Path) -> Result<Self> {
        let reader = hound::WavReader::open(path).map_err(|e| NuevaError::InvalidAudio {
            reason: format!("Failed to open WAV file: {}", e),
            source: None,
        })?;

        let spec = reader.spec();
        let num_samples = reader.duration() as u64;
        let duration_secs = num_samples as f64 / spec.sample_rate as f64;

        Ok(Self {
            sample_rate: spec.sample_rate,
            bits_per_sample: spec.bits_per_sample,
            channels: spec.channels,
            num_samples,
            duration_secs,
        })
    }
}

/// Layer 0: Immutable Source Storage
///
/// This layer stores the original audio file reference and metadata.
/// Once created, it is NEVER modified except during a "bake" operation
/// which creates a new Layer 0 from the rendered output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer0 {
    /// Path to the source WAV file
    source_path: PathBuf,
    /// Original import format information
    original_format: AudioFormat,
    /// ISO 8601 timestamp of when this layer was created
    created_at: String,
    /// SHA-256 checksum of the source file for integrity verification
    checksum: String,
}

impl Layer0 {
    /// Create a new Layer 0 from an audio file path
    ///
    /// # Arguments
    /// * `path` - Path to the source audio file (WAV format)
    ///
    /// # Errors
    /// Returns error if:
    /// - File does not exist
    /// - File is not a valid WAV file
    /// - File cannot be read for checksum calculation
    pub fn new(path: PathBuf) -> Result<Self> {
        // Verify file exists
        if !path.exists() {
            return Err(NuevaError::FileNotFound {
                path: path.display().to_string(),
                source: None,
            });
        }

        // Read audio format
        let original_format = AudioFormat::from_wav(&path)?;

        // Calculate checksum
        let checksum = Self::calculate_checksum(&path)?;

        // Get current timestamp
        let created_at = Self::current_timestamp();

        Ok(Self {
            source_path: path,
            original_format,
            created_at,
            checksum,
        })
    }

    /// Get the path to the source audio file
    pub fn get_source_path(&self) -> &Path {
        &self.source_path
    }

    /// Get the original audio format information
    pub fn get_format(&self) -> &AudioFormat {
        &self.original_format
    }

    /// Get the creation timestamp
    pub fn get_created_at(&self) -> &str {
        &self.created_at
    }

    /// Get the checksum
    pub fn get_checksum(&self) -> &str {
        &self.checksum
    }

    /// Verify the integrity of the source file by comparing checksums
    ///
    /// # Returns
    /// - `Ok(true)` if the file matches the stored checksum
    /// - `Ok(false)` if the file has been modified
    /// - `Err(...)` if the file cannot be read
    pub fn verify_integrity(&self) -> Result<bool> {
        if !self.source_path.exists() {
            return Err(NuevaError::FileNotFound {
                path: self.source_path.display().to_string(),
                source: None,
            });
        }

        let current_checksum = Self::calculate_checksum(&self.source_path)?;
        Ok(current_checksum == self.checksum)
    }

    /// Calculate SHA-256 checksum of a file
    fn calculate_checksum(path: &Path) -> Result<String> {
        let mut file = fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let hash = hasher.finalize();
        Ok(format!("{:x}", hash))
    }

    /// Get current ISO 8601 timestamp
    fn current_timestamp() -> String {
        // Using a simple approach without chrono dependency
        // Format: YYYY-MM-DDTHH:MM:SSZ
        use std::time::{SystemTime, UNIX_EPOCH};

        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();

        let secs = duration.as_secs();

        // Calculate date components (simplified UTC calculation)
        let days = secs / 86400;
        let remaining = secs % 86400;
        let hours = remaining / 3600;
        let minutes = (remaining % 3600) / 60;
        let seconds = remaining % 60;

        // Days since 1970-01-01 to year/month/day (simplified)
        let mut year = 1970i32;
        let mut remaining_days = days as i32;

        loop {
            let days_in_year = if is_leap_year(year) { 366 } else { 365 };
            if remaining_days < days_in_year {
                break;
            }
            remaining_days -= days_in_year;
            year += 1;
        }

        let days_in_months: [i32; 12] = if is_leap_year(year) {
            [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        } else {
            [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        };

        let mut month = 1u32;
        for &days_in_month in &days_in_months {
            if remaining_days < days_in_month {
                break;
            }
            remaining_days -= days_in_month;
            month += 1;
        }

        let day = remaining_days + 1;

        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            year, month, day, hours, minutes, seconds
        )
    }
}

/// Check if a year is a leap year
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn create_test_wav(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        // Write 1 second of silence
        for _ in 0..(44100 * 2) {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();
        path
    }

    #[test]
    fn test_layer0_new() {
        let dir = tempdir().unwrap();
        let wav_path = create_test_wav(dir.path(), "test.wav");

        let layer0 = Layer0::new(wav_path.clone()).unwrap();

        assert_eq!(layer0.get_source_path(), wav_path.as_path());
        assert_eq!(layer0.get_format().sample_rate, 44100);
        assert_eq!(layer0.get_format().channels, 2);
        assert!(!layer0.get_checksum().is_empty());
    }

    #[test]
    fn test_layer0_file_not_found() {
        let result = Layer0::new(PathBuf::from("/nonexistent/file.wav"));
        assert!(result.is_err());

        if let Err(NuevaError::FileNotFound { path, .. }) = result {
            assert!(path.contains("nonexistent"));
        } else {
            panic!("Expected FileNotFound error");
        }
    }

    #[test]
    fn test_verify_integrity() {
        let dir = tempdir().unwrap();
        let wav_path = create_test_wav(dir.path(), "integrity_test.wav");

        let layer0 = Layer0::new(wav_path.clone()).unwrap();

        // Integrity should pass initially
        assert!(layer0.verify_integrity().unwrap());

        // Modify the file
        let mut file = fs::OpenOptions::new()
            .write(true)
            .append(true)
            .open(&wav_path)
            .unwrap();
        file.write_all(b"modified").unwrap();

        // Integrity should now fail
        assert!(!layer0.verify_integrity().unwrap());
    }

    #[test]
    fn test_audio_format() {
        let dir = tempdir().unwrap();
        let wav_path = create_test_wav(dir.path(), "format_test.wav");

        let format = AudioFormat::from_wav(&wav_path).unwrap();

        assert_eq!(format.sample_rate, 44100);
        assert_eq!(format.bits_per_sample, 16);
        assert_eq!(format.channels, 2);
        assert_eq!(format.num_samples, 44100);
        // Duration should be approximately 1 second
        assert!((format.duration_secs - 1.0).abs() < 0.01);
    }
}
