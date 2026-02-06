//! Integration tests for ACE-Step integration
//!
//! These tests verify the full ACE-Step integration including:
//! - GPU detection
//! - Protocol translation
//! - Error handling
//! - Mock fallback behavior

use nueva::error::NuevaError;
use nueva::neural::{
    can_run_ace_step, gpu_status_summary, GpuInfo, IntentionalArtifact, NeuralContextTracker,
    NeuralModel, NeuralModelParams, NeuralModelRegistry, QuantizationLevel,
};

// ============================================================================
// GPU Detection Tests
// ============================================================================

#[test]
fn test_gpu_detection_returns_result() {
    // GPU detection should never panic, always return valid result
    let (can_run, quantization, reason) = can_run_ace_step();

    // ACE-Step can always run (with CPU fallback)
    assert!(can_run);
    assert!(!reason.is_empty());

    // Quantization should be valid
    assert!(matches!(
        quantization,
        QuantizationLevel::FP32
            | QuantizationLevel::FP16
            | QuantizationLevel::INT8
            | QuantizationLevel::CPU
    ));
}

#[test]
fn test_gpu_status_summary_is_human_readable() {
    let summary = gpu_status_summary();
    assert!(!summary.is_empty());

    // Should contain either GPU info or "No compatible GPU"
    assert!(summary.contains("GPU") || summary.contains("CPU"));
}

#[test]
fn test_quantization_level_ordering() {
    // Higher quality levels require more VRAM
    assert!(QuantizationLevel::FP32.min_vram_gb() > QuantizationLevel::FP16.min_vram_gb());
    assert!(QuantizationLevel::FP16.min_vram_gb() > QuantizationLevel::INT8.min_vram_gb());
    assert!(QuantizationLevel::INT8.min_vram_gb() > QuantizationLevel::CPU.min_vram_gb());
    assert_eq!(QuantizationLevel::CPU.min_vram_gb(), 0.0);
}

// ============================================================================
// Registry Tests
// ============================================================================

#[test]
fn test_registry_has_ace_step() {
    let registry = NeuralModelRegistry::with_defaults();

    // ACE-Step should be registered
    assert!(registry.has_model("ace-step"));

    // Get model and verify info
    let model = registry.get("ace-step").expect("ace-step should be registered");
    let info = model.info();

    assert_eq!(info.id, "ace-step");
    assert!(!info.capabilities.is_empty());
    assert!(info.vram_requirement_gb >= 4.0);
}

#[test]
fn test_registry_with_mocks() {
    let registry = NeuralModelRegistry::with_mocks();

    // All mock models should be present
    assert!(registry.has_model("style-transfer"));
    assert!(registry.has_model("denoise"));
    assert!(registry.has_model("restore"));
    assert!(registry.has_model("enhance"));
    assert!(registry.has_model("ace-step"));
}

#[test]
fn test_registry_find_capability() {
    let registry = NeuralModelRegistry::with_defaults();

    // Should find ACE-Step for cover capability
    let model = registry.find_model_for_capability("cover");
    assert!(model.is_some());
    assert_eq!(model.unwrap(), "ace-step");

    // Should find denoise for noise_removal
    let model = registry.find_model_for_capability("noise_removal");
    assert!(model.is_some());
    assert_eq!(model.unwrap(), "denoise");
}

// ============================================================================
// Context Tracker Tests for ACE-Step Artifacts
// ============================================================================

#[test]
fn test_context_tracks_cover_mode_artifacts() {
    let mut tracker = NeuralContextTracker::new();

    let mut params = std::collections::HashMap::new();
    params.insert(
        "mode".to_string(),
        serde_json::json!("cover"),
    );
    params.insert(
        "prompt".to_string(),
        serde_json::json!("jazz version"),
    );

    tracker.record_operation("ace-step", params, "Created jazz cover");

    // Should detect cover-specific artifacts
    assert!(tracker.has_artifact(&IntentionalArtifact::DifferentTimbre));
    assert!(tracker.has_artifact(&IntentionalArtifact::CoverTimbre));
    assert!(tracker.has_artifact(&IntentionalArtifact::GenreTransformation));
}

#[test]
fn test_context_tracks_extract_mode_artifacts() {
    let mut tracker = NeuralContextTracker::new();

    let mut params = std::collections::HashMap::new();
    params.insert(
        "mode".to_string(),
        serde_json::json!("extract"),
    );

    tracker.record_operation("ace-step", params, "Extracted vocals");

    assert!(tracker.has_artifact(&IntentionalArtifact::VocalExtractionArtifacts));
}

#[test]
fn test_context_tracks_layer_mode_artifacts() {
    let mut tracker = NeuralContextTracker::new();

    let mut params = std::collections::HashMap::new();
    params.insert(
        "mode".to_string(),
        serde_json::json!("layer"),
    );

    tracker.record_operation("ace-step", params, "Added piano layer");

    assert!(tracker.has_artifact(&IntentionalArtifact::LayerArtifacts));
}

#[test]
fn test_context_detects_tempo_key_changes() {
    let mut tracker = NeuralContextTracker::new();

    let mut params = std::collections::HashMap::new();
    params.insert("mode".to_string(), serde_json::json!("cover"));
    params.insert("tempo".to_string(), serde_json::json!(120));
    params.insert("key".to_string(), serde_json::json!("Am"));

    tracker.record_operation("ace-step", params, "Cover with tempo/key change");

    assert!(tracker.has_artifact(&IntentionalArtifact::TempoChange));
    assert!(tracker.has_artifact(&IntentionalArtifact::KeyChange));
}

