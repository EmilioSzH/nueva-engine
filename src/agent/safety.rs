//! Safety checks for audio processing
//!
//! Implements the "Do No Harm" rules from the spec:
//! - Clipping prevention (auto-limiter)
//! - Phase protection (warn if correlation < 0.2)
//! - Loudness sanity (warn if LUFS > -5)
//! - Duration validation (output matches input within 0.1s)

use serde::{Deserialize, Serialize};

/// Safety thresholds per spec
pub mod thresholds {
    /// Peak level that triggers clipping warning (dBFS)
    pub const CLIPPING_WARN: f32 = -1.0;

    /// Peak level that triggers auto-limiter (dBFS)
    pub const CLIPPING_LIMIT: f32 = 0.0;

    /// Default limiter ceiling (dBFS)
    pub const LIMITER_CEILING: f32 = -1.0;

    /// Stereo correlation below which we warn (phase issues)
    pub const PHASE_WARN: f32 = 0.3;

    /// Stereo correlation below which we block/strongly warn
    pub const PHASE_CRITICAL: f32 = 0.2;

    /// LUFS above which we warn (extremely loud)
    pub const LOUDNESS_WARN: f32 = -5.0;

    /// LUFS that indicates "very loud, likely limited"
    pub const LOUDNESS_VERY_LOUD: f32 = -9.0;

    /// LUFS that indicates "quiet, may need gain"
    pub const LOUDNESS_QUIET: f32 = -20.0;

    /// Noise floor above which we suggest denoise (dB)
    pub const NOISE_FLOOR_WARN: f32 = -50.0;

    /// Maximum allowed duration difference (seconds)
    pub const DURATION_TOLERANCE: f32 = 0.1;
}

/// Audio analysis results (matches spec §5.5)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AudioAnalysis {
    // Loudness (EBU R128)
    /// RMS level in dB
    pub rms_db: f32,

    /// Peak level in dBFS
    pub peak_db: f32,

    /// True peak level in dBFS
    pub true_peak_db: f32,

    /// Integrated LUFS (EBU R128)
    pub lufs_integrated: f32,

    /// Short-term LUFS
    pub lufs_short_term: f32,

    /// Momentary LUFS max
    pub lufs_momentary_max: f32,

    /// Loudness range (LRA) in dB
    pub dynamic_range_db: f32,

    // Clipping
    /// Percentage of samples that are clipped
    pub clip_percentage: f32,

    /// Number of clipping regions
    pub clip_region_count: u32,

    // Spectral
    /// Spectral centroid in Hz
    pub spectral_centroid_hz: f32,

    /// 85% energy rolloff frequency in Hz
    pub spectral_rolloff_hz: f32,

    /// Spectral flatness (0=tonal, 1=noisy)
    pub spectral_flatness: f32,

    // Stereo
    /// Stereo correlation (-1 to 1)
    pub stereo_correlation: f32,

    /// Stereo width (0 to 1)
    pub stereo_width: f32,

    /// Balance (-1=L, 0=center, 1=R)
    pub balance: f32,

    // Noise/Quality
    /// Noise floor in dB
    pub noise_floor_db: f32,

    /// Whether DC offset is present
    pub has_dc_offset: bool,

    /// DC offset value if present
    pub dc_offset_value: f32,

    // Musical (optional)
    /// Estimated BPM
    pub estimated_bpm: Option<f32>,

    /// Estimated key (e.g., "C major")
    pub estimated_key: Option<String>,

    /// Key detection confidence (0-1)
    pub key_confidence: Option<f32>,

    // Duration
    /// Duration in seconds
    pub duration_seconds: f32,

    /// Sample rate
    pub sample_rate: u32,

    /// Number of channels
    pub channels: u8,
}

impl AudioAnalysis {
    /// Check if audio has clipping
    pub fn has_clipping(&self) -> bool {
        self.clip_percentage > 0.0 || self.peak_db >= thresholds::CLIPPING_LIMIT
    }

