//! ACE-Step 1.5 Neural Model Integration
//!
//! This module provides the Rust interface to the ACE-Step 1.5 model
//! via the Python AI bridge.
//!
//! ACE-Step 1.5 is a music transformation model capable of:
//! - Text-guided audio transformation
//! - Cover generation
//! - Style transfer
//! - Track extraction
//! - Audio completion

use super::model::{NeuralModel, NeuralModelInfo, NeuralModelParams, ParamSpec, ParamType, ProcessingResult};
use super::registry::create_model_info;
use crate::error::{NuevaError, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Instant;

/// ACE-Step processing modes
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AceStepMode {
    Transform,
    Repaint,
    Cover,
    Extract,
    Layer,
    Complete,
}

impl Default for AceStepMode {
    fn default() -> Self {
        Self::Transform
    }
}

impl std::fmt::Display for AceStepMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transform => write!(f, "transform"),
            Self::Repaint => write!(f, "repaint"),
            Self::Cover => write!(f, "cover"),
            Self::Extract => write!(f, "extract"),
            Self::Layer => write!(f, "layer"),
            Self::Complete => write!(f, "complete"),
        }
    }
}

/// Request to the Python AI bridge
#[derive(Debug, Serialize)]
struct BridgeRequest {
    action: String,
    request_id: Option<String>,
    model: String,
    input_path: String,
    output_path: String,
    prompt: Option<String>,
    model_params: serde_json::Value,
}

