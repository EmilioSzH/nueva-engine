//! Full Pipeline Integration Tests
//!
//! End-to-end tests for the complete audio processing pipeline.

use nueva::audio::{AudioBuffer, load_wav, save_wav};
use nueva::audio::verification::{AudioAnalysis, calculate_rms_db};
use nueva::dsp::chain::DspChain;
use nueva::dsp::effects::GainEffect;
use nueva::dsp::Effect;
use tempfile::tempdir;

#[test]
fn test_full_pipeline_load_process_export() {
    let dir = tempdir().unwrap();
    let input_path = dir.path().join("input.wav");
    let output_path = dir.path().join("output.wav");

    // Create test audio and save
    let source = AudioBuffer::sine_wave(440.0, 2.0, 44100);
    save_wav(&source, &input_path).unwrap();

    // Load audio
    let mut buffer = load_wav(&input_path).unwrap();
    let original_rms = calculate_rms_db(buffer.samples());

    // Process through DSP chain
    let mut chain = DspChain::new();
    chain.add(Box::new(GainEffect::with_gain("gain-1", -6.0)));
    chain.process(&mut buffer).unwrap();

    // Verify processing
    let new_rms = calculate_rms_db(buffer.samples());
    assert!((new_rms - (original_rms - 6.0)).abs() < 0.2);

    // Export
    save_wav(&buffer, &output_path).unwrap();

    // Verify exported file
    let exported = load_wav(&output_path).unwrap();
    assert_eq!(exported.num_frames(), buffer.num_frames());
    assert!(buffer.is_approx_equal(&exported, 1e-6));
}

#[test]
fn test_pipeline_preserves_duration() {
    let source = AudioBuffer::sine_wave(440.0, 5.0, 44100);
    let original_duration = source.duration();

    let mut buffer = source.clone();

    // Apply multiple effects
    let mut chain = DspChain::new();
    chain.add(Box::new(GainEffect::with_gain("gain-1", -3.0)));
    chain.add(Box::new(GainEffect::with_gain("gain-2", 3.0)));
    chain.process(&mut buffer).unwrap();

    // Duration must be preserved within 0.1s (per spec)
    assert!((buffer.duration() - original_duration).abs() < 0.1);
}

#[test]
fn test_pipeline_safety_checks() {
    let mut buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);

    // Apply excessive gain
    let mut chain = DspChain::new();
    chain.add(Box::new(GainEffect::with_gain("gain-boost", 12.0)));
    chain.process(&mut buffer).unwrap();

    // Check for clipping
    let analysis = AudioAnalysis::analyze(&buffer);

    // Peak should exceed 0 dBFS
    assert!(analysis.peak_db > 0.0);
    // Should have clipped samples
    assert!(analysis.clipped_samples > 0 || analysis.peak_linear >= 1.0);
}
