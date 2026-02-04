//! Mock neural model implementations for testing
//!
//! These models don't do real AI processing but simulate the behavior
//! for pipeline testing. They use keyword-based processing to produce
//! verifiable audio changes.
//!
//! Implements Milestone 3.3 from the spec.

use super::model::{NeuralModel, NeuralModelInfo, NeuralModelParams, ParamSpec, ParamType, ProcessingResult};
use super::registry::{
    create_model_info, DENOISE_NOISE_TYPES, ENHANCE_TARGETS, RESTORE_MODES, STYLE_TRANSFER_PRESETS,
};
use crate::error::Result;
use std::path::Path;
use std::time::Instant;

/// Mock style transfer model
pub struct MockStyleTransfer {
    info: NeuralModelInfo,
}

impl MockStyleTransfer {
    pub fn new() -> Self {
        Self {
            info: create_model_info(
                "style-transfer",
                "Style Transfer",
                "1.0-mock",
                "Transfer timbral characteristics from reference or preset (MOCK)",
                vec!["timbre", "texture", "coloration", "vintage_simulation"],
                vec![
                    "Holistic sound transformation",
                    "vintage vibes",
                    "sounds like X",
                ],
                vec!["Takes several seconds", "Result may vary"],
                vec![
                    "High-frequency aliasing on 'vinyl' preset",
                    "Pumping artifacts when source already has heavy compression",
                ],
                2.0,
                "3-10 seconds",
                vec![
                    ParamSpec {
                        name: "style_preset".to_string(),
                        param_type: ParamType::Enum {
                            options: STYLE_TRANSFER_PRESETS
                                .iter()
                                .map(|s| s.to_string())
                                .collect(),
                        },
                        description: "Style preset to apply".to_string(),
                        default: Some(serde_json::json!("vintage_analog")),
                        required: false,
                    },
                    ParamSpec {
                        name: "intensity".to_string(),
                        param_type: ParamType::Float { min: 0.0, max: 1.0 },
                        description: "Intensity of the effect".to_string(),
                        default: Some(serde_json::json!(0.5)),
                        required: false,
                    },
                    ParamSpec {
                        name: "reference_audio".to_string(),
                        param_type: ParamType::String,
                        description: "Optional path to style reference audio".to_string(),
                        default: None,
                        required: false,
                    },
                ],
            ),
        }
    }
}

impl Default for MockStyleTransfer {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralModel for MockStyleTransfer {
    fn info(&self) -> &NeuralModelInfo {
        &self.info
    }

    fn process(
        &self,
        _input_path: &Path,
        output_path: &Path,
        params: &NeuralModelParams,
    ) -> Result<ProcessingResult> {
        let start = Instant::now();

        let preset = params
            .get_string("style_preset")
            .unwrap_or_else(|| "vintage_analog".to_string());
        let intensity = params.get_f32("intensity").unwrap_or(0.5);

        // Mock: just copy input to output (in real impl, would process)
        // For testing, we simulate the processing time
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Determine intentional artifacts based on preset
        let mut artifacts = Vec::new();
        if preset.contains("vinyl") {
            artifacts.push("high_frequency_noise".to_string());
            artifacts.push("subtle_crackle".to_string());
        }
        if preset.contains("tape") {
            artifacts.push("subtle_hiss".to_string());
            artifacts.push("saturation".to_string());
        }
        if preset.contains("lo_fi") {
            artifacts.push("bitcrushing".to_string());
            artifacts.push("noise".to_string());
        }

        let elapsed = start.elapsed().as_millis() as u64;

        Ok(ProcessingResult::success(
            output_path.to_string_lossy().to_string(),
            format!(
                "Applied {} style transfer at {:.0}% intensity (MOCK)",
                preset,
                intensity * 100.0
            ),
            elapsed,
        )
        .with_artifacts(artifacts))
    }
}

/// Mock denoise model
pub struct MockDenoise {
    info: NeuralModelInfo,
}

impl MockDenoise {
    pub fn new() -> Self {
        Self {
            info: create_model_info(
                "denoise",
                "AI Denoise",
                "3.0-mock",
                "AI-based noise reduction preserving signal (MOCK)",
                vec![
                    "noise_removal",
                    "hiss_removal",
                    "hum_removal",
                    "room_tone_reduction",
                ],
                vec!["Noise, hiss, hum, cleanup, clarity issues"],
                vec!["Musical noise at high strength", "Reverb tail truncation"],
                vec![
                    "Musical noise (twinkling) at strength >0.8",
                    "Sibilance dulling on speech at high strength",
                ],
                1.0,
                "2-5 seconds (real-time capable)",
                vec![
                    ParamSpec {
                        name: "strength".to_string(),
                        param_type: ParamType::Float { min: 0.0, max: 1.0 },
                        description: "Noise reduction strength".to_string(),
                        default: Some(serde_json::json!(0.5)),
                        required: false,
                    },
                    ParamSpec {
                        name: "preserve_transients".to_string(),
                        param_type: ParamType::Bool,
                        description: "Preserve attack transients".to_string(),
                        default: Some(serde_json::json!(true)),
                        required: false,
                    },
                    ParamSpec {
                        name: "noise_type".to_string(),
                        param_type: ParamType::Enum {
                            options: DENOISE_NOISE_TYPES.iter().map(|s| s.to_string()).collect(),
                        },
                        description: "Type of noise to target".to_string(),
                        default: Some(serde_json::json!("auto")),
                        required: false,
                    },
                ],
            ),
        }
    }
}

impl Default for MockDenoise {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralModel for MockDenoise {
    fn info(&self) -> &NeuralModelInfo {
        &self.info
    }

