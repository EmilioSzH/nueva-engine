//! Project Lifecycle Integration Tests
//!
//! Tests for creating, modifying, saving, and loading projects.

use nueva::audio::AudioBuffer;
use nueva::layers::LayerManager;

#[test]
fn test_project_create_and_load_audio() {
    // Create a new layer manager (represents a project)
    let mut manager = LayerManager::new();
    assert!(!manager.has_source());

    // Load source audio
    let source = AudioBuffer::sine_wave(440.0, 2.0, 44100);
    manager.load_source(source);
    assert!(manager.has_source());

    // Verify source properties
    let loaded = manager.source().unwrap();
    assert_eq!(loaded.sample_rate(), 44100);
    assert_eq!(loaded.channels(), 1);
    assert!((loaded.duration() - 2.0).abs() < 0.01);
}

#[test]
fn test_layer_cascade() {
    let mut manager = LayerManager::new();

    // Load source
    let source = AudioBuffer::sine_wave(440.0, 1.0, 44100);
    manager.load_source(source);

    // Without AI state, active audio should be source
    let active = manager.active_audio().unwrap();
    assert_eq!(active.num_frames(), 44100);

    // Set AI state (simulated AI processing)
    let ai_result = AudioBuffer::sine_wave(880.0, 1.0, 44100);
    manager.set_ai_state(ai_result);

    // Active audio should now be AI state
    assert!(manager.ai_state().is_some());
}
