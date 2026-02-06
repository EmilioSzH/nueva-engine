//! ACE-Step neural model implementation
//!
//! Real implementation that communicates with the Nueva AI Bridge,
//! which in turn talks to ACE-Step API.
//!
//! Implements the NeuralModel trait for ACE-Step 1.5.

use crate::error::{NuevaError, Result};
use crate::neural::gpu::{can_run_ace_step, GpuInfo, QuantizationLevel};
use crate::neural::model::{
    NeuralModel, NeuralModelInfo, NeuralModelParams, ParamSpec, ParamType, ProcessingResult,
};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::time::Instant;

/// ACE-Step processing modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AceStepMode {
    /// Generate music from text prompt (text2music)
    Transform,
    /// Create a cover version - change style, preserve structure
    Cover,
    /// Repaint/modify specific audio regions
    Repaint,
    /// Source separation / extraction
    Extract,
    /// Add/remove instrument layers (lego mode)
    Layer,
    /// Add accompaniment / complete
    Complete,
}

impl AceStepMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Transform => "transform",
            Self::Cover => "cover",
            Self::Repaint => "repaint",
            Self::Extract => "extract",
            Self::Layer => "layer",
            Self::Complete => "complete",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "transform" | "text2music" => Some(Self::Transform),
            "cover" => Some(Self::Cover),
            "repaint" => Some(Self::Repaint),
            "extract" | "separation" => Some(Self::Extract),
            "layer" | "lego" => Some(Self::Layer),
            "complete" | "accompaniment" => Some(Self::Complete),
            _ => None,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Transform => "Generate music from text description",
            Self::Cover => "Create a cover version with different style",
            Self::Repaint => "Modify specific regions of audio",
            Self::Extract => "Separate audio sources (vocals, drums, etc.)",
            Self::Layer => "Add or remove instrument layers",
            Self::Complete => "Add accompaniment to existing audio",
        }
    }
}

/// Request to send to Nueva AI Bridge
#[derive(Debug, Serialize)]
struct BridgeRequest {
    mode: String,
    input_path: String,
    output_path: String,
    prompt: Option<String>,
    intensity: f32,
    preserve_melody: bool,
    preserve_tempo: bool,
    preserve_key: bool,
    extract_target: Option<String>,
    add_layers: Option<Vec<String>>,
    remove_layers: Option<Vec<String>>,
    duration_seconds: Option<f32>,
    seed: Option<i64>,
    quantization: Option<String>,
    extra_params: HashMap<String, serde_json::Value>,
}

/// Response from Nueva AI Bridge
#[derive(Debug, Deserialize)]
struct BridgeResponse {
    success: bool,
    output_path: Option<String>,
    processing_time_ms: u64,
    description: String,
    intentional_artifacts: Vec<String>,
    warnings: Vec<String>,
    error_code: Option<String>,
    error_message: Option<String>,
    #[serde(default)]
    metadata: HashMap<String, serde_json::Value>,
}

/// Real ACE-Step model implementation
pub struct AceStepModel {
    info: NeuralModelInfo,
    bridge_url: String,
    timeout_ms: u64,
    available: bool,
    gpu_info: Option<GpuInfo>,
    quantization: QuantizationLevel,
}

