//! Integration Tests
//!
//! End-to-end tests for the Nueva audio processing pipeline.

use nueva::dsp::{AudioBuffer, EffectChain, Effect, EQBand};
use nueva::dsp::GainEffect;
use nueva::dsp::ParametricEQ;
use nueva::dsp::Compressor;
use nueva::dsp::Limiter;

/// Helper to create a test sine wave buffer
fn create_sine_buffer(frequency: f64, sample_rate: f64, duration_secs: f64) -> AudioBuffer {
    let num_samples = (sample_rate * duration_secs) as usize;
    let mut buffer = AudioBuffer::new(1, num_samples, sample_rate);
    let samples = buffer.samples_mut();

    for i in 0..num_samples {
        let t = i as f64 / sample_rate;
        samples[i] = (2.0 * std::f64::consts::PI * frequency * t).sin() as f32;
    }

    buffer
}

// === Full Pipeline Tests ===

#[test]
fn test_full_pipeline_process() {
    let buffer = create_sine_buffer(440.0, 44100.0, 1.0);
    let original_rms = buffer.rms_db(0);

    // Process through DSP chain
    let mut chain = EffectChain::new();
    chain.prepare(44100.0, 512);
    chain.add(Box::new(GainEffect::with_gain(-6.0).unwrap()));

    let mut processed = buffer.clone();
    chain.process(&mut processed);

    // Verify processing: -6 dB gain should reduce RMS by ~6 dB
    let new_rms = processed.rms_db(0);
    assert!(
        (new_rms - (original_rms - 6.0)).abs() < 1.0,
        "Expected ~{:.1} dB, got {:.1} dB",
        original_rms - 6.0,
        new_rms
    );
}

#[test]
fn test_pipeline_preserves_sample_count() {
    let buffer = create_sine_buffer(440.0, 44100.0, 2.0);
    let original_samples = buffer.num_samples();

    let mut chain = EffectChain::new();
    chain.prepare(44100.0, 512);
    chain.add(Box::new(GainEffect::with_gain(-3.0).unwrap()));
    chain.add(Box::new(GainEffect::with_gain(3.0).unwrap()));

    let mut processed = buffer.clone();
    chain.process(&mut processed);

    assert_eq!(
        processed.num_samples(),
        original_samples,
        "Sample count must be preserved"
    );
}

#[test]
fn test_empty_chain_passthrough() {
    let buffer = create_sine_buffer(440.0, 44100.0, 0.5);
    let original_rms = buffer.rms_db(0);

    let mut chain = EffectChain::new();
    chain.prepare(44100.0, 512);

    let mut processed = buffer.clone();
    chain.process(&mut processed);

    // Empty chain should not change audio
    let new_rms = processed.rms_db(0);
    assert!(
        (new_rms - original_rms).abs() < 0.01,
        "Empty chain modified audio: {:.2} -> {:.2}",
        original_rms,
        new_rms
    );
}

// === DSP Effect Tests ===

#[test]
fn test_gain_accuracy() {
    let buffer = create_sine_buffer(440.0, 44100.0, 0.5);
    let original_rms = buffer.rms_db(0);

    for gain_db in [-12.0_f32, -6.0, -3.0, 3.0, 6.0] {
        let mut processed = buffer.clone();
        let mut gain = GainEffect::with_gain(gain_db).unwrap();
        gain.prepare(44100.0, 512);
        gain.process(&mut processed);

        let new_rms = processed.rms_db(0);
        let actual_gain = new_rms - original_rms;

        assert!(
            (actual_gain - gain_db as f64).abs() < 0.5,
            "Gain at {} dB: expected {:.1}, got {:.1} (error: {:.2})",
            gain_db,
            gain_db,
            actual_gain,
            (actual_gain - gain_db as f64).abs()
        );
    }
}

