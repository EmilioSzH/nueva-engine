//! Intent analysis for user prompts
//!
//! Extracts structured intent from natural language.

use serde::{Deserialize, Serialize};

/// Analyzed intent from a user prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    /// Original prompt
    pub original: String,

    /// Lowercase prompt for matching
    pub prompt_lower: String,

    /// User explicitly asked for DSP
    pub explicit_dsp_request: bool,

    /// User explicitly asked for AI/neural
    pub explicit_neural_request: bool,

    /// Request is complex (multiple effects, vague goals)
    pub is_complex: bool,

    /// Intensity modifier (0.0-1.0)
    pub intensity: f32,

    /// Extracted effect types mentioned
    pub mentioned_effects: Vec<String>,

    /// Extracted parameters (e.g., "3dB at 1kHz")
    pub extracted_params: Vec<ExtractedParam>,
}

/// A parameter extracted from natural language
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedParam {
    pub param_type: String,
    pub value: f32,
    pub unit: Option<String>,
}

impl Intent {
    /// Analyze a prompt and extract intent
    pub fn analyze(prompt: &str) -> Self {
        let prompt_lower = prompt.to_lowercase();

        let explicit_dsp = Self::check_explicit_dsp(&prompt_lower);
        let explicit_neural = Self::check_explicit_neural(&prompt_lower);
        let intensity = Self::extract_intensity(&prompt_lower);
        let mentioned_effects = Self::extract_effects(&prompt_lower);
        let extracted_params = Self::extract_params(&prompt_lower);
        let is_complex = Self::check_complexity(&prompt_lower, &mentioned_effects);

        Self {
            original: prompt.to_string(),
            prompt_lower,
            explicit_dsp_request: explicit_dsp,
            explicit_neural_request: explicit_neural,
            is_complex,
            intensity,
            mentioned_effects,
            extracted_params,
        }
    }

    fn check_explicit_dsp(prompt: &str) -> bool {
        const DSP_EXPLICIT: &[&str] = &[
            "add an eq",
            "add eq",
            "add a compressor",
            "add compressor",
            "add reverb",
            "add delay",
            "use dsp",
            "just eq",
            "only compress",
        ];

        DSP_EXPLICIT.iter().any(|p| prompt.contains(p))
    }

    fn check_explicit_neural(prompt: &str) -> bool {
        const NEURAL_EXPLICIT: &[&str] = &[
            "use ai",
            "use neural",
            "ai process",
            "neural process",
            "style transfer",
            "use the ai",
            "use machine learning",
        ];

        NEURAL_EXPLICIT.iter().any(|p| prompt.contains(p))
    }

    fn extract_intensity(prompt: &str) -> f32 {
        // Intensity modifiers per spec §6.5
        const INTENSITY_MODIFIERS: &[(&[&str], f32)] = &[
            // Small
            (
                &["a bit", "slightly", "a little", "a touch", "subtly"],
                0.3,
            ),
            // Medium (implicit)
            (&["some", "more"], 0.5),
            // Large
            (
                &["much", "a lot", "significantly", "considerably"],
                0.7,
            ),
            // Extreme
            (&["extremely", "very", "heavily", "drastically"], 0.9),
        ];

        for (modifiers, intensity) in INTENSITY_MODIFIERS {
            for modifier in *modifiers {
                if prompt.contains(modifier) {
                    return *intensity;
                }
            }
        }

        0.5 // Default medium
    }

    fn extract_effects(prompt: &str) -> Vec<String> {
        const EFFECT_KEYWORDS: &[(&str, &str)] = &[
            ("eq", "eq"),
            ("equalizer", "eq"),
            ("equalize", "eq"),
            ("compressor", "compressor"),
            ("compression", "compressor"),
            ("compress", "compressor"),
            ("reverb", "reverb"),
            ("delay", "delay"),
            ("echo", "delay"),
            ("limiter", "limiter"),
            ("limit", "limiter"),
            ("gate", "gate"),
            ("saturation", "saturation"),
            ("distortion", "saturation"),
        ];

        let mut effects = Vec::new();
        for (keyword, effect) in EFFECT_KEYWORDS {
            if prompt.contains(keyword) && !effects.contains(&effect.to_string()) {
                effects.push(effect.to_string());
            }
        }
        effects
    }

    fn extract_params(prompt: &str) -> Vec<ExtractedParam> {
        let mut params = Vec::new();

        // Extract dB values
        if let Some(db) = Self::extract_db_value(prompt) {
            params.push(ExtractedParam {
                param_type: "gain".to_string(),
                value: db,
                unit: Some("dB".to_string()),
            });
        }

        // Extract frequency values
        if let Some(freq) = Self::extract_freq_value(prompt) {
            params.push(ExtractedParam {
                param_type: "frequency".to_string(),
                value: freq,
                unit: Some("Hz".to_string()),
            });
        }

        // Extract ratio values (e.g., "4:1")
        if let Some(ratio) = Self::extract_ratio_value(prompt) {
            params.push(ExtractedParam {
                param_type: "ratio".to_string(),
                value: ratio,
                unit: None,
            });
        }

        params
    }