impl AceStepModel {
    /// Create a new ACE-Step model instance
    pub fn new() -> Self {
        let bridge_url =
            env::var("NUEVA_ACESTEP_API_URL").unwrap_or_else(|_| "http://localhost:8001".into());
        let timeout_ms = env::var("NUEVA_ACESTEP_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300_000); // 5 minutes default

        let gpu_info = GpuInfo::detect();
        let (available, quantization, _reason) = can_run_ace_step();

        Self {
            info: Self::create_model_info(),
            bridge_url,
            timeout_ms,
            available,
            gpu_info,
            quantization,
        }
    }

    /// Create a new instance with custom configuration
    pub fn with_config(bridge_url: String, timeout_ms: u64) -> Self {
        let gpu_info = GpuInfo::detect();
        let (available, quantization, _reason) = can_run_ace_step();

        Self {
            info: Self::create_model_info(),
            bridge_url,
            timeout_ms,
            available,
            gpu_info,
            quantization,
        }
    }

    fn create_model_info() -> NeuralModelInfo {
        NeuralModelInfo {
            id: "ace-step".to_string(),
            name: "ACE-Step 1.5".to_string(),
            version: "1.5".to_string(),
            description: "Full music transformation via Hybrid Reasoning-Diffusion".to_string(),
            capabilities: vec![
                "text_to_music".to_string(),
                "cover".to_string(),
                "repaint".to_string(),
                "style_change".to_string(),
                "track_extraction".to_string(),
                "layering".to_string(),
                "completion".to_string(),
            ],
            use_when: vec![
                "Dramatic transformation".to_string(),
                "genre change".to_string(),
                "cover generation".to_string(),
                "reimagine as X".to_string(),
                "separate tracks".to_string(),
                "add instruments".to_string(),
            ],
            limitations: vec![
                "Requires GPU for best performance".to_string(),
                "Non-deterministic output".to_string(),
                "Processing takes several seconds".to_string(),
            ],
            known_artifacts: vec![
                "Vocal intelligibility loss on complex lyrics".to_string(),
                "Tempo drift on pieces >5 minutes".to_string(),
                "Transient softening on aggressive percussion".to_string(),
            ],
            vram_requirement_gb: 4.0,
            inference_time: "1-30 seconds depending on GPU".to_string(),
            supported_params: vec![
                ParamSpec {
                    name: "mode".to_string(),
                    param_type: ParamType::Enum {
                        options: vec![
                            "transform".to_string(),
                            "cover".to_string(),
                            "repaint".to_string(),
                            "extract".to_string(),
                            "layer".to_string(),
                            "complete".to_string(),
                        ],
                    },
                    description: "Processing mode".to_string(),
                    default: Some(serde_json::json!("transform")),
                    required: false,
                },
                ParamSpec {
                    name: "prompt".to_string(),
                    param_type: ParamType::String,
                    description: "Text description of desired output".to_string(),
                    default: None,
                    required: true,
                },
                ParamSpec {
                    name: "intensity".to_string(),
                    param_type: ParamType::Float { min: 0.0, max: 1.0 },
                    description: "Transformation intensity (0-1)".to_string(),
                    default: Some(serde_json::json!(0.7)),
                    required: false,
                },
                ParamSpec {
                    name: "preserve_melody".to_string(),
                    param_type: ParamType::Bool,
                    description: "Preserve original melody (cover mode)".to_string(),
                    default: Some(serde_json::json!(true)),
                    required: false,
                },
                ParamSpec {
                    name: "preserve_tempo".to_string(),
                    param_type: ParamType::Bool,
                    description: "Preserve original tempo".to_string(),
                    default: Some(serde_json::json!(true)),
                    required: false,
                },
                ParamSpec {
                    name: "preserve_key".to_string(),
                    param_type: ParamType::Bool,
                    description: "Preserve original key".to_string(),
                    default: Some(serde_json::json!(true)),
                    required: false,
                },
                ParamSpec {
                    name: "extract_target".to_string(),
                    param_type: ParamType::Enum {
                        options: vec![
                            "vocals".to_string(),
                            "drums".to_string(),
                            "bass".to_string(),
                            "other".to_string(),
                            "all".to_string(),
                        ],
                    },
                    description: "What to extract (extract mode)".to_string(),
                    default: Some(serde_json::json!("vocals")),
                    required: false,
                },
                ParamSpec {
                    name: "seed".to_string(),
                    param_type: ParamType::Int {
                        min: -1,
                        max: i32::MAX,
                    },
                    description: "Random seed for reproducibility (-1 for random)".to_string(),
                    default: Some(serde_json::json!(-1)),
                    required: false,
                },
            ],
        }
    }

    /// Get GPU information
    pub fn gpu_info(&self) -> Option<&GpuInfo> {
        self.gpu_info.as_ref()
    }

    /// Get recommended quantization level
    pub fn quantization(&self) -> QuantizationLevel {
        self.quantization
    }

    /// Check if the bridge is reachable
    #[cfg(feature = "acestep")]
    fn check_bridge_health(&self) -> Result<bool> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| NuevaError::BridgeConnectionError {
                message: e.to_string(),
            })?;

