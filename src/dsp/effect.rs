//! Effect trait definition
//!
//! Base trait for all DSP effects per spec §4.1.

use crate::engine::AudioBuffer;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Parameters common to all effects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectParams {
    /// Unique identifier for this effect instance
    pub id: String,
    /// Whether the effect is enabled
    pub enabled: bool,
}

impl Default for EffectParams {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            enabled: true,
        }
    }
}

/// Base trait for all DSP effects
///
/// Effects process audio buffers in-place and can be serialized
/// for project state persistence.
pub trait Effect: Send + Sync {
    /// Process audio buffer in-place
    fn process(&mut self, buffer: &mut AudioBuffer);

    /// Prepare the effect for processing
    ///
    /// Called when sample rate or block size changes.
    fn prepare(&mut self, sample_rate: u32, max_block_size: usize);

    /// Reset effect state
    ///
    /// Clears any internal buffers/state (e.g., filter history, delay lines).
    fn reset(&mut self);

    /// Get the effect type identifier
    fn effect_type(&self) -> &'static str;

    /// Get human-readable display name
    fn display_name(&self) -> &str;

    /// Get the unique instance ID
    fn id(&self) -> &str;

    /// Set the unique instance ID
    fn set_id(&mut self, id: String);

    /// Check if effect is enabled
    fn is_enabled(&self) -> bool;

    /// Enable or disable the effect
    fn set_enabled(&mut self, enabled: bool);

    /// Serialize effect parameters to JSON
    fn to_json(&self) -> Result<Value>;

    /// Deserialize effect parameters from JSON
    fn from_json(&mut self, json: &Value) -> Result<()>;

    /// Get all parameters as JSON (for UI/agent)
    fn get_params(&self) -> Value;

    /// Set a single parameter by name
    fn set_param(&mut self, name: &str, value: &Value) -> Result<()>;

    /// Clone the effect into a boxed trait object
    fn box_clone(&self) -> Box<dyn Effect>;
}

impl Clone for Box<dyn Effect> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

/// Helper macro to implement common Effect trait methods
#[macro_export]
macro_rules! impl_effect_common {
    ($type:ty, $effect_type:expr, $display_name:expr) => {
        fn effect_type(&self) -> &'static str {
            $effect_type
        }

        fn display_name(&self) -> &str {
            $display_name
        }

        fn id(&self) -> &str {
            &self.params.id
        }

        fn set_id(&mut self, id: String) {
            self.params.id = id;
        }

        fn is_enabled(&self) -> bool {
            self.params.enabled
        }

        fn set_enabled(&mut self, enabled: bool) {
            self.params.enabled = enabled;
        }

        fn box_clone(&self) -> Box<dyn Effect> {
            Box::new(self.clone())
        }
    };
}