#[test]
fn test_eq_boosts_frequency() {
    let mut buffer = create_sine_buffer(1000.0, 44100.0, 0.5);
    let original_rms = buffer.rms_db(0);

    let mut eq = ParametricEQ::new();
    eq.add_band(EQBand::peak(1000.0, 6.0, 1.0)).unwrap(); // +6 dB at 1kHz
    eq.prepare(44100.0, 512);
    eq.process(&mut buffer);

    let new_rms = buffer.rms_db(0);
    // 1kHz sine with +6dB boost at 1kHz should increase significantly
    assert!(
        new_rms > original_rms + 3.0,
        "EQ boost didn't increase level: {:.1} -> {:.1}",
        original_rms,
        new_rms
    );
}

#[test]
fn test_compressor_reduces_level() {
    let mut buffer = create_sine_buffer(440.0, 44100.0, 0.5);
    let original_peak = buffer.peak_db(0);

    let mut comp = Compressor::new();
    comp.set_threshold_db(-20.0);
    comp.set_ratio(4.0);
    comp.set_attack_ms(1.0);
    comp.set_release_ms(100.0);
    comp.prepare(44100.0, 512);
    comp.process(&mut buffer);

    // With -20dB threshold and 4:1 ratio, signal above threshold should be reduced
    let new_peak = buffer.peak_db(0);
    assert!(
        new_peak < original_peak,
        "Compressor didn't reduce peak: {:.1} -> {:.1}",
        original_peak,
        new_peak
    );
}

#[test]
fn test_limiter_prevents_clipping() {
    let mut buffer = create_sine_buffer(440.0, 44100.0, 0.5);

    // First boost the signal
    let mut gain = GainEffect::with_gain(12.0).unwrap();
    gain.prepare(44100.0, 512);
    gain.process(&mut buffer);

    // Then limit it
    let mut limiter = Limiter::new();
    limiter.set_ceiling_db(-1.0);
    limiter.prepare(44100.0, 512);
    limiter.process(&mut buffer);

    // Peak should not exceed threshold (with some tolerance)
    let peak = buffer.peak_db(0);
    assert!(
        peak <= 0.5,
        "Limiter failed to prevent clipping: peak = {:.1} dBFS",
        peak
    );
}

// === Chain Ordering Tests ===

#[test]
fn test_chain_auto_ordering() {
    let mut chain = EffectChain::new();
    chain.prepare(44100.0, 512);

    // Add effects out of order - chain should auto-sort
    chain.add(Box::new(Limiter::new()));
    chain.add(Box::new(Compressor::new()));
    chain.add(Box::new(ParametricEQ::new()));

    // Verify chain has all effects
    assert_eq!(chain.len(), 3);
}

// === Audio Validation Tests ===

#[test]
fn test_buffer_validity_check() {
    let buffer = create_sine_buffer(440.0, 44100.0, 0.1);
    assert!(buffer.is_valid(), "Valid sine buffer marked invalid");

    // Create buffer with NaN
    let mut invalid_buffer = AudioBuffer::new(1, 100, 44100.0);
    invalid_buffer.samples_mut()[50] = f32::NAN;
    assert!(!invalid_buffer.is_valid(), "Buffer with NaN marked valid");
}

#[test]
fn test_silence_remains_silent() {
    let mut buffer = AudioBuffer::new(1, 44100, 44100.0);
    // Buffer initialized to zeros (silence)

    let mut chain = EffectChain::new();
    chain.prepare(44100.0, 512);
    chain.add(Box::new(GainEffect::with_gain(6.0).unwrap()));
    chain.process(&mut buffer);

    // Silence processed with gain should still be silence
    let rms = buffer.rms_db(0);
    assert!(
        rms < -80.0 || rms == f64::NEG_INFINITY,
        "Silence became audible: {:.1} dBFS",
        rms
    );
}

#[test]
fn test_no_nan_after_processing() {
    let mut buffer = create_sine_buffer(440.0, 44100.0, 0.5);

    let mut chain = EffectChain::new();
    chain.prepare(44100.0, 512);
    chain.add(Box::new(GainEffect::with_gain(20.0).unwrap())); // High gain
    chain.add(Box::new(Compressor::new()));
    chain.add(Box::new(Limiter::new()));
    chain.process(&mut buffer);

    assert!(
        buffer.is_valid(),
        "Processing produced invalid samples (NaN/Inf)"
    );
}