    /// Check if audio is near clipping
    pub fn is_near_clipping(&self) -> bool {
        self.peak_db > thresholds::CLIPPING_WARN
    }

    /// Check if audio has phase issues
    pub fn has_phase_issues(&self) -> bool {
        self.channels >= 2 && self.stereo_correlation < thresholds::PHASE_WARN
    }

    /// Check if audio has critical phase issues
    pub fn has_critical_phase_issues(&self) -> bool {
        self.channels >= 2 && self.stereo_correlation < thresholds::PHASE_CRITICAL
    }

    /// Check if audio is extremely loud
    pub fn is_extremely_loud(&self) -> bool {
        self.lufs_integrated > thresholds::LOUDNESS_WARN
    }

    /// Check if audio is very loud (likely already limited)
    pub fn is_very_loud(&self) -> bool {
        self.lufs_integrated > thresholds::LOUDNESS_VERY_LOUD
    }

    /// Check if audio is quiet
    pub fn is_quiet(&self) -> bool {
        self.lufs_integrated < thresholds::LOUDNESS_QUIET
    }

    /// Check if audio is noisy
    pub fn is_noisy(&self) -> bool {
        self.noise_floor_db > thresholds::NOISE_FLOOR_WARN
    }

    /// Check if audio is mono
    pub fn is_mono(&self) -> bool {
        self.channels == 1
    }

    /// Check if audio is stereo
    pub fn is_stereo(&self) -> bool {
        self.channels == 2
    }

    /// Get character description (bright/dark)
    pub fn get_character(&self) -> &'static str {
        if self.spectral_centroid_hz > 4000.0 {
            "bright/harsh"
        } else if self.spectral_centroid_hz < 1500.0 {
            "dark/warm"
        } else {
            "balanced"
        }
    }

    /// Generate human-readable summary of issues
    pub fn to_human_summary(&self) -> String {
        let mut issues = Vec::new();

        if self.has_clipping() {
            issues.push(format!(
                "⚠️ CLIPPING: {:.1}% samples clipped",
                self.clip_percentage
            ));
        }

        if self.is_extremely_loud() {
            issues.push(format!(
                "⚠️ VERY LOUD: {:.1} LUFS - likely over-limited",
                self.lufs_integrated
            ));
        } else if self.is_quiet() {
            issues.push(format!(
                "Quiet: {:.1} LUFS - may need gain",
                self.lufs_integrated
            ));
        }

        if self.is_noisy() {
            issues.push(format!(
                "Noisy: {:.1} dB noise floor",
                self.noise_floor_db
            ));
        }

        if self.has_dc_offset {
            issues.push("DC offset present - recommend HP filter".to_string());
        }

        if self.has_phase_issues() {
            issues.push(format!(
                "⚠️ Phase issues detected (correlation: {:.2})",
                self.stereo_correlation
            ));
        }

        let character = self.get_character();
        if character != "balanced" {
            issues.push(format!("{} character", character));
        }

        if issues.is_empty() {
            "Audio appears healthy".to_string()
        } else {
            issues.join("\n")
        }
    }
}

/// Safety check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyCheckResult {
    /// Whether the change is safe to apply
    pub is_safe: bool,

    /// Safety issues found
    pub issues: Vec<SafetyIssue>,

    /// Automatic mitigations applied
    pub mitigations: Vec<SafetyMitigation>,

    /// Warnings to show user
    pub warnings: Vec<String>,
}

