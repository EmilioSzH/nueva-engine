"""
ACE-Step 1.5 Integration for Nueva

This module provides the interface to the ACE-Step 1.5 music generation model.
It handles model loading, inference, and audio processing.

ACE-Step 1.5 capabilities per spec §5.3:
- text_to_music: Generate music from text description
- cover: Create covers in different styles
- repaint: Modify specific aspects of audio
- style_change: Change the overall style
- track_extraction: Extract stems/tracks
- layering: Add layers to existing audio
- completion: Complete partial audio
"""

import os
import sys
import json
import subprocess
import tempfile
from pathlib import Path
from typing import Any, Optional
import shutil


class AceStepProcessor:
    """ACE-Step 1.5 processor for Nueva."""

    # ACE-Step installation path (configurable via env var)
    ACE_STEP_PATH = os.environ.get(
        "NUEVA_ACE_STEP_PATH",
        str(Path.home() / "ACE-Step-1.5")
    )

    # Default model parameters
    DEFAULT_PARAMS = {
        "mode": "transform",
        "preserve_melody": True,
        "intensity": 0.7,
        "inference_steps": 8,
        "guidance_scale": 3.0,
    }

    # Mode mappings
    MODES = {
        "transform": "Transform audio based on prompt",
        "repaint": "Repaint/modify specific aspects",
        "cover": "Create a cover version",
        "extract": "Extract stems/tracks",
        "layer": "Add layers to audio",
        "complete": "Complete partial audio",
    }

    def __init__(self):
        self._model_loaded = False
        self._pipeline = None
        self._check_installation()

    def _check_installation(self):
        """Check if ACE-Step is installed."""
        ace_path = Path(self.ACE_STEP_PATH)
        self._installed = ace_path.exists() and (ace_path / "acestep").exists()

        if not self._installed:
            print(
                f"Warning: ACE-Step not found at {self.ACE_STEP_PATH}. "
                f"Install with: git clone https://github.com/ACE-Step/ACE-Step-1.5.git",
                file=sys.stderr
            )

    def is_available(self) -> bool:
        """Check if the model is available for use."""
        return self._installed

    def get_info(self) -> dict:
        """Get model information."""
        return {
            "id": "ace-step",
            "name": "ACE-Step 1.5",
            "version": "1.5",
            "description": "Full music transformation via Hybrid Reasoning-Diffusion",
            "capabilities": list(self.MODES.keys()),
            "use_when": [
                "Dramatic transformation",
                "Genre change",
                "Cover generation",
                "Reimagine as X",
                "Style transfer",
            ],
            "limitations": [
                "Takes several seconds to process",
                "Non-deterministic results",
                "Requires GPU for best performance",
            ],
            "known_artifacts": [
                "Vocal intelligibility loss on complex lyrics",
                "Tempo drift on pieces >5 minutes",
                "Transient softening on aggressive percussion",
            ],
            "vram_requirement_gb": 4.0,
            "inference_time": "1-30 seconds depending on GPU",
            "installed": self._installed,
            "install_path": self.ACE_STEP_PATH,
            "supported_params": [
                {
                    "name": "mode",
                    "type": "enum",
                    "options": list(self.MODES.keys()),
                    "default": "transform",
                    "description": "Processing mode",
                },
                {
                    "name": "prompt",
                    "type": "string",
                    "required": True,
                    "description": "Text description of desired output",
                },
                {
                    "name": "preserve_melody",
                    "type": "bool",
                    "default": True,
                    "description": "Whether to preserve the original melody",
                },
                {
                    "name": "intensity",
                    "type": "float",
                    "min": 0.0,
                    "max": 1.0,
                    "default": 0.7,
                    "description": "Transformation intensity",
                },
                {
                    "name": "inference_steps",
                    "type": "int",
                    "min": 4,
                    "max": 50,
                    "default": 8,
                    "description": "Number of diffusion steps (more = better quality, slower)",
                },
                {
                    "name": "guidance_scale",
                    "type": "float",
                    "min": 1.0,
                    "max": 10.0,
                    "default": 3.0,
                    "description": "How closely to follow the prompt",
                },
            ],
        }

    def _load_model(self):
        """Load the ACE-Step model (lazy loading)."""
        if self._model_loaded:
            return

        if not self._installed:
            raise RuntimeError(
                f"ACE-Step not installed. Please install from: "
                f"https://github.com/ACE-Step/ACE-Step-1.5"
            )

        # Import ACE-Step modules
        sys.path.insert(0, self.ACE_STEP_PATH)

        try:
            # Try to import the handler
            from acestep.acestep_v15_pipeline import AceStepHandler

            # Initialize handler (this loads the model)
            self._pipeline = AceStepHandler()
            self._model_loaded = True
            print("ACE-Step 1.5 model loaded successfully", file=sys.stderr)

        except ImportError as e:
            raise RuntimeError(f"Failed to import ACE-Step: {e}")
        except Exception as e:
            raise RuntimeError(f"Failed to load ACE-Step model: {e}")

    def process(
        self,
        input_path: Path,
        output_path: Path,
        prompt: Optional[str] = None,
        params: Optional[dict] = None,
    ) -> dict:
        """
        Process audio through ACE-Step.

        Args:
            input_path: Path to input audio file
            output_path: Path where output should be written
            prompt: Text description of desired transformation
            params: Additional parameters (mode, intensity, etc.)

        Returns:
            dict with processing results
        """
        if not self._installed:
            return self._process_via_api(input_path, output_path, prompt, params)

        # Merge with defaults
        params = {**self.DEFAULT_PARAMS, **(params or {})}

        if not prompt:
            prompt = params.get("prompt", "transform the audio")

        # Load model if needed
        self._load_model()

        mode = params.get("mode", "transform")
        intensity = params.get("intensity", 0.7)
        preserve_melody = params.get("preserve_melody", True)
        inference_steps = params.get("inference_steps", 8)
        guidance_scale = params.get("guidance_scale", 3.0)

        # Map our modes to ACE-Step task_types
        task_type_map = {
            "transform": "repaint",
            "repaint": "repaint",
            "cover": "cover",
            "extract": "repaint",
            "layer": "repaint",
            "complete": "repaint",
        }
        task_type = task_type_map.get(mode, "repaint")

        try:
            # Initialize service if needed
            if not hasattr(self._pipeline, '_initialized'):
                self._pipeline.initialize_service()
                self._pipeline._initialized = True

            # Run ACE-Step processing
            result = self._pipeline.generate_music(
                captions=prompt,
                lyrics="",
                src_audio=str(input_path),
                task_type=task_type,
                inference_steps=inference_steps,
                guidance_scale=guidance_scale,
                audio_cover_strength=intensity,
            )

            # Save output audio
            if result and "audio" in result:
                import soundfile as sf
                sf.write(str(output_path), result["audio"], result.get("sample_rate", 44100))

            # Determine intentional artifacts based on mode and prompt
            artifacts = self._detect_artifacts(mode, prompt, params)

            return {
                "success": True,
                "message": f"ACE-Step {mode}: '{prompt}' at {intensity*100:.0f}% intensity",
                "reasoning": f"Applied {mode} transformation using ACE-Step 1.5",
                "intentional_artifacts": artifacts,
                "metadata": {
                    "mode": mode,
                    "prompt": prompt,
                    "intensity": intensity,
                    "preserve_melody": preserve_melody,
                    "inference_steps": inference_steps,
                },
            }

        except Exception as e:
            raise RuntimeError(f"ACE-Step processing failed: {e}")

    def _process_via_api(
        self,
        input_path: Path,
        output_path: Path,
        prompt: Optional[str],
        params: Optional[dict],
    ) -> dict:
        """
        Process via ACE-Step API server if model not loaded locally.

        This allows running ACE-Step as a separate service.
        """
        import urllib.request
        import urllib.error

        api_url = os.environ.get("NUEVA_ACE_STEP_API", "http://localhost:8001")

        params = {**self.DEFAULT_PARAMS, **(params or {})}
        prompt = prompt or params.get("prompt", "transform the audio")

        # Prepare API request
        request_data = {
            "audio_path": str(input_path),
            "prompt": prompt,
            "output_path": str(output_path),
            "mode": params.get("mode", "transform"),
            "strength": params.get("intensity", 0.7),
            "preserve_melody": params.get("preserve_melody", True),
            "num_inference_steps": params.get("inference_steps", 8),
            "guidance_scale": params.get("guidance_scale", 3.0),
        }

        try:
            req = urllib.request.Request(
                f"{api_url}/process",
                data=json.dumps(request_data).encode("utf-8"),
                headers={"Content-Type": "application/json"},
                method="POST",
            )

            with urllib.request.urlopen(req, timeout=600) as response:
                result = json.loads(response.read().decode("utf-8"))

            if not result.get("success", False):
                raise RuntimeError(result.get("error", "Unknown API error"))

            mode = params.get("mode", "transform")
            artifacts = self._detect_artifacts(mode, prompt, params)

            return {
                "success": True,
                "message": f"ACE-Step {mode} (via API): '{prompt}'",
                "reasoning": "Processed via ACE-Step API server",
                "intentional_artifacts": artifacts,
                "metadata": result.get("metadata", {}),
            }

        except urllib.error.URLError as e:
            raise RuntimeError(
                f"Cannot connect to ACE-Step API at {api_url}. "
                f"Start the API server with: cd {self.ACE_STEP_PATH} && uv run acestep-api"
            )

    def _detect_artifacts(self, mode: str, prompt: str, params: dict) -> list:
        """Detect intentional artifacts based on mode and prompt."""
        artifacts = []
        prompt_lower = prompt.lower()

        # Mode-specific artifacts
        if mode == "cover":
            artifacts.append("different_timbre")
            artifacts.append("style_variation")

        if mode == "repaint":
            artifacts.append("localized_changes")

        # Prompt-based artifacts
        if "vintage" in prompt_lower or "retro" in prompt_lower:
            artifacts.append("intentional_coloration")
            artifacts.append("frequency_rolloff")

        if "lo-fi" in prompt_lower or "lofi" in prompt_lower:
            artifacts.append("bitcrushing")
            artifacts.append("noise")
            artifacts.append("sample_rate_artifacts")

        if "vinyl" in prompt_lower:
            artifacts.append("high_frequency_noise")
            artifacts.append("subtle_crackle")

        if "tape" in prompt_lower:
            artifacts.append("subtle_hiss")
            artifacts.append("saturation")

        if "8-bit" in prompt_lower or "chiptune" in prompt_lower:
            artifacts.append("quantization")
            artifacts.append("limited_polyphony")

        return artifacts


