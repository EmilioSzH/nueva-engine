//! GPU detection and VRAM management for neural processing
//!
//! Detects available GPU hardware and determines capability for running
//! neural models like ACE-Step.

use serde::{Deserialize, Serialize};
use std::process::Command;

/// Recommended quantization level based on available VRAM
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuantizationLevel {
    /// Full precision (FP32) - requires 16GB+ VRAM
    FP32,
    /// Half precision (FP16) - requires 8GB+ VRAM
    FP16,
    /// 8-bit quantization - requires 4GB+ VRAM
    INT8,
    /// CPU fallback - no GPU required
    CPU,
}

impl QuantizationLevel {
    /// Minimum VRAM in GB for this quantization level
    pub fn min_vram_gb(&self) -> f32 {
        match self {
            Self::FP32 => 16.0,
            Self::FP16 => 8.0,
            Self::INT8 => 4.0,
            Self::CPU => 0.0,
        }
    }

    /// Human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::FP32 => "Full precision (best quality, requires 16GB+ VRAM)",
            Self::FP16 => "Half precision (good quality, requires 8GB+ VRAM)",
            Self::INT8 => "8-bit quantized (acceptable quality, requires 4GB+ VRAM)",
            Self::CPU => "CPU inference (slowest, no GPU required)",
        }
    }
}

/// Information about detected GPU
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    /// GPU name/model
    pub name: String,
    /// Total VRAM in GB
    pub vram_total_gb: f32,
    /// Available/free VRAM in GB
    pub vram_available_gb: f32,
    /// Driver version
    pub driver_version: String,
    /// CUDA version (if available)
    pub cuda_version: Option<String>,
    /// Whether GPU is suitable for ACE-Step
    pub suitable_for_ace_step: bool,
    /// Recommended quantization level
    pub recommended_quantization: QuantizationLevel,
}

impl GpuInfo {
    /// Detect GPU information from the system
    ///
    /// Currently supports NVIDIA GPUs via nvidia-smi.
    /// Returns None if no compatible GPU is found.
    pub fn detect() -> Option<Self> {
        Self::detect_nvidia()
    }

    /// Detect NVIDIA GPU using nvidia-smi
    fn detect_nvidia() -> Option<Self> {
        // Try to run nvidia-smi with CSV output
        let output = Command::new("nvidia-smi")
            .args([
                "--query-gpu=name,memory.total,memory.free,driver_version",
                "--format=csv,noheader,nounits",
            ])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.lines().next()?;
        let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();

        if parts.len() < 4 {
            return None;
        }

        let name = parts[0].to_string();
        let vram_total_mb: f32 = parts[1].parse().ok()?;
        let vram_free_mb: f32 = parts[2].parse().ok()?;
        let driver_version = parts[3].to_string();

        let vram_total_gb = vram_total_mb / 1024.0;
        let vram_available_gb = vram_free_mb / 1024.0;

        // Detect CUDA version
        let cuda_version = Self::detect_cuda_version();

        // Determine recommended quantization
        let recommended_quantization = if vram_available_gb >= 16.0 {
            QuantizationLevel::FP32
        } else if vram_available_gb >= 8.0 {
            QuantizationLevel::FP16
        } else if vram_available_gb >= 4.0 {
            QuantizationLevel::INT8
        } else {
            QuantizationLevel::CPU
        };

        // ACE-Step needs at least 4GB VRAM
        let suitable_for_ace_step = vram_available_gb >= 4.0;

        Some(Self {
            name,
            vram_total_gb,
            vram_available_gb,
            driver_version,
            cuda_version,
            suitable_for_ace_step,
            recommended_quantization,
        })
    }

    /// Detect CUDA version from nvidia-smi
    fn detect_cuda_version() -> Option<String> {
        let output = Command::new("nvidia-smi")
            .args(["--query-gpu=driver_version", "--format=csv,noheader"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        // nvidia-smi also shows CUDA version in its default output
        let output = Command::new("nvidia-smi").output().ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse CUDA version from output like "CUDA Version: 12.1"
        for line in stdout.lines() {
            if line.contains("CUDA Version") {
                if let Some(version) = line.split(':').nth(1) {
                    return Some(version.trim().to_string());
                }
            }
        }

        None
    }
}

/// Check if the system can run ACE-Step neural models
///
/// Returns a tuple of (can_run, recommended_quantization, reason)
pub fn can_run_ace_step() -> (bool, QuantizationLevel, String) {
    match GpuInfo::detect() {
        Some(gpu) => {
            if gpu.suitable_for_ace_step {
                (
                    true,
                    gpu.recommended_quantization,
                    format!(
                        "GPU detected: {} with {:.1}GB available VRAM",
                        gpu.name, gpu.vram_available_gb
                    ),
                )
            } else {
                (
                    true,
                    QuantizationLevel::CPU,
                    format!(
                        "GPU {} has insufficient VRAM ({:.1}GB available, 4GB required). Using CPU fallback.",
                        gpu.name, gpu.vram_available_gb
                    ),
                )
            }
        }
        None => (
            true,
            QuantizationLevel::CPU,
            "No compatible GPU detected. ACE-Step will use CPU inference (slower).".to_string(),
        ),
    }
}

/// Get a human-readable summary of GPU status
pub fn gpu_status_summary() -> String {
    match GpuInfo::detect() {
        Some(gpu) => {
            let mut summary = format!(
                "GPU: {}\n\
                 VRAM: {:.1}GB total, {:.1}GB available\n\
                 Driver: {}",
                gpu.name, gpu.vram_total_gb, gpu.vram_available_gb, gpu.driver_version
            );

            if let Some(cuda) = &gpu.cuda_version {
                summary.push_str(&format!("\nCUDA: {}", cuda));
            }

            summary.push_str(&format!(
                "\nACE-Step: {}\n\
                 Recommended: {}",
                if gpu.suitable_for_ace_step {
                    "Ready"
                } else {
                    "CPU fallback"
                },
                gpu.recommended_quantization.description()
            ));

            summary
        }
        None => "No compatible GPU detected. Neural models will use CPU inference.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantization_levels() {
        assert!(QuantizationLevel::FP32.min_vram_gb() > QuantizationLevel::FP16.min_vram_gb());
        assert!(QuantizationLevel::FP16.min_vram_gb() > QuantizationLevel::INT8.min_vram_gb());
        assert!(QuantizationLevel::INT8.min_vram_gb() > QuantizationLevel::CPU.min_vram_gb());
    }

    #[test]
    fn test_can_run_ace_step_returns_valid_result() {
        // This should always return a valid result, even without GPU
        let (can_run, quantization, reason) = can_run_ace_step();
        // ACE-Step can always run (with CPU fallback)
        assert!(can_run);
        assert!(!reason.is_empty());
        // Verify quantization is valid
        let _ = quantization.description();
    }

    #[test]
    fn test_gpu_status_summary_returns_string() {
        let summary = gpu_status_summary();
        assert!(!summary.is_empty());
    }

    #[test]
    fn test_quantization_descriptions() {
        // All quantization levels should have descriptions
        assert!(!QuantizationLevel::FP32.description().is_empty());
        assert!(!QuantizationLevel::FP16.description().is_empty());
        assert!(!QuantizationLevel::INT8.description().is_empty());
        assert!(!QuantizationLevel::CPU.description().is_empty());
    }
}
