//! Neural context tracking for intentional artifacts
//!
//! Tracks what neural processing did so the Agent makes informed DSP decisions.
//! Prevents: Gate removing vinyl crackle, EQ "fixing" intentional lo-fi, etc.
//!
//! Implements §5.4 from the spec.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tracks neural processing context and intentional artifacts
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NeuralContextTracker {
    /// Last neural operation performed
    pub last_neural_operation: Option<NeuralOperation>,

    /// Intentional artifacts from neural processing
    pub intentional_artifacts: Vec<IntentionalArtifact>,

    /// History of all neural operations
    pub operation_history: Vec<NeuralOperation>,
}

/// Record of a neural operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuralOperation {
    /// Model that was used
    pub model: String,

    /// Parameters used
    pub params: HashMap<String, serde_json::Value>,

    /// When it was performed
    pub timestamp: DateTime<Utc>,

    /// What the operation did (human readable)
    pub description: String,
}

/// An intentional artifact from neural processing
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentionalArtifact {
    /// High frequency noise (e.g., vinyl crackle)
    HighFrequencyNoise,
    /// Frequency rolloff (e.g., vintage sound)
    FrequencyRolloff,
    /// Subtle hiss (e.g., tape)
    SubtleHiss,
    /// Saturation/distortion
    Saturation,
    /// Bitcrushing artifacts
    Bitcrushing,
    /// Sample rate artifacts
    SampleRateArtifacts,
    /// Bandwidth limitation
    BandwidthLimitation,
    /// Different timbre (e.g., cover version)
    DifferentTimbre,
    /// Intentional coloration
    IntentionalColoration,
    /// General noise
    Noise,
    /// Subtle crackle
    SubtleCrackle,
    /// Distortion
    Distortion,

    // ACE-Step specific artifacts
    /// Cover mode timbre change - completely new instrumental character
    CoverTimbre,
    /// Genre transformation - intentional genre change
    GenreTransformation,
    /// Tempo change from ACE-Step processing
    TempoChange,
    /// Key change from ACE-Step processing
    KeyChange,
    /// Vocal extraction artifacts
    VocalExtractionArtifacts,
    /// Instrument layer artifacts
    LayerArtifacts,
}

impl IntentionalArtifact {
    /// Get DSP warning for this artifact
    pub fn get_warning(&self) -> &'static str {
        match self {
            Self::HighFrequencyNoise | Self::SubtleCrackle => {
                "DO NOT add a noise gate - vinyl crackle is intentional"
            }
            Self::SubtleHiss => {
                "DO NOT use high shelf cut to remove hiss - tape hiss is intentional"
            }
            Self::DifferentTimbre => {
                "DO NOT EQ to 'correct' timbre - it's a cover with new character"
            }
            Self::Saturation | Self::Distortion => {
                "DO NOT use restoration/declip - saturation is intentional"
            }
            Self::FrequencyRolloff => {
                "DO NOT boost highs to 'fix' rolloff - vintage sound is intentional"
            }
            Self::IntentionalColoration => {
                "DO NOT try to 'neutralize' the sound - coloration is intentional"
            }
            Self::Bitcrushing | Self::SampleRateArtifacts => {
                "DO NOT try to 'clean up' the sound - lo-fi artifacts are intentional"
            }
            Self::BandwidthLimitation => {
                "DO NOT use bandwidth extension - limited bandwidth is intentional"
            }
            Self::Noise => "DO NOT use noise reduction - some noise is intentional",
            // ACE-Step specific warnings
            Self::CoverTimbre => {
                "DO NOT EQ to match original - cover has intentionally different character"
            }
            Self::GenreTransformation => {
                "DO NOT try to restore original genre characteristics - transformation is intentional"
            }
            Self::TempoChange => {
                "DO NOT time-stretch to match original tempo - tempo change is intentional"
            }
            Self::KeyChange => {
                "DO NOT pitch-shift to match original key - key change is intentional"
            }
            Self::VocalExtractionArtifacts => {
                "Slight artifacts in vocal extraction are expected - do not over-process"
            }
            Self::LayerArtifacts => {
                "Layer blending artifacts are expected - do not try to 'clean' the mix"
            }
        }
    }
}