        let url = format!("{}/health", self.bridge_url);
        match client.get(&url).send() {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    #[cfg(not(feature = "acestep"))]
    fn check_bridge_health(&self) -> Result<bool> {
        Ok(false)
    }

    /// Send request to the bridge
    #[cfg(feature = "acestep")]
    fn send_request(&self, request: &BridgeRequest) -> Result<BridgeResponse> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_millis(self.timeout_ms))
            .build()
            .map_err(|e| NuevaError::BridgeConnectionError {
                message: e.to_string(),
            })?;

        let url = format!("{}/process", self.bridge_url);

        let response = client
            .post(&url)
            .json(request)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    NuevaError::AceStepTimeout {
                        timeout_ms: self.timeout_ms,
                    }
                } else if e.is_connect() {
                    NuevaError::AceStepUnavailable {
                        reason: format!("Cannot connect to bridge at {}: {}", self.bridge_url, e),
                    }
                } else {
                    NuevaError::BridgeConnectionError {
                        message: e.to_string(),
                    }
                }
            })?;

        if !response.status().is_success() {
            return Err(NuevaError::AceStepUnavailable {
                reason: format!("Bridge returned error: {}", response.status()),
            });
        }

        response.json::<BridgeResponse>().map_err(|e| {
            NuevaError::BridgeConnectionError {
                message: format!("Invalid response from bridge: {}", e),
            }
        })
    }

    #[cfg(not(feature = "acestep"))]
    fn send_request(&self, _request: &BridgeRequest) -> Result<BridgeResponse> {
        Err(NuevaError::AceStepUnavailable {
            reason: "ACE-Step support not compiled. Build with --features acestep".to_string(),
        })
    }
}

impl Default for AceStepModel {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralModel for AceStepModel {
    fn info(&self) -> &NeuralModelInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        self.available && self.check_bridge_health().unwrap_or(false)
    }

    fn validate_params(&self, params: &NeuralModelParams) -> Result<()> {
        // Check that prompt is provided
        if params.get_string("prompt").is_none() {
            return Err(NuevaError::InvalidParameter {
                param: "prompt".to_string(),
                value: "<missing>".to_string(),
                expected: "text description of desired output".to_string(),
            });
        }

        // Validate mode if provided
        if let Some(mode) = params.get_string("mode") {
            if AceStepMode::from_str(&mode).is_none() {
                return Err(NuevaError::InvalidParameter {
                    param: "mode".to_string(),
                    value: mode,
                    expected: "transform, cover, repaint, extract, layer, or complete".to_string(),
                });
            }
        }

        // Validate intensity range
        if let Some(intensity) = params.get_f32("intensity") {
            if !(0.0..=1.0).contains(&intensity) {
                return Err(NuevaError::InvalidParameter {
                    param: "intensity".to_string(),
                    value: intensity.to_string(),
                    expected: "value between 0.0 and 1.0".to_string(),
                });
            }
        }

        Ok(())
    }