def main():
    """CLI entry point for ACE-Step processor."""
    import argparse

    parser = argparse.ArgumentParser(description="ACE-Step 1.5 processor for Nueva")
    parser.add_argument("input", help="Input audio file")
    parser.add_argument("output", help="Output audio file")
    parser.add_argument("-p", "--prompt", required=True, help="Transformation prompt")
    parser.add_argument(
        "-m", "--mode",
        choices=["transform", "repaint", "cover", "extract", "layer", "complete"],
        default="transform",
        help="Processing mode"
    )
    parser.add_argument(
        "-i", "--intensity",
        type=float,
        default=0.7,
        help="Transformation intensity (0.0-1.0)"
    )
    parser.add_argument(
        "--preserve-melody",
        action="store_true",
        default=True,
        help="Preserve original melody"
    )
    parser.add_argument(
        "--steps",
        type=int,
        default=8,
        help="Inference steps (4-50)"
    )
    parser.add_argument(
        "--info",
        action="store_true",
        help="Print model info and exit"
    )

    args = parser.parse_args()

    processor = AceStepProcessor()

    if args.info:
        info = processor.get_info()
        print(json.dumps(info, indent=2))
        return

    if not processor.is_available():
        print(
            f"Error: ACE-Step not installed at {processor.ACE_STEP_PATH}",
            file=sys.stderr
        )
        print(
            "Install with: git clone https://github.com/ACE-Step/ACE-Step-1.5.git",
            file=sys.stderr
        )
        sys.exit(1)

    params = {
        "mode": args.mode,
        "intensity": args.intensity,
        "preserve_melody": args.preserve_melody,
        "inference_steps": args.steps,
    }

    try:
        result = processor.process(
            input_path=Path(args.input),
            output_path=Path(args.output),
            prompt=args.prompt,
            params=params,
        )
        print(json.dumps(result, indent=2))
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