/// Response from the Python AI bridge
#[derive(Debug, Deserialize)]
struct BridgeResponse {
    success: bool,
    request_id: Option<String>,
    tool_used: Option<String>,
    reasoning: Option<String>,
    message: Option<String>,
    neural_changes: Option<NeuralChanges>,
    error: Option<String>,
    error_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NeuralChanges {
    model: Option<String>,
    output_path: Option<String>,
    processing_time_ms: Option<u64>,
    intentional_artifacts: Option<Vec<String>>,
}

/// ACE-Step 1.5 model processor
pub struct AceStep {
    info: NeuralModelInfo,
    bridge_process: Mutex<Option<Child>>,
    python_path: String,
    bridge_module: String,
}

impl AceStep {
    /// Create a new ACE-Step processor
    pub fn new() -> Self {
        let python_path = std::env::var("NUEVA_PYTHON_PATH")
            .unwrap_or_else(|_| "python".to_string());

        let bridge_module = std::env::var("NUEVA_BRIDGE_MODULE")
            .unwrap_or_else(|_| "nueva.bridge".to_string());

        Self {
            info: create_model_info(
                "ace-step",
                "ACE-Step 1.5",
                "1.5",
                "Full music transformation via Hybrid Reasoning-Diffusion",
                vec![
                    "text_to_music",
                    "cover",
                    "repaint",
                    "style_change",
                    "track_extraction",
                    "layering",
                    "completion",
                ],
                vec![
                    "Dramatic transformation",
                    "Genre change",
                    "Cover generation",
                    "Reimagine as X",
                ],
                vec![
                    "Takes several seconds",
                    "Non-deterministic",
                    "Requires GPU",
                ],
                vec![
                    "Vocal intelligibility loss on complex lyrics",
                    "Tempo drift on pieces >5 minutes",
                    "Transient softening on aggressive percussion",
                ],
                4.0,  // VRAM requirement
                "1-30 seconds depending on GPU",
                vec![
                    ParamSpec {
                        name: "mode".to_string(),
                        param_type: ParamType::Enum {
                            options: vec![
                                "transform".to_string(),
                                "repaint".to_string(),
                                "cover".to_string(),
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
                        name: "preserve_melody".to_string(),
                        param_type: ParamType::Bool,
                        description: "Whether to preserve the original melody".to_string(),
                        default: Some(serde_json::json!(true)),
                        required: false,
                    },
                    ParamSpec {
                        name: "intensity".to_string(),
                        param_type: ParamType::Float { min: 0.0, max: 1.0 },
                        description: "Transformation intensity".to_string(),
                        default: Some(serde_json::json!(0.7)),
                        required: false,
                    },
                    ParamSpec {
                        name: "inference_steps".to_string(),
                        param_type: ParamType::Int { min: 4, max: 50 },
                        description: "Number of diffusion steps".to_string(),
                        default: Some(serde_json::json!(8)),
                        required: false,
                    },
                    ParamSpec {
                        name: "guidance_scale".to_string(),
                        param_type: ParamType::Float { min: 1.0, max: 10.0 },
                        description: "How closely to follow the prompt".to_string(),
                        default: Some(serde_json::json!(3.0)),
                        required: false,
                    },
                ],
            ),
            bridge_process: Mutex::new(None),
            python_path,
            bridge_module,
        }
    }

    /// Start the Python bridge process if not already running
    fn ensure_bridge(&self) -> Result<()> {
        let mut guard = self.bridge_process.lock().map_err(|_| NuevaError::ProcessingError {
            reason: "Failed to acquire bridge lock".to_string(),
        })?;

        if guard.is_none() {
            let child = Command::new(&self.python_path)
                .args(["-m", &self.bridge_module])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .map_err(|e| NuevaError::AiProcessingError {
                    reason: format!("Failed to start Python bridge: {}", e),
                })?;

            *guard = Some(child);
        }

        Ok(())
    }

    /// Send a request to the Python bridge
    fn send_request(&self, request: &BridgeRequest) -> Result<BridgeResponse> {
        self.ensure_bridge()?;

        let mut guard = self.bridge_process.lock().map_err(|_| NuevaError::ProcessingError {
            reason: "Failed to acquire bridge lock".to_string(),
        })?;

        let child = guard.as_mut().ok_or_else(|| NuevaError::ProcessingError {
            reason: "Bridge process not running".to_string(),
        })?;

        // Write request to stdin
        let stdin = child.stdin.as_mut().ok_or_else(|| NuevaError::ProcessingError {
            reason: "Bridge stdin not available".to_string(),
        })?;

        let request_json = serde_json::to_string(request).map_err(|e| NuevaError::ProcessingError {
            reason: format!("Failed to serialize request: {}", e),
        })?;

        writeln!(stdin, "{}", request_json).map_err(|e| NuevaError::ProcessingError {
            reason: format!("Failed to write to bridge: {}", e),
        })?;

        stdin.flush().map_err(|e| NuevaError::ProcessingError {
            reason: format!("Failed to flush bridge stdin: {}", e),
        })?;

        // Read response from stdout
        let stdout = child.stdout.as_mut().ok_or_else(|| NuevaError::ProcessingError {
            reason: "Bridge stdout not available".to_string(),
        })?;

        let mut reader = BufReader::new(stdout);
        let mut response_line = String::new();
        reader.read_line(&mut response_line).map_err(|e| NuevaError::ProcessingError {
            reason: format!("Failed to read from bridge: {}", e),
        })?;

        let response: BridgeResponse = serde_json::from_str(&response_line).map_err(|e| {
            NuevaError::ProcessingError {
                reason: format!("Failed to parse bridge response: {}", e),
            }
        })?;

        Ok(response)
    }

    /// Check if ACE-Step is available
    pub fn check_availability(&self) -> Result<bool> {
        self.ensure_bridge()?;

        let request = BridgeRequest {
            action: "get_model_info".to_string(),
            request_id: Some(uuid::Uuid::new_v4().to_string()),
            model: "ace-step".to_string(),
            input_path: String::new(),
            output_path: String::new(),
            prompt: None,
            model_params: serde_json::json!({}),
        };

        match self.send_request(&request) {
            Ok(response) => Ok(response.success),
            Err(_) => Ok(false),
        }
    }
}

impl Default for AceStep {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralModel for AceStep {
    fn info(&self) -> &NeuralModelInfo {
        &self.info
    }

    fn process(
        &self,
        input_path: &Path,
        output_path: &Path,
        params: &NeuralModelParams,
    ) -> Result<ProcessingResult> {
        let start = Instant::now();

        let prompt = params.get_string("prompt").unwrap_or_else(|| "transform audio".to_string());

        let request = BridgeRequest {
            action: "process".to_string(),
            request_id: Some(uuid::Uuid::new_v4().to_string()),
            model: "ace-step".to_string(),
            input_path: input_path.to_string_lossy().to_string(),
            output_path: output_path.to_string_lossy().to_string(),
            prompt: Some(prompt.clone()),
            model_params: serde_json::to_value(params).unwrap_or_default(),
        };

        let response = self.send_request(&request)?;

        if !response.success {
            return Err(NuevaError::AiProcessingError {
                reason: response.error.unwrap_or_else(|| "Unknown error".to_string()),
            });
        }

        let elapsed = start.elapsed().as_millis() as u64;
        let processing_time = response
            .neural_changes
            .as_ref()
            .and_then(|nc| nc.processing_time_ms)
            .unwrap_or(elapsed);

        let artifacts = response
            .neural_changes
            .as_ref()
            .and_then(|nc| nc.intentional_artifacts.clone())
            .unwrap_or_default();

        let description = response.message.unwrap_or_else(|| {
            format!("ACE-Step processed: '{}'", prompt)
        });

        Ok(ProcessingResult::success(
            output_path.to_string_lossy().to_string(),
            description,
            processing_time,
        )
        .with_artifacts(artifacts))
    }

    fn is_available(&self) -> bool {
        self.check_availability().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ace_step_mode_display() {
        assert_eq!(AceStepMode::Transform.to_string(), "transform");
        assert_eq!(AceStepMode::Cover.to_string(), "cover");
    }

    #[test]
    fn test_ace_step_info() {
        let model = AceStep::new();
        let info = model.info();

        assert_eq!(info.id, "ace-step");
        assert_eq!(info.version, "1.5");
        assert!(info.capabilities.contains(&"cover".to_string()));
    }

    #[test]
    fn test_ace_step_params() {
        let params = NeuralModelParams::new()
            .with_param("mode", "cover")
            .with_param("prompt", "jazz version")
            .with_param("intensity", 0.8);

        assert_eq!(params.get_string("mode"), Some("cover".to_string()));
        assert_eq!(params.get_f32("intensity"), Some(0.8));
    }
}
