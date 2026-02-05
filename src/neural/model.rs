//! Neural model trait and core types
//!
//! Defines the interface all neural models must implement.

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Parameters for neural model processing
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NeuralModelParams {
    /// Model-specific parameters as key-value pairs
    #[serde(flatten)]
    pub params: HashMap<String, serde_json::Value>,
}

impl NeuralModelParams {
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }

    pub fn with_param<V: Serialize>(mut self, key: &str, value: V) -> Self {
        self.params
            .insert(key.to_string(), serde_json::to_value(value).unwrap());
        self
    }

    pub fn get<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        self.params
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn get_f32(&self, key: &str) -> Option<f32> {
        self.get::<f64>(key).map(|v| v as f32)
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        self.get(key)
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key)
    }
}

/// Result of neural model processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingResult {
    /// Whether processing succeeded
    pub success: bool,

    /// Path to output audio file
    pub output_path: Option<String>,

    /// Processing time in milliseconds
    pub processing_time_ms: u64,

    /// Human-readable description of what was done
    pub description: String,

    /// Intentional artifacts introduced (for context tracking)
    pub intentional_artifacts: Vec<String>,

    /// Any warnings or notes
    pub warnings: Vec<String>,

    /// Detailed metadata about the processing
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ProcessingResult {
    pub fn success(output_path: String, description: String, time_ms: u64) -> Self {
        Self {
            success: true,
            output_path: Some(output_path),
            processing_time_ms: time_ms,
            description,
            intentional_artifacts: Vec::new(),
            warnings: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.intentional_artifacts = artifacts;
        self
    }

    pub fn with_warnings(mut self, warnings: Vec<String>) -> Self {
        self.warnings = warnings;
        self
    }

    pub fn failure(description: String) -> Self {
        Self {
            success: false,
            output_path: None,
            processing_time_ms: 0,
            description,
            intentional_artifacts: Vec::new(),
            warnings: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Information about a neural model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuralModelInfo {
    /// Model identifier (e.g., "style-transfer", "denoise")
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Model version
    pub version: String,

    /// Description of what the model does
    pub description: String,

    /// Capabilities list
    pub capabilities: Vec<String>,

    /// When to use this model (guidance for agent)
    pub use_when: Vec<String>,

    /// Known limitations
    pub limitations: Vec<String>,

    /// Known artifacts/issues at edge cases
    pub known_artifacts: Vec<String>,

    /// VRAM requirement in GB
    pub vram_requirement_gb: f32,

    /// Typical inference time description
    pub inference_time: String,

    /// Supported input parameters
    pub supported_params: Vec<ParamSpec>,
}

/// Specification for a model parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSpec {
    pub name: String,
    pub param_type: ParamType,
    pub description: String,
    pub default: Option<serde_json::Value>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ParamType {
    Float { min: f32, max: f32 },
    Int { min: i32, max: i32 },
    Bool,
    String,
    Enum { options: Vec<String> },
}

/// Trait that all neural models must implement
pub trait NeuralModel: Send + Sync {
    /// Get model information
    fn info(&self) -> &NeuralModelInfo;

    /// Process audio through the model
    ///
    /// # Arguments
    /// * `input_path` - Path to input audio file
    /// * `output_path` - Path where output should be written
    /// * `params` - Model-specific parameters
    ///
    /// # Returns
    /// Processing result with metadata
    fn process(
        &self,
        input_path: &Path,
        output_path: &Path,
        params: &NeuralModelParams,
    ) -> Result<ProcessingResult>;

    /// Check if the model is ready to use
    fn is_available(&self) -> bool {
        true
    }

    /// Get model ID (convenience method)
    fn id(&self) -> &str {
        &self.info().id
    }

    /// Validate parameters before processing
    fn validate_params(&self, params: &NeuralModelParams) -> Result<()> {
        // Default implementation: no validation
        let _ = params;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_params_builder() {
        let params = NeuralModelParams::new()
            .with_param("intensity", 0.5f32)
            .with_param("preset", "vintage_analog")
            .with_param("preserve_transients", true);

        assert_eq!(params.get_f32("intensity"), Some(0.5));
        assert_eq!(
            params.get_string("preset"),
            Some("vintage_analog".to_string())
        );
        assert_eq!(params.get_bool("preserve_transients"), Some(true));
    }

    #[test]
    fn test_processing_result_success() {
        let result = ProcessingResult::success(
            "/tmp/out.wav".to_string(),
            "Applied style transfer".to_string(),
            1500,
        )
        .with_artifacts(vec!["tape_hiss".to_string()]);

        assert!(result.success);
        assert_eq!(result.intentional_artifacts.len(), 1);
    }
}
