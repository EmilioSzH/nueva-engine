//! Artifact Detection Tests
//!
//! Tests to detect unwanted audio artifacts from processing.

use nueva::audio::AudioBuffer;
use nueva::audio::verification::{calculate_rms, calculate_dc_offset, linear_to_db};
use nueva::dsp::chain::DspChain;
use nueva::dsp::effects::GainEffect;
use nueva::dsp::Effect;

#[test]
fn test_no_dc_offset_introduced() {
    // Start with audio that has no DC offset
    let input = AudioBuffer::sine_wave(440.0, 1.0, 44100);
    let input_dc = calculate_dc_offset(input.samples());
    assert!(input_dc.abs() < 0.001, "Input should have no DC offset");

    // Process through chain
    let mut buffer = input.clone();
    let mut chain = DspChain::new();
    chain.add(Box::new(GainEffect::with_gain("gain-1", -6.0)));
    chain.process(&mut buffer).unwrap();

    // DC offset should not increase
    let output_dc = calculate_dc_offset(buffer.samples());
    assert!(
        output_dc.abs() < 0.01,
        "Processing introduced DC offset: {}",
        output_dc
    );
}

#[test]
fn test_silence_remains_silence() {
    let mut silence = AudioBuffer::silence(1.0, 1, 44100);

    // Process silence through effects
    let mut chain = DspChain::new();
    chain.add(Box::new(GainEffect::with_gain("gain-1", 6.0)));
    chain.process(&mut silence).unwrap();

    // Silence should remain silent (RMS < -80 dB)
    let rms = calculate_rms(silence.samples());
    let rms_db = linear_to_db(rms);
    assert!(
        rms_db < -80.0 || rms == 0.0,
        "Silence processing added noise: {} dBFS",
        rms_db
    );
}

#[test]
fn test_no_inf_or_nan_values() {
    let mut buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);

    // Apply extreme processing
    let mut chain = DspChain::new();
    chain.add(Box::new(GainEffect::with_gain("extreme-gain", 100.0)));
    chain.process(&mut buffer).unwrap();

    // Check for invalid values
    for &sample in buffer.samples() {
        assert!(
            sample.is_finite(),
            "Processing produced non-finite value: {}",
            sample
        );
    }
}

#[test]
fn test_sample_count_preserved() {
    let input = AudioBuffer::sine_wave(440.0, 2.5, 44100);
    let original_count = input.samples().len();

    let mut buffer = input.clone();
    let mut chain = DspChain::new();
    chain.add(Box::new(GainEffect::with_gain("gain", -3.0)));
    chain.process(&mut buffer).unwrap();

    assert_eq!(
        buffer.samples().len(),
        original_count,
        "Sample count changed during processing"
    );
}