impl SafetyCheckResult {
    pub fn safe() -> Self {
        Self {
            is_safe: true,
            issues: Vec::new(),
            mitigations: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn with_issue(mut self, issue: SafetyIssue) -> Self {
        self.issues.push(issue);
        self
    }

    pub fn with_mitigation(mut self, mitigation: SafetyMitigation) -> Self {
        self.mitigations.push(mitigation);
        self
    }

    pub fn with_warning(mut self, warning: &str) -> Self {
        self.warnings.push(warning.to_string());
        self
    }

    pub fn mark_unsafe(mut self) -> Self {
        self.is_safe = false;
        self
    }

    pub fn has_issues(&self) -> bool {
        !self.issues.is_empty()
    }

    pub fn has_mitigations(&self) -> bool {
        !self.mitigations.is_empty()
    }
}

/// Types of safety issues
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SafetyIssue {
    /// Output would clip
    Clipping {
        predicted_peak_db: i32,
    },

    /// Phase correlation would drop too low
    PhaseCorrelation {
        predicted_correlation: i32, // Stored as percentage
    },

    /// Output would be extremely loud
    ExcessiveLoudness {
        predicted_lufs: i32,
    },

    /// Duration would change significantly
    DurationMismatch {
        expected_seconds: i32,
        actual_seconds: i32,
    },

    /// Would undo intentional artifacts
    IntentionalArtifactRemoval {
        artifact: String,
    },
}

/// Automatic mitigations the safety system can apply
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SafetyMitigation {
    /// Auto-inserted limiter to prevent clipping
    AutoLimiter {
        ceiling_db: i32, // -1 dB typically
    },

    /// Reduced effect intensity
    ReducedIntensity {
        effect: String,
        original_intensity: i32, // Percentage
        new_intensity: i32,
    },

    /// Skipped problematic operation
    SkippedOperation {
        operation: String,
        reason: String,
    },
}

/// Main safety checker
pub struct SafetyChecker {
    /// Current audio analysis
    analysis: Option<AudioAnalysis>,

    /// Whether to auto-apply mitigations
    auto_mitigate: bool,
}

impl SafetyChecker {
    pub fn new() -> Self {
        Self {
            analysis: None,
            auto_mitigate: true,
        }
    }

    /// Set current audio analysis
    pub fn set_analysis(&mut self, analysis: AudioAnalysis) {
        self.analysis = Some(analysis);
    }

    /// Enable/disable automatic mitigations
    pub fn set_auto_mitigate(&mut self, enable: bool) {
        self.auto_mitigate = enable;
    }

    /// Check if a gain change would cause clipping
    pub fn check_gain(&self, gain_db: f32) -> SafetyCheckResult {
        let mut result = SafetyCheckResult::safe();

        if let Some(ref analysis) = self.analysis {
            let predicted_peak = analysis.peak_db + gain_db;

            if predicted_peak >= thresholds::CLIPPING_LIMIT {
                result = result.with_issue(SafetyIssue::Clipping {
                    predicted_peak_db: predicted_peak as i32,
                });

                if self.auto_mitigate {
                    result = result
                        .with_mitigation(SafetyMitigation::AutoLimiter {
                            ceiling_db: thresholds::LIMITER_CEILING as i32,
                        })
                        .with_warning("I added a limiter to prevent clipping");
                } else {
                    result = result
                        .with_warning(&format!(
                            "This would cause clipping (peak: {:.1} dBFS)",
                            predicted_peak
                        ))
                        .mark_unsafe();
                }
            } else if predicted_peak > thresholds::CLIPPING_WARN {
                result = result.with_warning(&format!(
                    "Peak will be close to clipping ({:.1} dBFS)",
                    predicted_peak
                ));
            }
        }

        result
    }

    /// Check if stereo effect would cause phase issues
    pub fn check_stereo_effect(&self, predicted_correlation: f32) -> SafetyCheckResult {
        let mut result = SafetyCheckResult::safe();

        if predicted_correlation < thresholds::PHASE_CRITICAL {
            result = result
                .with_issue(SafetyIssue::PhaseCorrelation {
                    predicted_correlation: (predicted_correlation * 100.0) as i32,
                })
                .with_warning(&format!(
                    "This would cause phase issues (correlation: {:.2}). Consider reducing effect intensity.",
                    predicted_correlation
                ))
                .mark_unsafe();
        } else if predicted_correlation < thresholds::PHASE_WARN {
            result = result.with_warning(&format!(
                "Phase correlation may be affected ({:.2})",
                predicted_correlation
            ));
        }

        result
    }

    /// Check if loudness would exceed limits
    pub fn check_loudness(&self, predicted_lufs: f32) -> SafetyCheckResult {
        let mut result = SafetyCheckResult::safe();

        if predicted_lufs > thresholds::LOUDNESS_WARN {
            result = result
                .with_issue(SafetyIssue::ExcessiveLoudness {
                    predicted_lufs: predicted_lufs as i32,
                })
                .with_warning(&format!(
                    "Output would be extremely loud ({:.1} LUFS) - exceeds all streaming standards",
                    predicted_lufs
                ));
        }

        result
    }

    /// Check if duration change is acceptable
    pub fn check_duration(&self, original_seconds: f32, new_seconds: f32) -> SafetyCheckResult {
        let mut result = SafetyCheckResult::safe();

        let diff = (new_seconds - original_seconds).abs();
        if diff > thresholds::DURATION_TOLERANCE {
            result = result
                .with_issue(SafetyIssue::DurationMismatch {
                    expected_seconds: original_seconds as i32,
                    actual_seconds: new_seconds as i32,
                })
                .with_warning(&format!(
                    "Duration changed by {:.2}s (from {:.2}s to {:.2}s)",
                    diff, original_seconds, new_seconds
                ));
        }

        result
    }

    /// Get recommendations based on current analysis
    pub fn get_recommendations(&self) -> Vec<SafetyRecommendation> {
        let mut recommendations = Vec::new();

        if let Some(ref analysis) = self.analysis {
            if analysis.has_clipping() {
                recommendations.push(SafetyRecommendation {
                    priority: RecommendationPriority::High,
                    message: "Audio has clipping - consider restore/declip before other processing"
                        .to_string(),
                    suggested_action: Some("Use 'restore' model in declip mode".to_string()),
                });
            }

            if analysis.is_very_loud() {
                recommendations.push(SafetyRecommendation {
                    priority: RecommendationPriority::Medium,
                    message: format!(
                        "Audio is very loud ({:.1} LUFS) - be cautious with gain",
                        analysis.lufs_integrated
                    ),
                    suggested_action: None,
                });
            }

            if analysis.is_quiet() {
                recommendations.push(SafetyRecommendation {
                    priority: RecommendationPriority::Low,
                    message: format!(
                        "Audio is quiet ({:.1} LUFS) - may benefit from gain/limiting",
                        analysis.lufs_integrated
                    ),
                    suggested_action: Some("Consider adding gain and/or limiter".to_string()),
                });
            }

            if analysis.is_noisy() {
                recommendations.push(SafetyRecommendation {
                    priority: RecommendationPriority::Medium,
                    message: format!(
                        "Audio has noise ({:.1} dB floor)",
                        analysis.noise_floor_db
                    ),
                    suggested_action: Some("Consider using 'denoise' model".to_string()),
                });
            }

            if analysis.has_phase_issues() {
                recommendations.push(SafetyRecommendation {
                    priority: RecommendationPriority::High,
                    message: format!(
                        "Audio has phase issues (correlation: {:.2})",
                        analysis.stereo_correlation
                    ),
                    suggested_action: Some("Be cautious with stereo widening effects".to_string()),
                });
            }

            if analysis.has_dc_offset {
                recommendations.push(SafetyRecommendation {
                    priority: RecommendationPriority::Low,
                    message: "DC offset detected".to_string(),
                    suggested_action: Some("Add high-pass filter at 20-30Hz".to_string()),
                });
            }
        }

        recommendations
    }
}

impl Default for SafetyChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// A recommendation from the safety system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyRecommendation {
    /// Priority level
    pub priority: RecommendationPriority,

    /// Description of the issue/recommendation
    pub message: String,

    /// Suggested action (if any)
    pub suggested_action: Option<String>,
}

/// Priority levels for recommendations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendationPriority {
    Low,
    Medium,
    High,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_analysis() -> AudioAnalysis {
        AudioAnalysis {
            peak_db: -6.0,
            lufs_integrated: -14.0,
            stereo_correlation: 0.8,
            noise_floor_db: -60.0,
            duration_seconds: 180.0,
            channels: 2,
            ..Default::default()
        }
    }

