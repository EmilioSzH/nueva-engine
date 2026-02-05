"""FastAPI server for Nueva AI Bridge.

This server acts as a bridge between Nueva (Rust) and ACE-Step (Python).
It handles:
- Protocol translation between Nueva and ACE-Step formats
- Process lifecycle management for ACE-Step
- Audio preprocessing (sample rate conversion if needed)
- Health monitoring and auto-restart
"""

import logging
import os
import time
from contextlib import asynccontextmanager
from typing import Optional

import aiofiles
import aiofiles.os
from fastapi import FastAPI, HTTPException, File, UploadFile, Form
from fastapi.responses import FileResponse, JSONResponse

from .acestep_client import get_client
from .process_manager import get_process_manager
from .protocol import (
    NuevaRequest,
    NuevaResponse,
    nueva_to_acestep,
    acestep_to_nueva,
    AceStepMode,
)

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
)
logger = logging.getLogger(__name__)


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan handler."""
    logger.info("Nueva AI Bridge starting up...")

    # Ensure ACE-Step is running on startup
    process_manager = get_process_manager()
    if process_manager.auto_start:
        await process_manager.ensure_running()

    yield

    # Cleanup on shutdown
    logger.info("Nueva AI Bridge shutting down...")
    await process_manager.stop()


app = FastAPI(
    title="Nueva AI Bridge",
    description="Bridge between Nueva and ACE-Step neural models",
    version="0.1.0",
    lifespan=lifespan,
)


@app.get("/health")
async def health_check():
    """Health check endpoint."""
    process_manager = get_process_manager()
    acestep_healthy = await process_manager.health_check()

    return {
        "status": "healthy" if acestep_healthy else "degraded",
        "bridge": "running",
        "acestep": "connected" if acestep_healthy else "disconnected",
    }


@app.get("/status")
async def status():
    """Detailed status information."""
    process_manager = get_process_manager()
    acestep_healthy = await process_manager.health_check()
    client = get_client()

    models = []
    if acestep_healthy:
        models = await client.get_models()

    return {
        "bridge_version": "0.1.0",
        "acestep_connected": acestep_healthy,
        "acestep_url": process_manager.acestep_url,
        "auto_start_enabled": process_manager.auto_start,
        "available_models": models,
        "available_modes": [mode.value for mode in AceStepMode],
    }


@app.post("/process", response_model=NuevaResponse)
async def process_audio(request: NuevaRequest):
    """Process audio through ACE-Step.

    This is the main endpoint that Nueva calls.
    """
    start_time = time.time()

    logger.info(f"Received request: mode={request.mode}, input={request.input_path}")

    # Ensure ACE-Step is running
    process_manager = get_process_manager()
    if not await process_manager.ensure_running():
        raise HTTPException(
            status_code=503,
            detail="ACE-Step API is not available. Please ensure ACE-Step is installed.",
        )

    # Validate input file exists
    if request.input_path and not await aiofiles.os.path.exists(request.input_path):
        raise HTTPException(
            status_code=400,
            detail=f"Input file not found: {request.input_path}",
        )

    try:
        # Convert to ACE-Step format
        acestep_request = nueva_to_acestep(request)

        # Send to ACE-Step
        client = get_client()
        acestep_response = await client.process(acestep_request)

        # Convert response back to Nueva format
        processing_time_ms = int((time.time() - start_time) * 1000)
        response = acestep_to_nueva(acestep_response, request, processing_time_ms)

        logger.info(
            f"Processing complete: success={response.success}, time={processing_time_ms}ms"
        )

        return response

    except Exception as e:
        logger.exception(f"Processing error: {e}")
        processing_time_ms = int((time.time() - start_time) * 1000)
        return NuevaResponse(
            success=False,
            processing_time_ms=processing_time_ms,
            description=f"Processing failed: {str(e)}",
            error_code="PROCESSING_ERROR",
            error_message=str(e),
        )


@app.post("/process/upload")
async def process_uploaded_audio(
    file: UploadFile = File(...),
    mode: str = Form(...),
    prompt: Optional[str] = Form(None),
    intensity: float = Form(0.7),
):
    """Process uploaded audio file.

    Alternative endpoint that accepts file upload instead of path.
    Useful for remote/web clients.
    """
    start_time = time.time()

    # Save uploaded file to temp location
    temp_dir = os.getenv("NUEVA_TEMP_DIR", "/tmp/nueva")
    os.makedirs(temp_dir, exist_ok=True)

    input_path = os.path.join(temp_dir, f"input_{int(time.time())}_{file.filename}")
    output_path = os.path.join(temp_dir, f"output_{int(time.time())}_{file.filename}")

    try:
        # Save uploaded file
        async with aiofiles.open(input_path, "wb") as f:
            content = await file.read()
            await f.write(content)

        # Create request
        request = NuevaRequest(
            mode=AceStepMode(mode),
            input_path=input_path,
            output_path=output_path,
            prompt=prompt,
            intensity=intensity,
        )

        # Process
        response = await process_audio(request)

        # If successful and output exists, return the file
        if response.success and response.output_path:
            if await aiofiles.os.path.exists(response.output_path):
                return FileResponse(
                    response.output_path,
                    media_type="audio/wav",
                    filename=f"processed_{file.filename}",
                )

        return JSONResponse(content=response.model_dump())

    finally:
        # Cleanup input file
        try:
            await aiofiles.os.remove(input_path)
        except Exception:
            pass


@app.post("/restart-acestep")
async def restart_acestep():
    """Restart the ACE-Step API process."""
    process_manager = get_process_manager()
    success = await process_manager.restart()

    if success:
        return {"status": "restarted", "healthy": True}
    else:
        raise HTTPException(
            status_code=500,
            detail="Failed to restart ACE-Step API",
        )


def run_server(host: str = "127.0.0.1", port: int = 8001, log_level: str = "info"):
    """Run the FastAPI server."""
    import uvicorn

    uvicorn.run(
        app,
        host=host,
        port=port,
        log_level=log_level,
    )
