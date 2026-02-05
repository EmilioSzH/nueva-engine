#!/bin/bash
# ACE-Step 1.5 Installation Script for Nueva
# This script downloads and sets up ACE-Step 1.5 for use with Nueva

set -e

ACE_STEP_VERSION="1.5"
ACE_STEP_REPO="https://github.com/ACE-Step/ACE-Step-1.5.git"
INSTALL_DIR="${NUEVA_ACE_STEP_PATH:-$HOME/ACE-Step-1.5}"

echo "============================================"
echo "  ACE-Step $ACE_STEP_VERSION Installation for Nueva"
echo "============================================"
echo ""

# Check prerequisites
echo "Checking prerequisites..."

# Check for Git
if ! command -v git &> /dev/null; then
    echo "ERROR: Git is not installed. Please install Git first."
    exit 1
fi

# Check for Python
PYTHON_CMD=""
if command -v python3 &> /dev/null; then
    PYTHON_CMD="python3"
elif command -v python &> /dev/null; then
    PYTHON_CMD="python"
fi

if [ -z "$PYTHON_CMD" ]; then
    echo "ERROR: Python is not installed. Please install Python 3.11+ first."
    exit 1
fi

PYTHON_VERSION=$($PYTHON_CMD --version 2>&1)
echo "Found: $PYTHON_VERSION"

# Check for uv package manager
if ! command -v uv &> /dev/null; then
    echo "Installing uv package manager..."
    curl -LsSf https://astral.sh/uv/install.sh | sh
    export PATH="$HOME/.cargo/bin:$PATH"
fi

echo "Found: uv $(uv --version)"

# Clone or update ACE-Step
if [ -d "$INSTALL_DIR" ]; then
    echo ""
    echo "ACE-Step already exists at $INSTALL_DIR"
    read -p "Update existing installation? (y/n) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "Updating ACE-Step..."
        cd "$INSTALL_DIR"
        git pull
    fi
else
    echo ""
    echo "Cloning ACE-Step 1.5 to $INSTALL_DIR..."
    git clone "$ACE_STEP_REPO" "$INSTALL_DIR"
fi

# Setup virtual environment and dependencies
echo ""
echo "Setting up Python environment..."
cd "$INSTALL_DIR"
uv sync

# Download models
echo ""
echo "Downloading ACE-Step models (this may take a while)..."
uv run acestep-download

# Setup Nueva Python bridge
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NUEVA_PYTHON_DIR="$(dirname "$SCRIPT_DIR")/python"

if [ -d "$NUEVA_PYTHON_DIR" ]; then
    echo ""
    echo "Setting up Nueva AI bridge..."
    cd "$NUEVA_PYTHON_DIR"
    pip install -e . || uv pip install -e .
fi

# Create environment file
ENV_FILE="$NUEVA_PYTHON_DIR/.env"
cat > "$ENV_FILE" << EOF
# Nueva ACE-Step Configuration
NUEVA_ACE_STEP_PATH=$INSTALL_DIR
NUEVA_PYTHON_PATH=$PYTHON_CMD
EOF

echo ""
echo "============================================"
echo "  ACE-Step 1.5 Installation Complete!"
echo "============================================"
echo ""
echo "Installation directory: $INSTALL_DIR"
echo ""
echo "To start the ACE-Step API server:"
echo "  cd $INSTALL_DIR"
echo "  uv run acestep-api"
echo ""
echo "To start the Gradio UI:"
echo "  cd $INSTALL_DIR"
echo "  uv run acestep"
echo ""
echo "Add to your shell profile:"
echo "  export NUEVA_ACE_STEP_PATH=$INSTALL_DIR"
echo ""