    #[test]
    fn test_gain_check_safe() {
        let mut checker = SafetyChecker::new();
        checker.set_analysis(make_analysis());

        let result = checker.check_gain(3.0); // -6 + 3 = -3dB, safe
        assert!(result.is_safe);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn test_gain_check_clipping_with_mitigation() {
        let mut checker = SafetyChecker::new();
        checker.set_analysis(make_analysis());

        let result = checker.check_gain(10.0); // -6 + 10 = +4dB, clips
        assert!(result.is_safe); // Still safe because mitigation applied
        assert!(!result.issues.is_empty());
        assert!(result
            .mitigations
            .iter()
            .any(|m| matches!(m, SafetyMitigation::AutoLimiter { .. })));
        assert!(result.warnings.iter().any(|w| w.contains("limiter")));
    }

    #[test]
    fn test_gain_check_clipping_no_mitigation() {
        let mut checker = SafetyChecker::new();
        checker.set_analysis(make_analysis());
        checker.set_auto_mitigate(false);

        let result = checker.check_gain(10.0);
        assert!(!result.is_safe);
        assert!(!result.issues.is_empty());
        assert!(result.mitigations.is_empty());
    }

    #[test]
    fn test_phase_check() {
        let checker = SafetyChecker::new();

        // Safe
        let result = checker.check_stereo_effect(0.5);
        assert!(result.is_safe);

        // Warning
        let result = checker.check_stereo_effect(0.25);
        assert!(result.is_safe);
        assert!(!result.warnings.is_empty());

        // Critical
        let result = checker.check_stereo_effect(0.15);
        assert!(!result.is_safe);
    }

    #[test]
    fn test_loudness_check() {
        let checker = SafetyChecker::new();

        // Safe
        let result = checker.check_loudness(-14.0);
        assert!(result.is_safe);

        // Warning
        let result = checker.check_loudness(-3.0);
        assert!(result.is_safe); // Warning only, not blocking
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_duration_check() {
        let checker = SafetyChecker::new();

        // OK - within tolerance
        let result = checker.check_duration(180.0, 180.05);
        assert!(result.issues.is_empty());

        // Exceeds tolerance
        let result = checker.check_duration(180.0, 180.5);
        assert!(!result.issues.is_empty());
    }

    #[test]
    fn test_audio_analysis_issues() {
        let mut analysis = make_analysis();

        assert!(!analysis.has_clipping());
        assert!(!analysis.has_phase_issues());
        assert!(!analysis.is_extremely_loud());

        analysis.peak_db = 0.1;
        assert!(analysis.has_clipping());

        analysis.stereo_correlation = 0.1;
        assert!(analysis.has_phase_issues());
        assert!(analysis.has_critical_phase_issues());

        analysis.lufs_integrated = -3.0;
        assert!(analysis.is_extremely_loud());
    }

    #[test]
    fn test_recommendations() {
        let mut checker = SafetyChecker::new();

        let mut analysis = make_analysis();
        analysis.clip_percentage = 1.0;
        analysis.noise_floor_db = -40.0;

        checker.set_analysis(analysis);

        let recs = checker.get_recommendations();
        assert!(recs.iter().any(|r| r.message.contains("clipping")));
        assert!(recs.iter().any(|r| r.message.contains("noise")));
    }

    #[test]
    fn test_human_summary() {
        let mut analysis = make_analysis();
        analysis.lufs_integrated = -3.0;
        analysis.stereo_correlation = 0.1;

        let summary = analysis.to_human_summary();
        assert!(summary.contains("VERY LOUD"));
        assert!(summary.contains("Phase issues"));
    }
}
