# ACE-Step 1.5 Installation Script for Nueva
# This script downloads and sets up ACE-Step 1.5 for use with Nueva

$ErrorActionPreference = "Stop"

$ACE_STEP_VERSION = "1.5"
$ACE_STEP_REPO = "https://github.com/ACE-Step/ACE-Step-1.5.git"
$INSTALL_DIR = "$env:USERPROFILE\ACE-Step-1.5"

Write-Host "============================================" -ForegroundColor Cyan
Write-Host "  ACE-Step $ACE_STEP_VERSION Installation for Nueva" -ForegroundColor Cyan
Write-Host "============================================" -ForegroundColor Cyan
Write-Host ""

# Check prerequisites
Write-Host "Checking prerequisites..." -ForegroundColor Yellow

# Check for Git
if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Host "ERROR: Git is not installed. Please install Git first." -ForegroundColor Red
    exit 1
}

# Check for Python
$pythonCmd = $null
if (Get-Command python -ErrorAction SilentlyContinue) {
    $pythonCmd = "python"
} elseif (Get-Command python3 -ErrorAction SilentlyContinue) {
    $pythonCmd = "python3"
}

if (-not $pythonCmd) {
    Write-Host "ERROR: Python is not installed. Please install Python 3.11+ first." -ForegroundColor Red
    exit 1
}

$pythonVersion = & $pythonCmd --version 2>&1
Write-Host "Found: $pythonVersion" -ForegroundColor Green

# Check for uv package manager
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    Write-Host "Installing uv package manager..." -ForegroundColor Yellow
    Invoke-WebRequest -UseBasicParsing https://astral.sh/uv/install.ps1 | Invoke-Expression
}

Write-Host "Found: uv $(uv --version)" -ForegroundColor Green

# Clone or update ACE-Step
if (Test-Path $INSTALL_DIR) {
    Write-Host ""
    Write-Host "ACE-Step already exists at $INSTALL_DIR" -ForegroundColor Yellow
    $response = Read-Host "Update existing installation? (y/n)"
    if ($response -eq "y") {
        Write-Host "Updating ACE-Step..." -ForegroundColor Yellow
        Push-Location $INSTALL_DIR
        git pull
        Pop-Location
    }
} else {
    Write-Host ""
    Write-Host "Cloning ACE-Step 1.5 to $INSTALL_DIR..." -ForegroundColor Yellow
    git clone $ACE_STEP_REPO $INSTALL_DIR
}

# Setup virtual environment and dependencies
Write-Host ""
Write-Host "Setting up Python environment..." -ForegroundColor Yellow
Push-Location $INSTALL_DIR
uv sync
Pop-Location

# Download models
Write-Host ""
Write-Host "Downloading ACE-Step models (this may take a while)..." -ForegroundColor Yellow
Push-Location $INSTALL_DIR
uv run acestep-download
Pop-Location

# Setup Nueva Python bridge
$NUEVA_PYTHON_DIR = Split-Path -Parent $PSScriptRoot
$NUEVA_PYTHON_DIR = Join-Path $NUEVA_PYTHON_DIR "python"

if (Test-Path $NUEVA_PYTHON_DIR) {
    Write-Host ""
    Write-Host "Setting up Nueva AI bridge..." -ForegroundColor Yellow
    Push-Location $NUEVA_PYTHON_DIR
    if (Get-Command pip -ErrorAction SilentlyContinue) {
        pip install -e .
    } else {
        uv pip install -e .
    }
    Pop-Location
}

# Create environment file
$envFile = Join-Path $NUEVA_PYTHON_DIR ".env"
@"
# Nueva ACE-Step Configuration
NUEVA_ACE_STEP_PATH=$INSTALL_DIR
NUEVA_PYTHON_PATH=$pythonCmd
"@ | Out-File -FilePath $envFile -Encoding UTF8

Write-Host ""
Write-Host "============================================" -ForegroundColor Green
Write-Host "  ACE-Step 1.5 Installation Complete!" -ForegroundColor Green
Write-Host "============================================" -ForegroundColor Green
Write-Host ""
Write-Host "Installation directory: $INSTALL_DIR" -ForegroundColor Cyan
Write-Host ""
Write-Host "To start the ACE-Step API server:" -ForegroundColor Yellow
Write-Host "  cd $INSTALL_DIR" -ForegroundColor White
Write-Host "  uv run acestep-api" -ForegroundColor White
Write-Host ""
Write-Host "To start the Gradio UI:" -ForegroundColor Yellow
Write-Host "  cd $INSTALL_DIR" -ForegroundColor White
Write-Host "  uv run acestep" -ForegroundColor White
Write-Host ""
Write-Host "Environment variables set:" -ForegroundColor Yellow
Write-Host "  NUEVA_ACE_STEP_PATH=$INSTALL_DIR" -ForegroundColor White
Write-Host ""