impl NeuralContextTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a neural operation and detect intentional artifacts
    pub fn record_operation(
        &mut self,
        model: &str,
        params: HashMap<String, serde_json::Value>,
        description: &str,
    ) {
        let operation = NeuralOperation {
            model: model.to_string(),
            params: params.clone(),
            timestamp: Utc::now(),
            description: description.to_string(),
        };

        // Detect intentional artifacts based on model and params
        self.intentional_artifacts = self.detect_intentional_artifacts(model, &params);
        self.last_neural_operation = Some(operation.clone());
        self.operation_history.push(operation);
    }

    /// Detect intentional artifacts from model and parameters
    fn detect_intentional_artifacts(
        &self,
        model: &str,
        params: &HashMap<String, serde_json::Value>,
    ) -> Vec<IntentionalArtifact> {
        let mut artifacts = Vec::new();

        if model == "style-transfer" {
            if let Some(preset) = params.get("style_preset").and_then(|v| v.as_str()) {
                let preset_lower = preset.to_lowercase();

                if preset_lower.contains("vinyl") {
                    artifacts.push(IntentionalArtifact::HighFrequencyNoise);
                    artifacts.push(IntentionalArtifact::FrequencyRolloff);
                    artifacts.push(IntentionalArtifact::SubtleCrackle);
                }
                if preset_lower.contains("tape") {
                    artifacts.push(IntentionalArtifact::SubtleHiss);
                    artifacts.push(IntentionalArtifact::Saturation);
                    artifacts.push(IntentionalArtifact::FrequencyRolloff);
                }
                if preset_lower.contains("lo_fi") || preset_lower.contains("lofi") {
                    artifacts.push(IntentionalArtifact::Bitcrushing);
                    artifacts.push(IntentionalArtifact::SampleRateArtifacts);
                    artifacts.push(IntentionalArtifact::Noise);
                }
                if preset_lower.contains("transistor") {
                    artifacts.push(IntentionalArtifact::BandwidthLimitation);
                    artifacts.push(IntentionalArtifact::Distortion);
                }
                if preset_lower.contains("vintage") || preset_lower.contains("analog") {
                    artifacts.push(IntentionalArtifact::IntentionalColoration);
                    artifacts.push(IntentionalArtifact::FrequencyRolloff);
                }
            }
        }

        if model == "ace-step" {
            if let Some(mode) = params.get("mode").and_then(|v| v.as_str()) {
                match mode {
                    "cover" => {
                        artifacts.push(IntentionalArtifact::DifferentTimbre);
                        artifacts.push(IntentionalArtifact::CoverTimbre);
                    }
                    "extract" => {
                        artifacts.push(IntentionalArtifact::VocalExtractionArtifacts);
                    }
                    "layer" => {
                        artifacts.push(IntentionalArtifact::LayerArtifacts);
                    }
                    _ => {}
                }
            }

            if let Some(prompt) = params.get("prompt").and_then(|v| v.as_str()) {
                let prompt_lower = prompt.to_lowercase();
                if prompt_lower.contains("vintage") {
                    artifacts.push(IntentionalArtifact::IntentionalColoration);
                    artifacts.push(IntentionalArtifact::FrequencyRolloff);
                }
                // Detect genre transformation
                let genre_keywords = [
                    "jazz", "rock", "classical", "electronic", "hip hop", "metal",
                    "country", "folk", "reggae", "blues", "pop", "r&b", "punk",
                ];
                if genre_keywords.iter().any(|g| prompt_lower.contains(g)) {
                    artifacts.push(IntentionalArtifact::GenreTransformation);
                }
            }

            // Check for explicit tempo/key changes in params
            if params.get("tempo").is_some() || params.get("bpm").is_some() {
                artifacts.push(IntentionalArtifact::TempoChange);
            }
            if params.get("key").is_some() || params.get("pitch").is_some() {
                artifacts.push(IntentionalArtifact::KeyChange);
            }
        }

        artifacts
    }

    /// Get DSP warnings based on current intentional artifacts
    pub fn get_dsp_warnings(&self) -> Vec<&'static str> {
        self.intentional_artifacts
            .iter()
            .map(|a| a.get_warning())
            .collect()
    }

    /// Check if a specific artifact type is present
    pub fn has_artifact(&self, artifact: &IntentionalArtifact) -> bool {
        self.intentional_artifacts.contains(artifact)
    }

    /// Clear all context (e.g., after user resets neural layer)
    pub fn clear(&mut self) {
        self.last_neural_operation = None;
        self.intentional_artifacts.clear();
        // Keep history for reference
    }

    /// Get a summary of current context for agent prompts
    pub fn get_context_summary(&self) -> String {
        if self.intentional_artifacts.is_empty() {
            return "No special neural context - DSP is unrestricted.".to_string();
        }

        let warnings = self.get_dsp_warnings();
        format!(
            "Neural processing context ({} artifacts detected):\n{}",
            self.intentional_artifacts.len(),
            warnings
                .iter()
                .map(|w| format!("  - {}", w))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vinyl_preset_detection() {
        let mut tracker = NeuralContextTracker::new();

        let mut params = HashMap::new();
        params.insert(
            "style_preset".to_string(),
            serde_json::Value::String("vinyl_crackle".to_string()),
        );

        tracker.record_operation("style-transfer", params, "Applied vinyl preset");

        assert!(tracker.has_artifact(&IntentionalArtifact::HighFrequencyNoise));
        assert!(tracker.has_artifact(&IntentionalArtifact::SubtleCrackle));
    }

    #[test]
    fn test_tape_preset_detection() {
        let mut tracker = NeuralContextTracker::new();

        let mut params = HashMap::new();
        params.insert(
            "style_preset".to_string(),
            serde_json::Value::String("tape_warmth".to_string()),
        );

        tracker.record_operation("style-transfer", params, "Applied tape preset");

        assert!(tracker.has_artifact(&IntentionalArtifact::SubtleHiss));
        assert!(tracker.has_artifact(&IntentionalArtifact::Saturation));
    }

    #[test]
    fn test_cover_mode_detection() {
        let mut tracker = NeuralContextTracker::new();

        let mut params = HashMap::new();
        params.insert(
            "mode".to_string(),
            serde_json::Value::String("cover".to_string()),
        );

        tracker.record_operation("ace-step", params, "Created cover version");

        assert!(tracker.has_artifact(&IntentionalArtifact::DifferentTimbre));
    }

    #[test]
    fn test_dsp_warnings() {
        let mut tracker = NeuralContextTracker::new();

        let mut params = HashMap::new();
        params.insert(
            "style_preset".to_string(),
            serde_json::Value::String("vinyl_crackle".to_string()),
        );

        tracker.record_operation("style-transfer", params, "Applied vinyl preset");

        let warnings = tracker.get_dsp_warnings();
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.contains("noise gate")));
    }

    #[test]
    fn test_clear_context() {
        let mut tracker = NeuralContextTracker::new();

        let mut params = HashMap::new();
        params.insert(
            "style_preset".to_string(),
            serde_json::Value::String("vinyl_crackle".to_string()),
        );

        tracker.record_operation("style-transfer", params, "Applied vinyl preset");
        assert!(!tracker.intentional_artifacts.is_empty());

        tracker.clear();
        assert!(tracker.intentional_artifacts.is_empty());
        assert!(tracker.last_neural_operation.is_none());
        // History should be preserved
        assert!(!tracker.operation_history.is_empty());
    }
}
