//! Effect trait and types (spec §4.1)

use super::AudioBuffer;
use crate::error::Result;
use serde::{Deserialize, Serialize};

/// Result of processing an effect
#[derive(Debug, Clone)]
pub enum ProcessResult {
    /// Processing succeeded
    Success,
    /// Processing failed, buffer was rolled back
    Failure(String),
    /// Processing succeeded but with warnings
    Warning(String),
}

impl ProcessResult {
    pub fn success() -> Self {
        ProcessResult::Success
    }

    pub fn failure(msg: impl Into<String>) -> Self {
        ProcessResult::Failure(msg.into())
    }

    pub fn warning(msg: impl Into<String>) -> Self {
        ProcessResult::Warning(msg.into())
    }

    pub fn is_success(&self) -> bool {
        matches!(self, ProcessResult::Success | ProcessResult::Warning(_))
    }
}

/// Metadata about an effect type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectMetadata {
    /// Type name (e.g., "compressor", "eq")
    pub effect_type: String,
    /// Display name for UI (e.g., "Parametric EQ")
    pub display_name: String,
    /// Category for ordering (e.g., "dynamics", "eq", "time", "utility")
    pub category: String,
    /// Default chain position priority (lower = earlier in chain)
    pub order_priority: u32,
}

/// Core Effect trait (spec §4.1)
///
/// All DSP effects implement this trait for uniform processing.
pub trait Effect: Send + Sync {
    /// Process audio in-place
    fn process(&mut self, buffer: &mut AudioBuffer);

    /// Prepare the effect for processing at the given sample rate and block size
    fn prepare(&mut self, sample_rate: f64, samples_per_block: usize);

    /// Reset all internal state (delay lines, envelope followers, etc.)
    fn reset(&mut self);

    /// Serialize effect state to JSON
    fn to_json(&self) -> Result<serde_json::Value>;

    /// Deserialize effect state from JSON
    fn from_json(&mut self, json: &serde_json::Value) -> Result<()>;

    /// Get effect type identifier (kebab-case per spec)
    fn effect_type(&self) -> &'static str;

    /// Get display name for UI
    fn display_name(&self) -> &'static str;

    /// Get effect metadata
    fn metadata(&self) -> EffectMetadata;

    /// Whether the effect is currently enabled
    fn is_enabled(&self) -> bool;

    /// Enable or disable the effect
    fn set_enabled(&mut self, enabled: bool);

    /// Get the unique instance ID
    fn id(&self) -> &str;

    /// Set the unique instance ID
    fn set_id(&mut self, id: String);

    /// Process with safety wrapper (spec §9.4)
    ///
    /// Validates output and rolls back if invalid.
    fn process_safe(&mut self, buffer: &mut AudioBuffer) -> ProcessResult {
        if !self.is_enabled() {
            return ProcessResult::Success;
        }

        // Create backup for rollback
        let backup = buffer.create_copy();

        // Process
        self.process(buffer);

        // Validate output
        if !buffer.is_valid() {
            // Rollback
            *buffer = backup;
            return ProcessResult::failure(format!(
                "Effect '{}' produced invalid audio (NaN/Inf/extreme values)",
                self.id()
            ));
        }

        // Check for clipping warning
        let clipping = buffer.clipping_ratio();
        if clipping > 0.01 {
            return ProcessResult::warning(format!(
                "Effect '{}' caused {:.1}% clipping",
                self.id(),
                clipping * 100.0
            ));
        }

        ProcessResult::Success
    }
}

/// Helper to generate unique effect IDs
#[allow(dead_code)]
pub fn generate_effect_id(effect_type: &str, index: usize) -> String {
    format!("{}-{}", effect_type, index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_result() {
        assert!(ProcessResult::success().is_success());
        assert!(ProcessResult::warning("test").is_success());
        assert!(!ProcessResult::failure("test").is_success());
    }

    #[test]
    fn test_generate_id() {
        assert_eq!(generate_effect_id("parametric-eq", 1), "parametric-eq-1");
    }
}
