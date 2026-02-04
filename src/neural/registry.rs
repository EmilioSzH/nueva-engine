//! Neural model registry
//!
//! Manages available neural models and their metadata.
//! Implements §5.3 from the spec.

use super::model::{NeuralModel, NeuralModelInfo, ParamSpec};
use crate::error::{NuevaError, Result};
use std::collections::HashMap;
use std::sync::Arc;

/// Registry of available neural models
pub struct NeuralModelRegistry {
    models: HashMap<String, Arc<dyn NeuralModel>>,
    model_info: HashMap<String, NeuralModelInfo>,
}

impl NeuralModelRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
            model_info: HashMap::new(),
        }
    }

    /// Create registry with all default models (including mocks)
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // Register mock models
        registry.register(Arc::new(super::mock::MockStyleTransfer::new()));
        registry.register(Arc::new(super::mock::MockDenoise::new()));
        registry.register(Arc::new(super::mock::MockRestore::new()));
        registry.register(Arc::new(super::mock::MockEnhance::new()));
        registry.register(Arc::new(super::mock::MockAceStep::new()));

        registry
    }

    /// Register a model
    pub fn register(&mut self, model: Arc<dyn NeuralModel>) {
        let info = model.info().clone();
        let id = info.id.clone();
        self.model_info.insert(id.clone(), info);
        self.models.insert(id, model);
    }

    /// Get a model by ID
    pub fn get(&self, id: &str) -> Result<Arc<dyn NeuralModel>> {
        self.models
            .get(id)
            .cloned()
            .ok_or_else(|| NuevaError::UnknownModel {
                model: id.to_string(),
            })
    }

    /// Get model info by ID
    pub fn get_info(&self, id: &str) -> Option<&NeuralModelInfo> {
        self.model_info.get(id)
    }

    /// List all registered model IDs
    pub fn list_models(&self) -> Vec<&str> {
        self.models.keys().map(|s| s.as_str()).collect()
    }

    /// List all model info
    pub fn list_model_info(&self) -> Vec<&NeuralModelInfo> {
        self.model_info.values().collect()
    }

    /// Check if a model is registered
    pub fn has_model(&self, id: &str) -> bool {
        self.models.contains_key(id)
    }

    /// Get the best model for a given capability
    pub fn find_model_for_capability(&self, capability: &str) -> Option<&str> {
        for (id, info) in &self.model_info {
            if info
                .capabilities
                .iter()
                .any(|c| c.to_lowercase().contains(&capability.to_lowercase()))
            {
                return Some(id.as_str());
            }
        }
        None
    }

    /// Get models that match a use-case description
    pub fn suggest_models_for(&self, description: &str) -> Vec<&NeuralModelInfo> {
        let desc_lower = description.to_lowercase();
        self.model_info
            .values()
            .filter(|info| {
                info.use_when
                    .iter()
                    .any(|u| desc_lower.contains(&u.to_lowercase()))
                    || info
                        .description
                        .to_lowercase()
                        .contains(&desc_lower)
            })
            .collect()
    }
}

impl Default for NeuralModelRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Create model info for a standard model from the spec
pub fn create_model_info(
    id: &str,
    name: &str,
    version: &str,
    description: &str,
    capabilities: Vec<&str>,
    use_when: Vec<&str>,
    limitations: Vec<&str>,
    known_artifacts: Vec<&str>,
    vram_gb: f32,
    inference_time: &str,
    params: Vec<ParamSpec>,
) -> NeuralModelInfo {
    NeuralModelInfo {
        id: id.to_string(),
        name: name.to_string(),
        version: version.to_string(),
        description: description.to_string(),
        capabilities: capabilities.into_iter().map(String::from).collect(),
        use_when: use_when.into_iter().map(String::from).collect(),
        limitations: limitations.into_iter().map(String::from).collect(),
        known_artifacts: known_artifacts.into_iter().map(String::from).collect(),
        vram_requirement_gb: vram_gb,
        inference_time: inference_time.to_string(),
        supported_params: params,
    }
}

/// Standard style transfer presets from spec §5.3
pub const STYLE_TRANSFER_PRESETS: &[&str] = &[
    "vintage_analog",
    "lo_fi",
    "modern_clean",
    "tape_warmth",
    "vinyl_crackle",
    "tube_console",
    "transistor_radio",
    "abbey_road_60s",
    "motown",
    "80s_digital",
    "90s_grunge",
];

/// Standard denoise noise types from spec
pub const DENOISE_NOISE_TYPES: &[&str] = &["auto", "broadband", "tonal", "impulse"];

/// Standard restore modes from spec
pub const RESTORE_MODES: &[&str] = &[
    "auto",
    "declip",
    "dehum",
    "declick",
    "decrackle",
    "extend_bandwidth",
];

/// Standard enhance targets from spec
pub const ENHANCE_TARGETS: &[&str] = &["clarity", "fullness", "presence", "width", "all"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_defaults() {
        let registry = NeuralModelRegistry::with_defaults();

        assert!(registry.has_model("style-transfer"));
        assert!(registry.has_model("denoise"));
        assert!(registry.has_model("restore"));
        assert!(registry.has_model("enhance"));
        assert!(registry.has_model("ace-step"));
    }

    #[test]
    fn test_get_model() {
        let registry = NeuralModelRegistry::with_defaults();

        let model = registry.get("style-transfer");
        assert!(model.is_ok());

        let model = registry.get("nonexistent");
        assert!(model.is_err());
    }

    #[test]
    fn test_find_capability() {
        let registry = NeuralModelRegistry::with_defaults();

        let model = registry.find_model_for_capability("noise_removal");
        assert!(model.is_some());
        assert_eq!(model.unwrap(), "denoise");
    }

    #[test]
    fn test_list_models() {
        let registry = NeuralModelRegistry::with_defaults();
        let models = registry.list_models();

        assert!(models.contains(&"style-transfer"));
        assert!(models.contains(&"denoise"));
    }
}