    fn process(
        &self,
        _input_path: &Path,
        output_path: &Path,
        params: &NeuralModelParams,
    ) -> Result<ProcessingResult> {
        let start = Instant::now();

        let strength = params.get_f32("strength").unwrap_or(0.5);
        let noise_type = params
            .get_string("noise_type")
            .unwrap_or_else(|| "auto".to_string());

        std::thread::sleep(std::time::Duration::from_millis(50));

        let mut warnings = Vec::new();
        if strength > 0.8 {
            warnings.push("High strength may cause musical noise artifacts".to_string());
        }

        let elapsed = start.elapsed().as_millis() as u64;

        Ok(ProcessingResult::success(
            output_path.to_string_lossy().to_string(),
            format!(
                "Reduced {} noise at {:.0}% strength (MOCK)",
                noise_type,
                strength * 100.0
            ),
            elapsed,
        )
        .with_warnings(warnings))
    }
}

/// Mock restore model
pub struct MockRestore {
    info: NeuralModelInfo,
}

impl MockRestore {
    pub fn new() -> Self {
        Self {
            info: create_model_info(
                "restore",
                "Audio Restore",
                "1.0-mock",
                "Audio restoration for damaged/degraded recordings (MOCK)",
                vec![
                    "declip",
                    "dehum",
                    "declick",
                    "decrackle",
                    "bandwidth_extension",
                ],
                vec!["Clipping, distortion, pops, clicks, old recordings, low quality"],
                vec![
                    "May smooth intentional distortion",
                    "60Hz removal affects bass guitar",
                ],
                vec![
                    "Over-smoothing of intentional distortion",
                    "Ghost transients in declip mode",
                ],
                2.0,
                "3-8 seconds",
                vec![
                    ParamSpec {
                        name: "mode".to_string(),
                        param_type: ParamType::Enum {
                            options: RESTORE_MODES.iter().map(|s| s.to_string()).collect(),
                        },
                        description: "Restoration mode".to_string(),
                        default: Some(serde_json::json!("auto")),
                        required: false,
                    },
                    ParamSpec {
                        name: "aggressiveness".to_string(),
                        param_type: ParamType::Float { min: 0.0, max: 1.0 },
                        description: "How aggressive the restoration should be".to_string(),
                        default: Some(serde_json::json!(0.5)),
                        required: false,
                    },
                ],
            ),
        }
    }
}

impl Default for MockRestore {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralModel for MockRestore {
    fn info(&self) -> &NeuralModelInfo {
        &self.info
    }

    fn process(
        &self,
        _input_path: &Path,
        output_path: &Path,
        params: &NeuralModelParams,
    ) -> Result<ProcessingResult> {
        let start = Instant::now();

        let mode = params
            .get_string("mode")
            .unwrap_or_else(|| "auto".to_string());
        let aggressiveness = params.get_f32("aggressiveness").unwrap_or(0.5);

        std::thread::sleep(std::time::Duration::from_millis(75));

        let elapsed = start.elapsed().as_millis() as u64;

        Ok(ProcessingResult::success(
            output_path.to_string_lossy().to_string(),
            format!(
                "Applied {} restoration at {:.0}% aggressiveness (MOCK)",
                mode,
                aggressiveness * 100.0
            ),
            elapsed,
        ))
    }
}

/// Mock enhance model
pub struct MockEnhance {
    info: NeuralModelInfo,
}

impl MockEnhance {
    pub fn new() -> Self {
        Self {
            info: create_model_info(
                "enhance",
                "AI Enhance",
                "1.0-mock",
                "AI upsampling, clarity enhancement, presence boost (MOCK)",
                vec![
                    "clarity",
                    "fullness",
                    "presence",
                    "stereo_width",
                    "upsample",
                ],
                vec!["Improve overall quality, add presence, enhance clarity"],
                vec!["Start at 0.3, increase gradually"],
                vec![
                    "Artificial 'sparkle' at amounts >0.7",
                    "Phase issues in stereo width enhancement",
                ],
                3.0,
                "5-15 seconds",
                vec![
                    ParamSpec {
                        name: "target".to_string(),
                        param_type: ParamType::Enum {
                            options: ENHANCE_TARGETS.iter().map(|s| s.to_string()).collect(),
                        },
                        description: "Enhancement target".to_string(),
                        default: Some(serde_json::json!("clarity")),
                        required: false,
                    },
                    ParamSpec {
                        name: "amount".to_string(),
                        param_type: ParamType::Float { min: 0.0, max: 1.0 },
                        description: "Enhancement amount (start at 0.3)".to_string(),
                        default: Some(serde_json::json!(0.3)),
                        required: false,
                    },
                ],
            ),
        }
    }
}

impl Default for MockEnhance {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralModel for MockEnhance {
    fn info(&self) -> &NeuralModelInfo {
        &self.info
    }

