//! Audio Quality Tests
//!
//! Objective measurements for audio processing quality.

use nueva::audio::AudioBuffer;
use nueva::audio::verification::{calculate_rms, calculate_dc_offset, calculate_rms_db, linear_to_db};
use nueva::dsp::chain::DspChain;
use nueva::dsp::effects::GainEffect;
use nueva::dsp::Effect;

// === Passthrough Tests ===

#[test]
fn test_passthrough_is_bit_perfect() {
    let input = AudioBuffer::sine_wave(440.0, 1.0, 44100);
    let mut output = input.clone();

    let mut empty_chain = DspChain::new();
    empty_chain.process(&mut output).unwrap();

    assert!(input.is_identical_to(&output), "Empty chain must not modify audio");
}

#[test]
fn test_passthrough_various_sample_rates() {
    for sample_rate in [44100, 48000, 96000] {
        let input = AudioBuffer::sine_wave(440.0, 0.5, sample_rate);
        let mut output = input.clone();

        let mut empty_chain = DspChain::new();
        empty_chain.process(&mut output).unwrap();

        assert!(
            input.is_identical_to(&output),
            "Passthrough failed at {} Hz",
            sample_rate
        );
    }
}

// === Effect Quality Tests ===

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

    assert!(input.is_approx_equal(&buffer, 1e-6));
}

// === Artifact Detection Tests ===

#[test]
fn test_no_dc_offset_introduced() {
    let input = AudioBuffer::sine_wave(440.0, 1.0, 44100);
    let input_dc = calculate_dc_offset(input.samples());
    assert!(input_dc.abs() < 0.001);

    let mut buffer = input.clone();
    let mut chain = DspChain::new();
    chain.add(Box::new(GainEffect::with_gain("gain-1", -6.0)));
    chain.process(&mut buffer).unwrap();

    let output_dc = calculate_dc_offset(buffer.samples());
    assert!(output_dc.abs() < 0.01, "Processing introduced DC offset: {}", output_dc);
}

#[test]
fn test_silence_remains_silence() {
    let mut silence = AudioBuffer::silence(1.0, 1, 44100);

    let mut chain = DspChain::new();
    chain.add(Box::new(GainEffect::with_gain("gain-1", 6.0)));
    chain.process(&mut silence).unwrap();

    let rms = calculate_rms(silence.samples());
    let rms_db = linear_to_db(rms);
    assert!(rms_db < -80.0 || rms == 0.0, "Silence processing added noise: {} dBFS", rms_db);
}

#[test]
fn test_no_inf_or_nan_values() {
    let mut buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);

    let mut chain = DspChain::new();
    chain.add(Box::new(GainEffect::with_gain("extreme-gain", 100.0)));
    chain.process(&mut buffer).unwrap();

    for &sample in buffer.samples() {
        assert!(sample.is_finite(), "Processing produced non-finite value: {}", sample);
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

    assert_eq!(buffer.samples().len(), original_count, "Sample count changed during processing");
}

// === Stub tests for effects not yet implemented ===

#[test]
#[ignore = "EQ not yet implemented by wt-dsp"]
fn test_eq_doesnt_introduce_noise() {
    todo!("Implement when EQ effect is available");
}

#[test]
#[ignore = "Compressor not yet implemented by wt-dsp"]
fn test_compressor_reduces_dynamic_range() {
    todo!("Implement when Compressor effect is available");
}

#[test]
#[ignore = "Limiter not yet implemented by wt-dsp"]
fn test_limiter_prevents_clipping() {
    todo!("Implement when Limiter effect is available");
}
