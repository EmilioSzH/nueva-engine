"""
AI Bridge Protocol Implementation

Handles JSON communication between Rust and Python for neural model processing.
Implements the protocol from Nueva spec §14.1.
"""

import json
import sys
import traceback
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any, Optional
import time


class Action(str, Enum):
    PING = "ping"
    PROCESS = "process"
    LIST_MODELS = "list_models"
    GET_MODEL_INFO = "get_model_info"
    ABORT = "abort"


@dataclass
class BridgeRequest:
    """Request from Rust to Python AI bridge."""
    action: Action
    request_id: Optional[str] = None
    prompt: Optional[str] = None
    input_path: Optional[str] = None
    output_path: Optional[str] = None
    model: Optional[str] = None
    model_params: dict = field(default_factory=dict)
    context: dict = field(default_factory=dict)

    @classmethod
    def from_json(cls, data: dict) -> "BridgeRequest":
        return cls(
            action=Action(data.get("action", "ping")),
            request_id=data.get("request_id"),
            prompt=data.get("prompt"),
            input_path=data.get("input_path"),
            output_path=data.get("output_path"),
            model=data.get("model"),
            model_params=data.get("model_params", {}),
            context=data.get("context", {}),
        )


@dataclass
class BridgeResponse:
    """Response from Python AI bridge to Rust."""
    success: bool
    request_id: Optional[str] = None
    tool_used: Optional[str] = None  # "dsp", "neural", "both", "none"
    reasoning: Optional[str] = None
    message: Optional[str] = None
    neural_changes: Optional[dict] = None
    error: Optional[str] = None
    error_code: Optional[str] = None

    def to_json(self) -> dict:
        result = {"success": self.success}
        if self.request_id:
            result["request_id"] = self.request_id
        if self.tool_used:
            result["tool_used"] = self.tool_used
        if self.reasoning:
            result["reasoning"] = self.reasoning
        if self.message:
            result["message"] = self.message
        if self.neural_changes:
            result["neural_changes"] = self.neural_changes
        if self.error:
            result["error"] = self.error
        if self.error_code:
            result["error_code"] = self.error_code
        return result


class AIBridge:
    """Main AI Bridge handler."""

    def __init__(self):
        self.models = {}
        self._load_models()

    def _load_models(self):
        """Load available model processors."""
        try:
            from .ace_step import AceStepProcessor
            self.models["ace-step"] = AceStepProcessor()
        except ImportError as e:
            print(f"Warning: ACE-Step not available: {e}", file=sys.stderr)

        # Add other models as they become available
        # self.models["style-transfer"] = StyleTransferProcessor()
        # self.models["denoise"] = DenoiseProcessor()

    def handle_request(self, request: BridgeRequest) -> BridgeResponse:
        """Handle a bridge request."""
        try:
            if request.action == Action.PING:
                return self._handle_ping(request)
            elif request.action == Action.LIST_MODELS:
                return self._handle_list_models(request)
            elif request.action == Action.GET_MODEL_INFO:
                return self._handle_get_model_info(request)
            elif request.action == Action.PROCESS:
                return self._handle_process(request)
            elif request.action == Action.ABORT:
                return self._handle_abort(request)
            else:
                return BridgeResponse(
                    success=False,
                    request_id=request.request_id,
                    error=f"Unknown action: {request.action}",
                    error_code="UNKNOWN_ACTION",
                )
        except Exception as e:
            return BridgeResponse(
                success=False,
                request_id=request.request_id,
                error=str(e),
                error_code="PROCESSING_ERROR",
            )

    def _handle_ping(self, request: BridgeRequest) -> BridgeResponse:
        return BridgeResponse(
            success=True,
            request_id=request.request_id,
            message="pong",
        )

    def _handle_list_models(self, request: BridgeRequest) -> BridgeResponse:
        models = list(self.models.keys())
        return BridgeResponse(
            success=True,
            request_id=request.request_id,
            message=json.dumps(models),
        )

    def _handle_get_model_info(self, request: BridgeRequest) -> BridgeResponse:
        model_id = request.model
        if not model_id or model_id not in self.models:
            return BridgeResponse(
                success=False,
                request_id=request.request_id,
                error=f"Model not found: {model_id}",
                error_code="MODEL_NOT_FOUND",
            )

        model = self.models[model_id]
        info = model.get_info()
        return BridgeResponse(
            success=True,
            request_id=request.request_id,
            message=json.dumps(info),
        )

    def _handle_process(self, request: BridgeRequest) -> BridgeResponse:
        model_id = request.model
        if not model_id or model_id not in self.models:
            return BridgeResponse(
                success=False,
                request_id=request.request_id,
                error=f"Model not found: {model_id}",
                error_code="MODEL_NOT_FOUND",
            )

        if not request.input_path:
            return BridgeResponse(
                success=False,
                request_id=request.request_id,
                error="input_path is required",
                error_code="MISSING_PARAMETER",
            )

        if not request.output_path:
            return BridgeResponse(
                success=False,
                request_id=request.request_id,
                error="output_path is required",
                error_code="MISSING_PARAMETER",
            )

        model = self.models[model_id]
        start_time = time.time()

        try:
            result = model.process(
                input_path=Path(request.input_path),
                output_path=Path(request.output_path),
                prompt=request.prompt,
                params=request.model_params,
            )

            processing_time_ms = int((time.time() - start_time) * 1000)

            return BridgeResponse(
                success=True,
                request_id=request.request_id,
                tool_used="neural",
                reasoning=result.get("reasoning", ""),
                message=result.get("message", "Processing complete"),
                neural_changes={
                    "model": model_id,
                    "output_path": str(request.output_path),
                    "processing_time_ms": processing_time_ms,
                    "intentional_artifacts": result.get("intentional_artifacts", []),
                },
            )
        except Exception as e:
            return BridgeResponse(
                success=False,
                request_id=request.request_id,
                error=str(e),
                error_code="PROCESSING_ERROR",
            )

    def _handle_abort(self, request: BridgeRequest) -> BridgeResponse:
        # TODO: Implement abort functionality
        return BridgeResponse(
            success=True,
            request_id=request.request_id,
            message="Abort requested",
        )


def main():
    """Main entry point for bridge - reads JSON from stdin, writes to stdout."""
    bridge = AIBridge()

    # Read from stdin
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue

        try:
            data = json.loads(line)
            request = BridgeRequest.from_json(data)
            response = bridge.handle_request(request)
            print(json.dumps(response.to_json()), flush=True)
        except json.JSONDecodeError as e:
            error_response = BridgeResponse(
                success=False,
                error=f"Invalid JSON: {e}",
                error_code="INVALID_JSON",
            )
            print(json.dumps(error_response.to_json()), flush=True)
        except Exception as e:
            error_response = BridgeResponse(
                success=False,
                error=str(e),
                error_code="INTERNAL_ERROR",
            )
            print(json.dumps(error_response.to_json()), flush=True)


if __name__ == "__main__":
    main()
