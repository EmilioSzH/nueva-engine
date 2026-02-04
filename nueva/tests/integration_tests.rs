//! Integration Tests
//!
//! Tests for component interaction and full pipeline verification.

use nueva::audio::{AudioBuffer, load_wav, save_wav};
use nueva::audio::verification::{AudioAnalysis, calculate_rms_db};
use nueva::dsp::chain::DspChain;
use nueva::dsp::effects::GainEffect;
use nueva::layers::LayerManager;
use tempfile::tempdir;

// === Project Lifecycle Tests ===

#[test]
fn test_project_create_and_load_audio() {
    let mut manager = LayerManager::new();
    assert!(!manager.has_source());

    let source = AudioBuffer::sine_wave(440.0, 2.0, 44100);
    manager.load_source(source);
    assert!(manager.has_source());

    let loaded = manager.source().unwrap();
    assert_eq!(loaded.sample_rate(), 44100);
    assert_eq!(loaded.channels(), 1);
    assert!((loaded.duration() - 2.0).abs() < 0.01);
}

#[test]
fn test_layer_cascade() {
    let mut manager = LayerManager::new();
    let source = AudioBuffer::sine_wave(440.0, 1.0, 44100);
    manager.load_source(source);

    let active = manager.active_audio().unwrap();
    assert_eq!(active.num_frames(), 44100);

    let ai_result = AudioBuffer::sine_wave(880.0, 1.0, 44100);
    manager.set_ai_state(ai_result);
    assert!(manager.ai_state().is_some());
}

// === Full Pipeline Tests ===

#[test]
fn test_full_pipeline_load_process_export() {
    let dir = tempdir().unwrap();
    let input_path = dir.path().join("input.wav");
    let output_path = dir.path().join("output.wav");

    let source = AudioBuffer::sine_wave(440.0, 2.0, 44100);
    save_wav(&source, &input_path).unwrap();

    let mut buffer = load_wav(&input_path).unwrap();
    let original_rms = calculate_rms_db(buffer.samples());

    let mut chain = DspChain::new();
    chain.add(Box::new(GainEffect::with_gain("gain-1", -6.0)));
    chain.process(&mut buffer).unwrap();

    let new_rms = calculate_rms_db(buffer.samples());
    assert!((new_rms - (original_rms - 6.0)).abs() < 0.2);

    save_wav(&buffer, &output_path).unwrap();

    let exported = load_wav(&output_path).unwrap();
    assert_eq!(exported.num_frames(), buffer.num_frames());
    assert!(buffer.is_approx_equal(&exported, 1e-6));
}

#[test]
fn test_pipeline_preserves_duration() {
    let source = AudioBuffer::sine_wave(440.0, 5.0, 44100);
    let original_duration = source.duration();
    let mut buffer = source.clone();

    let mut chain = DspChain::new();
    chain.add(Box::new(GainEffect::with_gain("gain-1", -3.0)));
    chain.add(Box::new(GainEffect::with_gain("gain-2", 3.0)));
    chain.process(&mut buffer).unwrap();

    assert!((buffer.duration() - original_duration).abs() < 0.1);
}

#[test]
fn test_pipeline_safety_checks() {
    let mut buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);

    let mut chain = DspChain::new();
    chain.add(Box::new(GainEffect::with_gain("gain-boost", 12.0)));
    chain.process(&mut buffer).unwrap();

    let analysis = AudioAnalysis::analyze(&buffer);
    assert!(analysis.peak_db > 0.0);
    assert!(analysis.clipped_samples > 0 || analysis.peak_linear >= 1.0);
}
