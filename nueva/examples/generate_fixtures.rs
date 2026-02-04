//! Test Fixture Generator
//!
//! Generates audio test fixtures for Nueva testing.
//! Run with: cargo run --example generate_fixtures

use nueva::audio::{AudioBuffer, save_wav};
use std::f32::consts::PI;
use std::path::Path;

fn main() {
    let fixtures_dir = Path::new("tests/fixtures");

    if !fixtures_dir.exists() {
        std::fs::create_dir_all(fixtures_dir).expect("Failed to create fixtures directory");
    }

    println!("Generating test fixtures in {:?}", fixtures_dir);

    // Sine waves at various frequencies
    for &freq in &[100.0, 440.0, 1000.0, 10000.0] {
        let name = format!("sine_{}hz.wav", freq as u32);
        let path = fixtures_dir.join(&name);
        let buffer = AudioBuffer::sine_wave(freq, 2.0, 44100);
        save_wav(&buffer, &path).expect(&format!("Failed to save {}", name));
        println!("  Created {}", name);
    }

    // White noise
    {
        let path = fixtures_dir.join("white_noise.wav");
        let buffer = generate_white_noise(2.0, 44100);
        save_wav(&buffer, &path).expect("Failed to save white_noise.wav");
        println!("  Created white_noise.wav");
    }

    // Silence
    {
        let path = fixtures_dir.join("silence.wav");
        let buffer = AudioBuffer::silence(2.0, 1, 44100);
        save_wav(&buffer, &path).expect("Failed to save silence.wav");
        println!("  Created silence.wav");
    }

    // Clipped audio
    {
        let path = fixtures_dir.join("clipped.wav");
        let buffer = generate_clipped_sine(440.0, 2.0, 44100, 2.0);
        save_wav(&buffer, &path).expect("Failed to save clipped.wav");
        println!("  Created clipped.wav");
    }

    // Quiet audio (-60dB)
    {
        let path = fixtures_dir.join("quiet.wav");
        let mut buffer = AudioBuffer::sine_wave(440.0, 2.0, 44100);
        buffer.apply_gain_db(-60.0);
        save_wav(&buffer, &path).expect("Failed to save quiet.wav");
        println!("  Created quiet.wav");
    }

    // DC offset
    {
        let path = fixtures_dir.join("dc_offset.wav");
        let buffer = generate_with_dc_offset(440.0, 2.0, 44100, 0.1);
        save_wav(&buffer, &path).expect("Failed to save dc_offset.wav");
        println!("  Created dc_offset.wav");
    }

    // Stereo test
    {
        let path = fixtures_dir.join("stereo_test.wav");
        let buffer = generate_stereo_test(440.0, 880.0, 2.0, 44100);
        save_wav(&buffer, &path).expect("Failed to save stereo_test.wav");
        println!("  Created stereo_test.wav");
    }

    // Chord (C major)
    {
        let path = fixtures_dir.join("chord_progression.wav");
        let buffer = generate_chord(4.0, 44100);
        save_wav(&buffer, &path).expect("Failed to save chord_progression.wav");
        println!("  Created chord_progression.wav");
    }

    println!("\nDone! All fixtures generated.");
}

fn generate_white_noise(duration: f32, sample_rate: u32) -> AudioBuffer {
    let num_samples = (duration * sample_rate as f32) as usize;
    let mut samples = Vec::with_capacity(num_samples);

    // Simple LCG random number generator
    let mut seed: u64 = 12345;
    for _ in 0..num_samples {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let rand = ((seed >> 16) & 0x7fff) as f32 / 16383.0 - 1.0;
        samples.push(rand * 0.5); // Reduce amplitude to avoid clipping
    }

    AudioBuffer::new(samples, 1, sample_rate).unwrap()
}

fn generate_clipped_sine(freq: f32, duration: f32, sample_rate: u32, overdrive: f32) -> AudioBuffer {
    let num_samples = (duration * sample_rate as f32) as usize;
    let mut samples = Vec::with_capacity(num_samples);

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample = (2.0 * PI * freq * t).sin() * overdrive;
        samples.push(sample.clamp(-1.0, 1.0));
    }

    AudioBuffer::new(samples, 1, sample_rate).unwrap()
}

fn generate_with_dc_offset(freq: f32, duration: f32, sample_rate: u32, offset: f32) -> AudioBuffer {
    let num_samples = (duration * sample_rate as f32) as usize;
    let mut samples = Vec::with_capacity(num_samples);

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample = (2.0 * PI * freq * t).sin() * 0.5 + offset;
        samples.push(sample);
    }

    AudioBuffer::new(samples, 1, sample_rate).unwrap()
}

fn generate_stereo_test(freq_l: f32, freq_r: f32, duration: f32, sample_rate: u32) -> AudioBuffer {
    let num_frames = (duration * sample_rate as f32) as usize;
    let mut samples = Vec::with_capacity(num_frames * 2);

    for i in 0..num_frames {
        let t = i as f32 / sample_rate as f32;
        let left = (2.0 * PI * freq_l * t).sin() * 0.7;
        let right = (2.0 * PI * freq_r * t).sin() * 0.7;
        samples.push(left);
        samples.push(right);
    }

    AudioBuffer::new(samples, 2, sample_rate).unwrap()
}

fn generate_chord(duration: f32, sample_rate: u32) -> AudioBuffer {
    // C major chord: C3 (130.81 Hz), E3 (164.81 Hz), G3 (196.00 Hz)
    let frequencies = [130.81, 164.81, 196.00];
    let num_samples = (duration * sample_rate as f32) as usize;
    let mut samples = Vec::with_capacity(num_samples);

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let mut sample = 0.0;
        for &freq in &frequencies {
            sample += (2.0 * PI * freq * t).sin();
        }
        // Normalize and apply envelope
        sample /= frequencies.len() as f32;
        let envelope = if t < 0.1 {
            t / 0.1 // Attack
        } else if t > duration - 0.5 {
            (duration - t) / 0.5 // Release
        } else {
            1.0
        };
        samples.push(sample * envelope * 0.7);
    }

    AudioBuffer::new(samples, 1, sample_rate).unwrap()
}
