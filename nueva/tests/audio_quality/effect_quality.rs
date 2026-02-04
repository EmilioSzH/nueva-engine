//! Effect Quality Tests
//!
//! Tests for DSP effect accuracy and quality.

use nueva::audio::AudioBuffer;
use nueva::audio::verification::{calculate_rms_db, calculate_crest_factor, linear_to_db};
use nueva::dsp::effects::GainEffect;
use nueva::dsp::Effect;

#[test]
fn test_gain_accuracy_within_0_1db() {
    let input = AudioBuffer::sine_wave(440.0, 1.0, 44100);
    let input_rms = calculate_rms_db(input.samples());

    for gain_db in [-12.0, -6.0, -3.0, 0.0, 3.0, 6.0, 12.0] {
        let mut buffer = input.clone();
        let mut gain = GainEffect::with_gain("test-gain", gain_db);
        gain.process(&mut buffer).unwrap();

        let output_rms = calculate_rms_db(buffer.samples());
        let actual_gain = output_rms - input_rms;

        assert!(
            (actual_gain - gain_db).abs() < 0.1,
            "Gain at {} dB was {} dB (error: {:.2} dB)",
            gain_db,
            actual_gain,
            (actual_gain - gain_db).abs()
        );
    }
}

#[test]
fn test_gain_unity_is_transparent() {
    let input = AudioBuffer::sine_wave(440.0, 1.0, 44100);
    let mut buffer = input.clone();

    let mut gain = GainEffect::with_gain("unity-gain", 0.0);
    gain.process(&mut buffer).unwrap();

    // 0 dB gain should leave audio unchanged
    assert!(input.is_approx_equal(&buffer, 1e-6));
}

// Stub tests for effects not yet implemented by wt-dsp worktree

#[test]
#[ignore = "EQ not yet implemented by wt-dsp"]
fn test_eq_doesnt_introduce_noise() {
    // EQ on silence should remain silence
    // RMS < -80 dBFS after processing
    todo!("Implement when EQ effect is available");
}

#[test]
#[ignore = "Compressor not yet implemented by wt-dsp"]
fn test_compressor_reduces_dynamic_range() {
    // Crest factor should decrease after compression
    todo!("Implement when Compressor effect is available");
}

#[test]
#[ignore = "Limiter not yet implemented by wt-dsp"]
fn test_limiter_prevents_clipping() {
    // Output peak must never exceed threshold
    todo!("Implement when Limiter effect is available");
}
