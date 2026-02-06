# ACE-Step 1.5 Integration Guide

This document describes how to set up and use ACE-Step 1.5 with Nueva.

## Overview

ACE-Step 1.5 is a powerful open-source music generation model that provides:
- **Text-to-music transformation**: Transform audio based on text descriptions
- **Cover generation**: Create covers in different styles
- **Style transfer**: Change the overall style of audio
- **Track extraction**: Extract stems/tracks from mixed audio
- **Audio completion**: Complete partial audio

## Requirements

- **Python**: 3.11 or higher
- **GPU**: CUDA-capable GPU recommended (works on CPU but slower)
- **VRAM**: 4GB minimum (works with less via quantization)
- **Disk**: ~10GB for model weights

## Installation

### Quick Install (Windows)

```powershell
.\scripts\install-ace-step.ps1
```

### Quick Install (macOS/Linux)

```bash
chmod +x ./scripts/install-ace-step.sh
./scripts/install-ace-step.sh
```

### Manual Installation

1. **Clone ACE-Step repository**:
   ```bash
   git clone https://github.com/ACE-Step/ACE-Step-1.5.git ~/ACE-Step-1.5
   cd ~/ACE-Step-1.5
   ```

2. **Install uv package manager** (if not already installed):
   ```bash
   # macOS/Linux
   curl -LsSf https://astral.sh/uv/install.sh | sh

   # Windows (PowerShell)
   powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
   ```

3. **Set up environment**:
   ```bash
   uv sync
   ```

4. **Download models**:
   ```bash
   uv run acestep-download
   ```

5. **Install Nueva Python bridge**:
   ```bash
   cd /path/to/nueva-engine/python
   pip install -e .
   ```

## Configuration

Set the following environment variables:

```bash
# Path to ACE-Step installation
export NUEVA_ACE_STEP_PATH=~/ACE-Step-1.5

# Python executable (optional, defaults to 'python')
export NUEVA_PYTHON_PATH=python3

# ACE-Step API URL (if using API server)
export NUEVA_ACE_STEP_API=http://localhost:8001
```

## Usage

### Option 1: API Server (Recommended)

Start the ACE-Step API server:
```bash
cd ~/ACE-Step-1.5
uv run acestep-api
```

The API runs at `http://localhost:8001`.

### Option 2: Direct Integration

If ACE-Step is installed locally and `NUEVA_ACE_STEP_PATH` is set, Nueva will load the model directly.

### DAW Workflow

1. **Import audio** into Nueva project
2. **Invoke AI agent** with a transformation prompt:
   - "Make this sound like a jazz version"
   - "Transform to lo-fi hip hop style"
   - "Create an 80s synth-pop cover"
   - "Add orchestral layers"
3. **Agent processes** using ACE-Step (Layer 1 regenerated)
4. **Preview result** and adjust DSP effects (Layer 2)
5. **Bake** when satisfied

### Example Prompts

| Prompt | ACE-Step Mode | Description |
|--------|---------------|-------------|
| "jazz version of this song" | cover | Creates a jazz-style cover |
| "add vinyl warmth and tape hiss" | transform | Adds vintage character |
| "orchestral arrangement" | repaint | Transforms to orchestral |
| "extract the vocals" | extract | Stem separation |
| "complete this melody" | complete | Audio completion |
| "80s synthwave style" | style_change | Genre transformation |

### Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `mode` | enum | transform | Processing mode |
| `prompt` | string | required | Text description |
| `intensity` | 0.0-1.0 | 0.7 | Transformation strength |
| `preserve_melody` | bool | true | Keep original melody |
| `inference_steps` | 4-50 | 8 | Quality vs speed tradeoff |
| `guidance_scale` | 1.0-10.0 | 3.0 | Prompt adherence |

## Intentional Artifacts

ACE-Step may introduce intentional artifacts based on the prompt. Nueva's context tracker prevents the DSP chain from "correcting" these:

| Prompt Keywords | Artifacts | DSP Warning |
|-----------------|-----------|-------------|
| "vinyl", "record" | crackle, noise | Don't add noise gate |
| "tape", "analog" | hiss, saturation | Don't remove hiss |
| "lo-fi", "lofi" | bitcrushing, noise | Don't enhance clarity |
| "vintage", "retro" | frequency rolloff | Don't boost highs |

## Troubleshooting

### "ACE-Step not installed"
Run the installation script or set `NUEVA_ACE_STEP_PATH` correctly.

### "Cannot connect to ACE-Step API"
Start the API server: `cd ~/ACE-Step-1.5 && uv run acestep-api`

### "Out of VRAM"
- Reduce `inference_steps` (try 4)
- Enable INT8 quantization in ACE-Step config
- Use CPU mode (slower but works)

### "Processing too slow"
- Ensure GPU is being used (check CUDA availability)
- Reduce `inference_steps`
- Use the API server for persistent model loading

## API Reference

### Python Bridge

```python
from nueva import AceStepProcessor

processor = AceStepProcessor()
result = processor.process(
    input_path=Path("input.wav"),
    output_path=Path("output.wav"),
    prompt="jazz version",
    params={"mode": "cover", "intensity": 0.8}
)
```

### Rust Interface

```rust
use nueva::neural::{AceStep, NeuralModel, NeuralModelParams};

let model = AceStep::new();
let params = NeuralModelParams::new()
    .with_param("mode", "cover")
    .with_param("prompt", "jazz version")
    .with_param("intensity", 0.8);

let result = model.process(input_path, output_path, &params)?;
```

## Resources

- [ACE-Step GitHub](https://github.com/ace-step/ACE-Step-1.5)
- [ACE-Step Documentation](https://ace-step.github.io/ace-step-v1.5.github.io/)
- [Hugging Face Models](https://huggingface.co/ACE-Step/Ace-Step1.5)
- [Nueva Spec §5.3](../NUEVA_IMPLEMENTATION%20(3).md) - Neural Model Registry