    fn process(
        &self,
        input_path: &Path,
        output_path: &Path,
        params: &NeuralModelParams,
    ) -> Result<ProcessingResult> {
        let start = Instant::now();

        // Validate params
        self.validate_params(params)?;

        // Check VRAM if GPU info available
        if let Some(gpu) = &self.gpu_info {
            if !gpu.suitable_for_ace_step {
                tracing::warn!(
                    "GPU has insufficient VRAM ({:.1}GB), using CPU fallback",
                    gpu.vram_available_gb
                );
            }
        }

        // Extract parameters
        let mode = params
            .get_string("mode")
            .unwrap_or_else(|| "transform".to_string());
        let prompt = params.get_string("prompt");
        let intensity = params.get_f32("intensity").unwrap_or(0.7);
        let preserve_melody = params.get_bool("preserve_melody").unwrap_or(true);
        let preserve_tempo = params.get_bool("preserve_tempo").unwrap_or(true);
        let preserve_key = params.get_bool("preserve_key").unwrap_or(true);
        let extract_target = params.get_string("extract_target");
        let seed = params.get::<i64>("seed");

        // Build request
        let request = BridgeRequest {
            mode,
            input_path: input_path.to_string_lossy().to_string(),
            output_path: output_path.to_string_lossy().to_string(),
            prompt,
            intensity,
            preserve_melody,
            preserve_tempo,
            preserve_key,
            extract_target,
            add_layers: params.get("add_layers"),
            remove_layers: params.get("remove_layers"),
            duration_seconds: params.get_f32("duration"),
            seed,
            quantization: Some(match self.quantization {
                QuantizationLevel::FP32 => "fp32".to_string(),
                QuantizationLevel::FP16 => "fp16".to_string(),
                QuantizationLevel::INT8 => "int8".to_string(),
                QuantizationLevel::CPU => "cpu".to_string(),
            }),
            extra_params: HashMap::new(),
        };

        // Send to bridge
        let response = self.send_request(&request)?;

        let elapsed = start.elapsed().as_millis() as u64;

        if response.success {
            Ok(ProcessingResult {
                success: true,
                output_path: response.output_path,
                processing_time_ms: response.processing_time_ms.max(elapsed),
                description: response.description,
                intentional_artifacts: response.intentional_artifacts,
                warnings: response.warnings,
                metadata: response.metadata,
            })
        } else {
            Err(NuevaError::AiProcessingError {
                reason: response
                    .error_message
                    .unwrap_or_else(|| "Unknown ACE-Step error".to_string()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ace_step_mode_conversion() {
        assert_eq!(AceStepMode::from_str("cover"), Some(AceStepMode::Cover));
        assert_eq!(
            AceStepMode::from_str("extract"),
            Some(AceStepMode::Extract)
        );
        assert_eq!(AceStepMode::from_str("lego"), Some(AceStepMode::Layer));
        assert_eq!(
            AceStepMode::from_str("text2music"),
            Some(AceStepMode::Transform)
        );
        assert_eq!(AceStepMode::from_str("invalid"), None);
    }

    #[test]
    fn test_model_info() {
        let model = AceStepModel::new();
        let info = model.info();

        assert_eq!(info.id, "ace-step");
        assert_eq!(info.version, "1.5");
        assert!(info.vram_requirement_gb >= 4.0);
    }

    #[test]
    fn test_validate_params_missing_prompt() {
        let model = AceStepModel::new();
        let params = NeuralModelParams::new().with_param("mode", "cover");

        let result = model.validate_params(&params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_params_invalid_mode() {
        let model = AceStepModel::new();
        let params = NeuralModelParams::new()
            .with_param("prompt", "make it jazzy")
            .with_param("mode", "invalid_mode");

        let result = model.validate_params(&params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_params_valid() {
        let model = AceStepModel::new();
        let params = NeuralModelParams::new()
            .with_param("prompt", "convert to jazz style")
            .with_param("mode", "cover")
            .with_param("intensity", 0.8f32);

        let result = model.validate_params(&params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_params_invalid_intensity() {
        let model = AceStepModel::new();
        let params = NeuralModelParams::new()
            .with_param("prompt", "test")
            .with_param("intensity", 1.5f32);

        let result = model.validate_params(&params);
        assert!(result.is_err());
    }
}
