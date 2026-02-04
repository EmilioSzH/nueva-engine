//! DSP Benchmarks
//!
//! Performance benchmarks for audio processing operations.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nueva::audio::AudioBuffer;
use nueva::dsp::effects::GainEffect;
use nueva::dsp::chain::DspChain;
use nueva::dsp::Effect;

fn benchmark_gain_processing(c: &mut Criterion) {
    let mut buffer = AudioBuffer::sine_wave(440.0, 10.0, 44100);

    c.bench_function("gain_10s_mono", |b| {
        b.iter(|| {
            let mut gain = GainEffect::with_gain("bench-gain", -6.0);
            gain.process(black_box(&mut buffer)).unwrap();
        })
    });
}

fn benchmark_dsp_chain(c: &mut Criterion) {
    let mut buffer = AudioBuffer::sine_wave(440.0, 10.0, 44100);

    let mut chain = DspChain::new();
    chain.add(Box::new(GainEffect::with_gain("gain-1", -3.0)));
    chain.add(Box::new(GainEffect::with_gain("gain-2", -3.0)));
    chain.add(Box::new(GainEffect::with_gain("gain-3", -3.0)));

    c.bench_function("chain_3_effects_10s", |b| {
        b.iter(|| {
            chain.process(black_box(&mut buffer)).unwrap();
        })
    });
}

criterion_group!(benches, benchmark_gain_processing, benchmark_dsp_chain);
criterion_main!(benches);
