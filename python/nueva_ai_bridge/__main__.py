"""Entry point for running the Nueva AI Bridge.

Usage:
    python -m nueva_ai_bridge [--host HOST] [--port PORT] [--acestep-url URL]
"""

import argparse
import os
import sys


def main():
    parser = argparse.ArgumentParser(
        description="Nueva AI Bridge - connects Nueva to ACE-Step"
    )
    parser.add_argument(
        "--host",
        default=os.getenv("NUEVA_BRIDGE_HOST", "127.0.0.1"),
        help="Host to bind to (default: 127.0.0.1)",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=int(os.getenv("NUEVA_BRIDGE_PORT", "8001")),
        help="Port to listen on (default: 8001)",
    )
    parser.add_argument(
        "--acestep-url",
        default=os.getenv("NUEVA_ACESTEP_UPSTREAM_URL", "http://localhost:8000"),
        help="ACE-Step API URL (default: http://localhost:8000)",
    )
    parser.add_argument(
        "--auto-start",
        action="store_true",
        default=os.getenv("NUEVA_ACESTEP_AUTO_START", "true").lower() == "true",
        help="Automatically start ACE-Step API if not running",
    )
    parser.add_argument(
        "--log-level",
        default=os.getenv("NUEVA_LOG_LEVEL", "info"),
        choices=["debug", "info", "warning", "error"],
        help="Logging level (default: info)",
    )

    args = parser.parse_args()

    # Set environment variables for the server
    os.environ["NUEVA_ACESTEP_UPSTREAM_URL"] = args.acestep_url
    os.environ["NUEVA_ACESTEP_AUTO_START"] = str(args.auto_start).lower()

    from .server import run_server

    print(f"Starting Nueva AI Bridge on {args.host}:{args.port}")
    print(f"ACE-Step upstream: {args.acestep_url}")
    print(f"Auto-start ACE-Step: {args.auto_start}")

    run_server(host=args.host, port=args.port, log_level=args.log_level)


if __name__ == "__main__":
    main()
