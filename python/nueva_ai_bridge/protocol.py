"""Protocol definitions for Nueva <-> ACE-Step communication.

This module defines the data structures for:
- Nueva requests (from Rust client)
- ACE-Step API requests (to upstream)
- Responses in both directions
"""

from enum import Enum
from typing import Any, Optional
from pydantic import BaseModel, Field


class AceStepMode(str, Enum):
    """ACE-Step processing modes mapped to Nueva modes."""

    TRANSFORM = "transform"  # text2music - generate from prompt
    COVER = "cover"  # change style, preserve structure
    REPAINT = "repaint"  # modify specific regions
    EXTRACT = "extract"  # source separation
    LAYER = "layer"  # add/remove instrument layers (lego)
    COMPLETE = "complete"  # add accompaniment


# Mapping from Nueva modes to ACE-Step task names
NUEVA_TO_ACESTEP_TASK = {
    AceStepMode.TRANSFORM: "text2music",
    AceStepMode.COVER: "cover",
    AceStepMode.REPAINT: "repaint",
    AceStepMode.EXTRACT: "extract",
    AceStepMode.LAYER: "lego",
    AceStepMode.COMPLETE: "complete",
}


class NuevaRequest(BaseModel):
    """Request from Nueva Rust client."""

    # Required fields
    mode: AceStepMode = Field(description="Processing mode")
    input_path: str = Field(description="Path to input audio file")
    output_path: str = Field(description="Path for output audio file")

    # Common parameters
    prompt: Optional[str] = Field(default=None, description="Text prompt for generation/transformation")
    intensity: float = Field(default=0.7, ge=0.0, le=1.0, description="Transformation intensity")

    # Mode-specific parameters
    preserve_melody: bool = Field(default=True, description="Preserve original melody (cover mode)")
    preserve_tempo: bool = Field(default=True, description="Preserve original tempo")
    preserve_key: bool = Field(default=True, description="Preserve original key")

    # Extract mode parameters
    extract_target: Optional[str] = Field(
        default=None,
        description="What to extract: vocals, drums, bass, other, all"
    )

    # Layer mode parameters
    add_layers: Optional[list[str]] = Field(default=None, description="Instruments to add")
    remove_layers: Optional[list[str]] = Field(default=None, description="Instruments to remove")

    # Generation parameters
    duration_seconds: Optional[float] = Field(default=None, description="Target duration for generation")
    seed: Optional[int] = Field(default=None, description="Random seed for reproducibility")

    # Quality parameters
    quantization: Optional[str] = Field(
        default=None,
        description="Quantization level: fp32, fp16, int8"
    )

    # Additional model-specific params
    extra_params: dict[str, Any] = Field(default_factory=dict, description="Additional parameters")


class NuevaResponse(BaseModel):
    """Response to Nueva Rust client."""

    success: bool = Field(description="Whether processing succeeded")
    output_path: Optional[str] = Field(default=None, description="Path to output file")
    processing_time_ms: int = Field(description="Processing time in milliseconds")
    description: str = Field(description="Human-readable description of what was done")

    # Artifacts for context tracking
    intentional_artifacts: list[str] = Field(
        default_factory=list,
        description="Intentional artifacts introduced"
    )

    # Warnings
    warnings: list[str] = Field(default_factory=list, description="Processing warnings")

    # Error details (if success=False)
    error_code: Optional[str] = Field(default=None, description="Error code")
    error_message: Optional[str] = Field(default=None, description="Error message")

    # Metadata
    metadata: dict[str, Any] = Field(default_factory=dict, description="Additional metadata")


class AceStepRequest(BaseModel):
    """Request to ACE-Step API."""

    task: str = Field(description="ACE-Step task name")
    audio_path: Optional[str] = Field(default=None, description="Input audio path")
    prompt: Optional[str] = Field(default=None, description="Text prompt")

    # ACE-Step specific parameters
    guidance_scale: float = Field(default=7.0, description="Classifier-free guidance scale")
    num_inference_steps: int = Field(default=50, description="Number of diffusion steps")
    audio_duration: Optional[float] = Field(default=None, description="Target duration in seconds")

    # Additional parameters passed through
    kwargs: dict[str, Any] = Field(default_factory=dict)


