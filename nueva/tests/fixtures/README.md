# Test Fixtures

This directory contains audio test files for Nueva testing.

## Generation

Run `generate_fixtures.sh` (requires sox) or use Rust-based generation:

```bash
cargo run --example generate_fixtures
```

## Fixtures

| File | Description | Use Case |
|------|-------------|----------|
| sine_100hz.wav | 100 Hz sine wave | Low frequency testing |
| sine_440hz.wav | 440 Hz sine wave (A4) | Standard reference tone |
| sine_1000hz.wav | 1 kHz sine wave | Mid frequency, EQ testing |
| sine_10000hz.wav | 10 kHz sine wave | High frequency testing |
| white_noise.wav | White noise | Broadband testing |
| pink_noise.wav | Pink noise | Musical/mixing testing |
| silence.wav | Digital silence | Noise floor testing |
| clipped.wav | Clipped sine wave | Clipping detection testing |
| quiet.wav | Very quiet audio (-60dB) | Low level handling |
| dc_offset.wav | Audio with DC offset | DC detection testing |
| stereo_test.wav | Different L/R content | Stereo correlation testing |
| chord_progression.wav | Musical content | Compression/dynamics testing |

## Verification

All fixtures can be verified with:

```bash
cargo test --test fixtures_verify
```