#[test]
fn test_dsp_warnings_for_ace_step_artifacts() {
    let mut tracker = NeuralContextTracker::new();

    let mut params = std::collections::HashMap::new();
    params.insert("mode".to_string(), serde_json::json!("cover"));
    params.insert("prompt".to_string(), serde_json::json!("electronic version"));

    tracker.record_operation("ace-step", params, "Created electronic cover");

    let warnings = tracker.get_dsp_warnings();
    assert!(!warnings.is_empty());

    // Should warn against EQ corrections for cover timbre
    let has_timbre_warning = warnings.iter().any(|w| w.contains("EQ") || w.contains("timbre"));
    assert!(has_timbre_warning);
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_acestep_unavailable_error_has_recovery() {
    let error = NuevaError::AceStepUnavailable {
        reason: "Bridge not running".to_string(),
    };

    let suggestions = error.recovery_suggestions();
    assert!(!suggestions.is_empty());
    assert!(suggestions.iter().any(|s| s.contains("bridge") || s.contains("ACE-Step")));
}

#[test]
fn test_acestep_timeout_error_has_recovery() {
    let error = NuevaError::AceStepTimeout { timeout_ms: 300000 };

    let suggestions = error.recovery_suggestions();
    assert!(!suggestions.is_empty());
    assert!(suggestions.iter().any(|s| s.contains("timeout") || s.contains("loading")));
}

#[test]
fn test_insufficient_vram_error_has_recovery() {
    let error = NuevaError::InsufficientVram {
        required_gb: 4.0,
        available_gb: 2.0,
    };

    let suggestions = error.recovery_suggestions();
    assert!(!suggestions.is_empty());
    assert!(suggestions.iter().any(|s| s.contains("CPU") || s.contains("quantization")));
}

#[test]
fn test_bridge_connection_error_has_recovery() {
    let error = NuevaError::BridgeConnectionError {
        message: "Connection refused".to_string(),
    };

    let suggestions = error.recovery_suggestions();
    assert!(!suggestions.is_empty());
    assert!(suggestions.iter().any(|s| s.contains("bridge") || s.contains("port")));
}

// ============================================================================
// Model Parameter Validation Tests
// ============================================================================

#[test]
fn test_mock_ace_step_processes_params() {
    let registry = NeuralModelRegistry::with_mocks();
    let model = registry.get("ace-step").unwrap();

    let params = NeuralModelParams::new()
        .with_param("mode", "cover")
        .with_param("prompt", "jazz version")
        .with_param("intensity", 0.8f32);

    // Mock processing should succeed
    let result = model.process(
        std::path::Path::new("/tmp/test_input.wav"),
        std::path::Path::new("/tmp/test_output.wav"),
        &params,
    );

    // Mock always succeeds
    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(result.success);
    assert!(!result.description.is_empty());
}

// ============================================================================
// Feature Flag Tests
// ============================================================================

#[test]
fn test_acestep_mock_feature_provides_mock() {
    // With acestep-mock feature (or default), registry should have mock
    let registry = NeuralModelRegistry::with_mocks();

    let model = registry.get("ace-step").unwrap();

    // Mock model info should indicate it's a mock
    let info = model.info();
    // The mock version contains "mock"
    assert!(
        info.version.contains("mock") || info.description.contains("MOCK"),
        "Expected mock model, got version: {}",
        info.version
    );
}

// ============================================================================
// End-to-End Flow Tests
// ============================================================================

#[test]
fn test_full_neural_processing_flow_with_mock() {
    // 1. Create registry
    let registry = NeuralModelRegistry::with_mocks();

    // 2. Find appropriate model for task
    let model_id = registry.find_model_for_capability("cover");
    assert_eq!(model_id, Some("ace-step"));

    // 3. Get the model
    let model = registry.get("ace-step").unwrap();

    // 4. Build parameters
    let params = NeuralModelParams::new()
        .with_param("mode", "cover")
        .with_param("prompt", "lo-fi hip hop version")
        .with_param("intensity", 0.7f32);

    // 5. Process (mock)
    let result = model
        .process(
            std::path::Path::new("/tmp/input.wav"),
            std::path::Path::new("/tmp/output.wav"),
            &params,
        )
        .unwrap();

    assert!(result.success);

    // 6. Track context
    let mut tracker = NeuralContextTracker::new();
    let mut tracked_params = std::collections::HashMap::new();
    tracked_params.insert("mode".to_string(), serde_json::json!("cover"));
    tracked_params.insert(
        "prompt".to_string(),
        serde_json::json!("lo-fi hip hop version"),
    );

    tracker.record_operation("ace-step", tracked_params, &result.description);

    // 7. Verify artifacts tracked
    assert!(tracker.has_artifact(&IntentionalArtifact::DifferentTimbre));

    // 8. Get DSP warnings
    let summary = tracker.get_context_summary();
    assert!(summary.contains("artifacts detected"));
}

#[test]
fn test_genre_detection_in_prompts() {
    let mut tracker = NeuralContextTracker::new();

    let genres = [
        "jazz",
        "rock",
        "classical",
        "electronic",
        "hip hop",
        "metal",
        "country",
        "folk",
        "reggae",
        "blues",
    ];

    for genre in genres {
        tracker.clear();

        let mut params = std::collections::HashMap::new();
        params.insert("mode".to_string(), serde_json::json!("transform"));
        params.insert("prompt".to_string(), serde_json::json!(format!("{} version", genre)));

        tracker.record_operation("ace-step", params, &format!("Created {} version", genre));

        assert!(
            tracker.has_artifact(&IntentionalArtifact::GenreTransformation),
            "Should detect genre transformation for: {}",
            genre
        );
    }
}