class AceStepResponse(BaseModel):
    """Response from ACE-Step API."""

    success: bool
    audio_path: Optional[str] = None
    error: Optional[str] = None
    metadata: dict[str, Any] = Field(default_factory=dict)


def nueva_to_acestep(request: NuevaRequest) -> AceStepRequest:
    """Convert Nueva request to ACE-Step API format."""

    task = NUEVA_TO_ACESTEP_TASK[request.mode]

    # Build kwargs based on mode
    kwargs: dict[str, Any] = {}

    # Map intensity to guidance scale (higher intensity = stronger effect)
    # ACE-Step uses guidance_scale typically 1-15, with 7 being default
    guidance_scale = 3.0 + (request.intensity * 12.0)  # Maps 0-1 to 3-15

    if request.mode == AceStepMode.COVER:
        kwargs["preserve_melody"] = request.preserve_melody
        kwargs["preserve_tempo"] = request.preserve_tempo
        kwargs["preserve_key"] = request.preserve_key

    elif request.mode == AceStepMode.EXTRACT:
        if request.extract_target:
            kwargs["target"] = request.extract_target

    elif request.mode == AceStepMode.LAYER:
        if request.add_layers:
            kwargs["add_instruments"] = request.add_layers
        if request.remove_layers:
            kwargs["remove_instruments"] = request.remove_layers

    # Add seed if specified
    if request.seed is not None:
        kwargs["seed"] = request.seed

    # Add any extra params
    kwargs.update(request.extra_params)

    return AceStepRequest(
        task=task,
        audio_path=request.input_path,
        prompt=request.prompt,
        guidance_scale=guidance_scale,
        audio_duration=request.duration_seconds,
        kwargs=kwargs,
    )


def acestep_to_nueva(
    acestep_response: AceStepResponse,
    original_request: NuevaRequest,
    processing_time_ms: int,
) -> NuevaResponse:
    """Convert ACE-Step response to Nueva format."""

    if not acestep_response.success:
        return NuevaResponse(
            success=False,
            processing_time_ms=processing_time_ms,
            description=f"ACE-Step processing failed: {acestep_response.error}",
            error_code="ACESTEP_ERROR",
            error_message=acestep_response.error,
        )

    # Determine intentional artifacts based on mode
    artifacts = []
    mode = original_request.mode

    if mode == AceStepMode.COVER:
        artifacts.append("cover_timbre")
        artifacts.append("different_timbre")
        if original_request.prompt:
            prompt_lower = original_request.prompt.lower()
            if any(g in prompt_lower for g in ["jazz", "rock", "classical", "electronic"]):
                artifacts.append("genre_transformation")

    elif mode == AceStepMode.TRANSFORM:
        artifacts.append("intentional_coloration")
        if original_request.prompt and "vintage" in original_request.prompt.lower():
            artifacts.append("frequency_rolloff")

    elif mode == AceStepMode.EXTRACT:
        artifacts.append("vocal_extraction_artifacts")

    elif mode == AceStepMode.LAYER:
        artifacts.append("layer_artifacts")

    # Build description
    mode_desc = {
        AceStepMode.TRANSFORM: "Generated music from prompt",
        AceStepMode.COVER: "Created cover version",
        AceStepMode.REPAINT: "Repainted audio region",
        AceStepMode.EXTRACT: "Extracted audio sources",
        AceStepMode.LAYER: "Modified instrument layers",
        AceStepMode.COMPLETE: "Added accompaniment",
    }

    description = mode_desc.get(mode, "Processed audio")
    if original_request.prompt:
        description += f": '{original_request.prompt}'"

    return NuevaResponse(
        success=True,
        output_path=acestep_response.audio_path or original_request.output_path,
        processing_time_ms=processing_time_ms,
        description=description,
        intentional_artifacts=artifacts,
        metadata=acestep_response.metadata,
    )
