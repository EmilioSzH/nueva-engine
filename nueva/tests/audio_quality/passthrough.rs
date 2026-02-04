//! Passthrough Tests
//!
//! Verify that empty processing chains don't modify audio.

use nueva::audio::AudioBuffer;
use nueva::dsp::chain::DspChain;

#[test]
fn test_passthrough_is_bit_perfect() {
    let input = AudioBuffer::sine_wave(440.0, 1.0, 44100);
    let mut output = input.clone();

    let mut empty_chain = DspChain::new();
    empty_chain.process(&mut output).unwrap();

    assert!(input.is_identical_to(&output), "Empty chain must not modify audio");
}

#[test]
fn test_passthrough_stereo() {
    // Create stereo buffer
    let samples = vec![0.5, -0.5; 44100]; // Alternating L/R
    let input = AudioBuffer::new(samples, 2, 44100).unwrap();
    let mut output = input.clone();

    let mut empty_chain = DspChain::new();
    empty_chain.process(&mut output).unwrap();

    assert!(input.is_identical_to(&output));
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
