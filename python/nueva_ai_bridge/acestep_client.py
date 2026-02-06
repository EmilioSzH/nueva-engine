"""ACE-Step API client.

Handles communication with the ACE-Step API upstream.
"""

import logging
import os
from typing import Optional

import httpx

from .protocol import AceStepRequest, AceStepResponse

logger = logging.getLogger(__name__)


class AceStepClient:
    """Client for communicating with ACE-Step API."""

    def __init__(
        self,
        base_url: str = "http://localhost:8000",
        timeout: float = 300.0,  # 5 minutes default - AI processing takes time
    ):
        self.base_url = base_url
        self.timeout = timeout

    async def process(self, request: AceStepRequest) -> AceStepResponse:
        """Send a processing request to ACE-Step API."""

        try:
            async with httpx.AsyncClient(timeout=self.timeout) as client:
                # Build the request payload
                payload = {
                    "task": request.task,
                    "prompt": request.prompt,
                    "guidance_scale": request.guidance_scale,
                    "num_inference_steps": request.num_inference_steps,
                    **request.kwargs,
                }

                # Add optional fields
                if request.audio_path:
                    payload["audio_path"] = request.audio_path
                if request.audio_duration:
                    payload["audio_duration"] = request.audio_duration

                logger.info(f"Sending request to ACE-Step: task={request.task}")
                logger.debug(f"Request payload: {payload}")

                response = await client.post(
                    f"{self.base_url}/generate",
                    json=payload,
                )

                if response.status_code != 200:
                    error_msg = f"ACE-Step API error: {response.status_code} - {response.text}"
                    logger.error(error_msg)
                    return AceStepResponse(
                        success=False,
                        error=error_msg,
                    )

                data = response.json()
                logger.info("ACE-Step processing complete")

                return AceStepResponse(
                    success=True,
                    audio_path=data.get("audio_path"),
                    metadata=data.get("metadata", {}),
                )

        except httpx.TimeoutException:
            error_msg = f"ACE-Step request timed out after {self.timeout}s"
            logger.error(error_msg)
            return AceStepResponse(success=False, error=error_msg)

        except httpx.ConnectError as e:
            error_msg = f"Could not connect to ACE-Step API: {e}"
            logger.error(error_msg)
            return AceStepResponse(success=False, error=error_msg)

        except Exception as e:
            error_msg = f"ACE-Step client error: {e}"
            logger.exception(error_msg)
            return AceStepResponse(success=False, error=error_msg)

    async def health_check(self) -> bool:
        """Check if ACE-Step API is healthy."""
        try:
            async with httpx.AsyncClient(timeout=5.0) as client:
                response = await client.get(f"{self.base_url}/health")
                return response.status_code == 200
        except Exception:
            return False

    async def get_models(self) -> list[str]:
        """Get list of available models from ACE-Step."""
        try:
            async with httpx.AsyncClient(timeout=10.0) as client:
                response = await client.get(f"{self.base_url}/models")
                if response.status_code == 200:
                    return response.json().get("models", [])
        except Exception as e:
            logger.warning(f"Could not get models from ACE-Step: {e}")
        return []


# Global client instance
_client: Optional[AceStepClient] = None


def get_client() -> AceStepClient:
    """Get or create the global ACE-Step client."""
    global _client

    if _client is None:
        base_url = os.getenv("NUEVA_ACESTEP_UPSTREAM_URL", "http://localhost:8000")
        timeout = float(os.getenv("NUEVA_ACESTEP_TIMEOUT_MS", "300000")) / 1000.0

        _client = AceStepClient(base_url=base_url, timeout=timeout)

    return _client