    fn extract_db_value(prompt: &str) -> Option<f32> {
        // Simple regex-like matching for "NdB" or "N dB"
        let words: Vec<&str> = prompt.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            // Check for "3db", "3dB", "-6db", etc.
            if word.to_lowercase().ends_with("db") {
                let num_part = &word[..word.len() - 2];
                if let Ok(val) = num_part.parse::<f32>() {
                    return Some(val);
                }
            }
            // Check for "3 dB", "-6 dB"
            if word.to_lowercase() == "db" && i > 0 {
                if let Ok(val) = words[i - 1].parse::<f32>() {
                    return Some(val);
                }
            }
        }
        None
    }

    fn extract_freq_value(prompt: &str) -> Option<f32> {
        let words: Vec<&str> = prompt.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            // Check for "1000hz", "1khz", "1kHz"
            let word_lower = word.to_lowercase();
            if word_lower.ends_with("hz") {
                let num_part = &word_lower[..word_lower.len() - 2];
                if num_part.ends_with('k') {
                    let num = &num_part[..num_part.len() - 1];
                    if let Ok(val) = num.parse::<f32>() {
                        return Some(val * 1000.0);
                    }
                } else if let Ok(val) = num_part.parse::<f32>() {
                    return Some(val);
                }
            }
            // Check for "1000 Hz", "1 kHz"
            if word_lower == "hz" || word_lower == "khz" && i > 0 {
                if let Ok(val) = words[i - 1].parse::<f32>() {
                    return Some(if word_lower == "khz" {
                        val * 1000.0
                    } else {
                        val
                    });
                }
            }
        }
        None
    }

    fn extract_ratio_value(prompt: &str) -> Option<f32> {
        // Match patterns like "4:1", "8:1"
        for word in prompt.split_whitespace() {
            if word.contains(':') {
                let parts: Vec<&str> = word.split(':').collect();
                if parts.len() == 2 {
                    if let (Ok(num), Ok(denom)) =
                        (parts[0].parse::<f32>(), parts[1].parse::<f32>())
                    {
                        if denom == 1.0 {
                            return Some(num);
                        }
                    }
                }
            }
        }
        None
    }

    fn check_complexity(prompt: &str, effects: &[String]) -> bool {
        // Multiple effects = complex
        if effects.len() > 1 {
            return true;
        }

        // Vague goals
        const VAGUE_INDICATORS: &[&str] = &["better", "improve", "fix", "enhance", "professional"];

        if VAGUE_INDICATORS.iter().any(|v| prompt.contains(v)) {
            return true;
        }

        // "and" joining multiple requests
        if prompt.contains(" and ") {
            return true;
        }

        false
    }
}

/// Analyzer for generating clarification questions
pub struct IntentAnalyzer;

impl IntentAnalyzer {
    /// Get clarification question for ambiguous intent
    pub fn get_clarification(intent: &Intent) -> Option<String> {
        let prompt = &intent.prompt_lower;

        if prompt.contains("improve") || prompt.contains("better") {
            return Some(
                "I'd be happy to help improve this! Could you tell me more about what aspect you'd like to focus on? For example:\n\
                 - Clarity and presence\n\
                 - Warmth and fullness\n\
                 - Punch and energy\n\
                 - Noise reduction\n\
                 - Overall polish".to_string()
            );
        }

        if prompt.contains("fix") {
            return Some(
                "What specifically needs fixing? I can help with:\n\
                 - Noise or hiss\n\
                 - Clipping/distortion\n\
                 - Muddy or harsh frequencies\n\
                 - Lack of punch or presence"
                    .to_string(),
            );
        }

        if prompt.contains("warmer") && intent.is_complex {
            return Some(
                "When you say 'warmer', do you mean:\n\
                 1. Gentle high-frequency roll-off and bass boost (quick EQ adjustment)\n\
                 2. Analog-style saturation and character (subtle processing)\n\
                 3. Full vintage transformation (AI style transfer)\n\n\
                 I'd recommend starting with option 1 - it's quick and you can always go further."
                    .to_string(),
            );
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explicit_dsp_detection() {
        let intent = Intent::analyze("add an EQ to the track");
        assert!(intent.explicit_dsp_request);
    }

    #[test]
    fn test_explicit_neural_detection() {
        let intent = Intent::analyze("use AI to transform this");
        assert!(intent.explicit_neural_request);
    }

    #[test]
    fn test_intensity_extraction() {
        assert_eq!(Intent::analyze("make it slightly louder").intensity, 0.3);
        assert_eq!(Intent::analyze("make it much louder").intensity, 0.7);
        assert_eq!(Intent::analyze("make it extremely bright").intensity, 0.9);
    }

    #[test]
    fn test_db_extraction() {
        let intent = Intent::analyze("add 3dB at 1kHz");
        assert!(intent
            .extracted_params
            .iter()
            .any(|p| p.param_type == "gain" && (p.value - 3.0).abs() < 0.01));
    }

    #[test]
    fn test_frequency_extraction() {
        let intent = Intent::analyze("boost 1kHz");
        assert!(intent
            .extracted_params
            .iter()
            .any(|p| p.param_type == "frequency" && (p.value - 1000.0).abs() < 0.01));
    }

    #[test]
    fn test_ratio_extraction() {
        let intent = Intent::analyze("compress with 4:1 ratio");
        assert!(intent
            .extracted_params
            .iter()
            .any(|p| p.param_type == "ratio" && (p.value - 4.0).abs() < 0.01));
    }

    #[test]
    fn test_complexity_detection() {
        let intent = Intent::analyze("add EQ and compression");
        assert!(intent.is_complex);

        let intent2 = Intent::analyze("make it better");
        assert!(intent2.is_complex);
    }

    #[test]
    fn test_effect_extraction() {
        let intent = Intent::analyze("add compression and reverb");
        assert!(intent.mentioned_effects.contains(&"compressor".to_string()));
        assert!(intent.mentioned_effects.contains(&"reverb".to_string()));
    }
}
