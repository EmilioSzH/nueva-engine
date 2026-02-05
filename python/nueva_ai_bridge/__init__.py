"""Nueva AI Bridge - Connects Nueva to ACE-Step neural models.

This package provides a FastAPI server that:
1. Accepts requests from Nueva (Rust)
2. Translates them to ACE-Step API format
3. Manages the ACE-Step process lifecycle
4. Returns results in Nueva format
"""

__version__ = "0.1.0"

from .server import app, run_server
from .protocol import NuevaRequest, NuevaResponse, AceStepMode

__all__ = [
    "app",
    "run_server",
    "NuevaRequest",
    "NuevaResponse",
    "AceStepMode",
]