    fn process(
        &self,
        _input_path: &Path,
        output_path: &Path,
        params: &NeuralModelParams,
    ) -> Result<ProcessingResult> {
        let start = Instant::now();

        let target = params
            .get_string("target")
            .unwrap_or_else(|| "clarity".to_string());
        let amount = params.get_f32("amount").unwrap_or(0.3);

        std::thread::sleep(std::time::Duration::from_millis(80));

        let mut warnings = Vec::new();
        if amount > 0.7 {
            warnings.push("High enhancement amount may introduce artifacts".to_string());
        }

        let elapsed = start.elapsed().as_millis() as u64;

        Ok(ProcessingResult::success(
            output_path.to_string_lossy().to_string(),
            format!(
                "Enhanced {} at {:.0}% (MOCK)",
                target,
                amount * 100.0
            ),
            elapsed,
        )
        .with_warnings(warnings))
    }
}

/// Mock ACE-Step model (the big transformer)
pub struct MockAceStep {
    info: NeuralModelInfo,
}

impl MockAceStep {
    pub fn new() -> Self {
        Self {
            info: create_model_info(
                "ace-step",
                "ACE-Step 1.5",
                "1.5-mock",
                "Full music transformation via Hybrid Reasoning-Diffusion (MOCK)",
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
                    "genre change",
                    "cover generation",
                    "reimagine as X",
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
                4.0,
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
                ],
            ),
        }
    }
}

impl Default for MockAceStep {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralModel for MockAceStep {
    fn info(&self) -> &NeuralModelInfo {
        &self.info
    }

    fn process(
        &self,
        _input_path: &Path,
        output_path: &Path,
        params: &NeuralModelParams,
    ) -> Result<ProcessingResult> {
        let start = Instant::now();

        let mode = params
            .get_string("mode")
            .unwrap_or_else(|| "transform".to_string());
        let prompt = params
            .get_string("prompt")
            .unwrap_or_else(|| "transform audio".to_string());
        let intensity = params.get_f32("intensity").unwrap_or(0.7);

        std::thread::sleep(std::time::Duration::from_millis(150));

        let mut artifacts = Vec::new();
        if mode == "cover" {
            artifacts.push("different_timbre".to_string());
        }
        if prompt.to_lowercase().contains("vintage") {
            artifacts.push("intentional_coloration".to_string());
        }

        let elapsed = start.elapsed().as_millis() as u64;

        Ok(ProcessingResult::success(
            output_path.to_string_lossy().to_string(),
            format!(
                "ACE-Step {} mode: '{}' at {:.0}% intensity (MOCK)",
                mode,
                prompt,
                intensity * 100.0
            ),
            elapsed,
        )
        .with_artifacts(artifacts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_style_transfer() {
        let model = MockStyleTransfer::new();
        let params = NeuralModelParams::new()
            .with_param("style_preset", "vinyl_crackle")
            .with_param("intensity", 0.7);

        let result = model
            .process(
                Path::new("/tmp/in.wav"),
                Path::new("/tmp/out.wav"),
                &params,
            )
            .unwrap();

        assert!(result.success);
        assert!(result.intentional_artifacts.contains(&"subtle_crackle".to_string()));
    }

    #[test]
    fn test_mock_denoise() {
        let model = MockDenoise::new();
        let params = NeuralModelParams::new().with_param("strength", 0.9);

        let result = model
            .process(
                Path::new("/tmp/in.wav"),
                Path::new("/tmp/out.wav"),
                &params,
            )
            .unwrap();

        assert!(result.success);
        assert!(!result.warnings.is_empty()); // Should warn about high strength
    }

    #[test]
    fn test_mock_restore() {
        let model = MockRestore::new();
        let params = NeuralModelParams::new().with_param("mode", "declip");

        let result = model
            .process(
                Path::new("/tmp/in.wav"),
                Path::new("/tmp/out.wav"),
                &params,
            )
            .unwrap();

        assert!(result.success);
        assert!(result.description.contains("declip"));
    }

    #[test]
    fn test_mock_enhance() {
        let model = MockEnhance::new();
        let params = NeuralModelParams::new()
            .with_param("target", "presence")
            .with_param("amount", 0.4);

        let result = model
            .process(
                Path::new("/tmp/in.wav"),
                Path::new("/tmp/out.wav"),
                &params,
            )
            .unwrap();

        assert!(result.success);
        assert!(result.description.contains("presence"));
    }

    #[test]
    fn test_mock_ace_step() {
        let model = MockAceStep::new();
        let params = NeuralModelParams::new()
            .with_param("mode", "cover")
            .with_param("prompt", "jazz version");

        let result = model
            .process(
                Path::new("/tmp/in.wav"),
                Path::new("/tmp/out.wav"),
                &params,
            )
            .unwrap();

        assert!(result.success);
        assert!(result.intentional_artifacts.contains(&"different_timbre".to_string()));
    }
}
