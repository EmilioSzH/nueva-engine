"""Process manager for ACE-Step API.

Handles starting, stopping, and monitoring the ACE-Step API process.
"""

import asyncio
import logging
import os
import shutil
import subprocess
import sys
from typing import Optional

import httpx

logger = logging.getLogger(__name__)


class AceStepProcessManager:
    """Manages the ACE-Step API process lifecycle."""

    def __init__(
        self,
        acestep_url: str = "http://localhost:8000",
        auto_start: bool = True,
        startup_timeout: float = 120.0,
        health_check_interval: float = 5.0,
    ):
        self.acestep_url = acestep_url
        self.auto_start = auto_start
        self.startup_timeout = startup_timeout
        self.health_check_interval = health_check_interval

        self._process: Optional[subprocess.Popen] = None
        self._healthy = False
        self._starting = False

    @property
    def is_running(self) -> bool:
        """Check if the ACE-Step process is running."""
        if self._process is None:
            return False
        return self._process.poll() is None

    async def health_check(self) -> bool:
        """Check if ACE-Step API is healthy."""
        try:
            async with httpx.AsyncClient(timeout=5.0) as client:
                response = await client.get(f"{self.acestep_url}/health")
                self._healthy = response.status_code == 200
                return self._healthy
        except Exception:
            self._healthy = False
            return False

    async def ensure_running(self) -> bool:
        """Ensure ACE-Step is running, starting it if necessary."""

        # First, check if it's already running
        if await self.health_check():
            logger.info("ACE-Step API is already running")
            return True

        if not self.auto_start:
            logger.warning("ACE-Step API not running and auto-start is disabled")
            return False

        if self._starting:
            logger.info("ACE-Step is already starting, waiting...")
            return await self._wait_for_healthy()

        return await self.start()

    async def start(self) -> bool:
        """Start the ACE-Step API process."""

        if self._starting:
            return await self._wait_for_healthy()

        self._starting = True
        logger.info("Starting ACE-Step API...")

        try:
            # Find the uv or python executable
            python_cmd = self._find_python_command()
            if python_cmd is None:
                logger.error("Could not find Python or uv to start ACE-Step")
                return False

            # Start the ACE-Step API
            # ACE-Step uses: uv run acestep-api
            if "uv" in python_cmd:
                cmd = [python_cmd, "run", "acestep-api"]
            else:
                # Fallback to direct acestep-api command
                acestep_cmd = shutil.which("acestep-api")
                if acestep_cmd:
                    cmd = [acestep_cmd]
                else:
                    cmd = [python_cmd, "-m", "ace_step.api"]

            logger.info(f"Starting ACE-Step with command: {' '.join(cmd)}")

            self._process = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                # Don't inherit env completely to avoid conflicts
                env={
                    **os.environ,
                    "PYTHONUNBUFFERED": "1",
                },
            )

            # Wait for the API to become healthy
            return await self._wait_for_healthy()

        except Exception as e:
            logger.error(f"Failed to start ACE-Step: {e}")
            return False
        finally:
            self._starting = False

    async def _wait_for_healthy(self) -> bool:
        """Wait for ACE-Step to become healthy."""

        start_time = asyncio.get_event_loop().time()

        while asyncio.get_event_loop().time() - start_time < self.startup_timeout:
            if await self.health_check():
                logger.info("ACE-Step API is now healthy")
                return True

            # Check if process died
            if self._process and self._process.poll() is not None:
                stdout, stderr = self._process.communicate()
                logger.error(
                    f"ACE-Step process died: {stderr.decode() if stderr else 'unknown error'}"
                )
                return False

            await asyncio.sleep(self.health_check_interval)

        logger.error(f"ACE-Step API did not become healthy within {self.startup_timeout}s")
        return False

    def _find_python_command(self) -> Optional[str]:
        """Find the best Python/uv command to use."""

        # Prefer uv if available (ACE-Step standard)
        uv_path = shutil.which("uv")
        if uv_path:
            return uv_path

        # Fall back to python
        python_path = shutil.which("python3") or shutil.which("python")
        if python_path:
            return python_path

        # Last resort: use sys.executable
        return sys.executable

    async def stop(self):
        """Stop the ACE-Step process."""

        if self._process is None:
            return

        logger.info("Stopping ACE-Step API...")

        try:
            self._process.terminate()
            try:
                self._process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                logger.warning("ACE-Step did not terminate gracefully, killing...")
                self._process.kill()
                self._process.wait()
        except Exception as e:
            logger.error(f"Error stopping ACE-Step: {e}")
        finally:
            self._process = None
            self._healthy = False

    async def restart(self) -> bool:
        """Restart the ACE-Step process."""
        await self.stop()
        return await self.start()


# Global instance
_process_manager: Optional[AceStepProcessManager] = None


def get_process_manager() -> AceStepProcessManager:
    """Get or create the global process manager."""
    global _process_manager

    if _process_manager is None:
        acestep_url = os.getenv("NUEVA_ACESTEP_UPSTREAM_URL", "http://localhost:8000")
        auto_start = os.getenv("NUEVA_ACESTEP_AUTO_START", "true").lower() == "true"

        _process_manager = AceStepProcessManager(
            acestep_url=acestep_url,
            auto_start=auto_start,
        )

    return _process_manager
