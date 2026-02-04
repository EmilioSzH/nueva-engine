#!/bin/bash
# generate_fixtures.sh - Create all test audio files using sox
#
# Requires: sox (Sound eXchange)
# Install: brew install sox (macOS) or apt install sox (Linux)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Generating test fixtures..."

# Sine waves at various frequencies
for freq in 100 440 1000 10000; do
    echo "  Creating sine_${freq}hz.wav"
    sox -n -r 44100 -c 1 "sine_${freq}hz.wav" synth 2 sine $freq
done

# White noise
echo "  Creating white_noise.wav"
sox -n -r 44100 -c 1 white_noise.wav synth 2 whitenoise

# Pink noise (more musical frequency distribution)
echo "  Creating pink_noise.wav"
sox -n -r 44100 -c 1 pink_noise.wav synth 2 pinknoise

# Silence
echo "  Creating silence.wav"
sox -n -r 44100 -c 1 silence.wav trim 0 2

# Clipped audio (over-driven sine wave)
echo "  Creating clipped.wav"
sox -n -r 44100 -c 1 clipped.wav synth 2 sine 440 gain 20

# Very quiet audio (-60dB)
echo "  Creating quiet.wav"
sox -n -r 44100 -c 1 quiet.wav synth 2 sine 440 gain -60

# DC offset
echo "  Creating dc_offset.wav"
sox -n -r 44100 -c 1 dc_offset.wav synth 2 sine 440 dcshift 0.1

# Stereo test (different frequencies L/R)
echo "  Creating stereo_test.wav"
sox -n -r 44100 -c 2 stereo_test.wav synth 2 sine 440 sine 880

# Simple chord progression (for dynamics testing)
echo "  Creating chord_progression.wav"
sox -n -r 44100 -c 1 chord_progression.wav \
    synth 4 pl C3 pl E3 pl G3 fade 0 4 0.5

echo "Done! Created $(ls -1 *.wav 2>/dev/null | wc -l) fixture files."
