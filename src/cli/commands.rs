//! CLI Command Implementations
//!
//! Implements the actual logic for each CLI command.

use std::path::Path;

use log::{info, warn};

use crate::agent::{Agent, ToolType};
use crate::neural::{AceStep, AceStepMode, NeuralModel, NeuralModelParams};
use crate::state::error::Result;
use crate::state::{recover_from_crash, Project, UndoManager};

/// Create a new project directory.
pub fn create_project(path: &Path, input: Option<&Path>) -> Result<()> {
    info!("Creating new project at: {}", path.display());

    let mut project = Project::create(path, input)?;
    project.save()?;

    println!("Project created: {}", path.display());
    if let Some(input_path) = input {
        println!("Imported audio: {}", input_path.display());
    }

    Ok(())
}

/// Load an existing project and check for crash recovery.
pub fn load_project(path: &Path) -> Result<()> {
    info!("Loading project: {}", path.display());

    // Check for crash recovery first
    let recovery = recover_from_crash(path)?;
    if recovery.needed {
        match recovery.success {
            true => {
                println!("Recovery needed: {}", recovery.message);
                if let Some(autosave_path) = &recovery.recovery_state_path {
                    println!("Autosave available: {}", autosave_path.display());
                    println!("Use 'nueva recover {}' to restore", path.display());
                }
            }
            false => {
                warn!("{}", recovery.message);
            }
        }
    }

    let project = Project::load(path)?;
    println!("Project loaded: {}", path.display());
    println!("Schema version: {}", project.schema_version);
    println!("Last modified: {}", project.modified_at);

    Ok(())
}

/// Save the current project state.
pub fn save_state(path: &Path) -> Result<()> {
    info!("Saving project state: {}", path.display());

    let mut project = Project::load(path)?;
    project.save()?;

    println!("Project saved: {}", path.display());

    Ok(())
}

/// Undo the last action.
pub fn undo(path: &Path) -> Result<()> {
    info!("Undoing last action in: {}", path.display());

    let mut project = Project::load(path)?;
    let mut undo_manager = UndoManager::load(&project.history_dir())?;

    let action = undo_manager.undo(&mut project)?;
    project.save()?;
    undo_manager.save(&project.history_dir())?;

    println!("Undone: {}", action.description);

    Ok(())
}

/// Redo the last undone action.
pub fn redo(path: &Path) -> Result<()> {
    info!("Redoing last undone action in: {}", path.display());

    let mut project = Project::load(path)?;
    let mut undo_manager = UndoManager::load(&project.history_dir())?;

    let action = undo_manager.redo(&mut project)?;
    project.save()?;
    undo_manager.save(&project.history_dir())?;

    println!("Redone: {}", action.description);

    Ok(())
}

/// Show action history.
pub fn show_history(path: &Path) -> Result<()> {
    info!("Showing history for: {}", path.display());

    let project = Project::load(path)?;
    let undo_manager = UndoManager::load(&project.history_dir())?;

    let history = undo_manager.get_history();

    if history.is_empty() {
        println!("No actions in history.");
        return Ok(());
    }

    println!("Action History:");
    println!("{:-<60}", "");

    for (i, action) in history.iter().enumerate() {
        let marker = if i == undo_manager.current_position() {
            ">>> "
        } else {
            "    "
        };
        println!(
            "{}{}: {} ({})",
            marker,
            action.id,
            action.description,
            action.timestamp.format("%Y-%m-%d %H:%M:%S")
        );
    }

    println!("{:-<60}", "");
    println!(
        "Undo stack: {} | Redo stack: {}",
        undo_manager.undo_count(),
        undo_manager.redo_count()
    );

    Ok(())
}

/// Bake all layers (destructive flatten).
pub fn bake(path: &Path) -> Result<()> {
    info!("Baking project: {}", path.display());

    let mut project = Project::load(path)?;

    // Pre-bake validation
    project.validate_for_bake()?;

    // Confirm with user (in real CLI, would be interactive)
    println!("WARNING: Bake is a destructive operation!");
    println!("This will flatten all layers into a new source.");
    println!("Layer 0 will be backed up to: {}/backups/", path.display());

    project.bake()?;

    println!("Bake complete. All layers flattened.");
    println!("Previous Layer 0 backed up.");

    Ok(())
}

/// Print current project state.
pub fn print_state(path: &Path) -> Result<()> {
    let project = Project::load(path)?;

    let json = serde_json::to_string_pretty(&project)?;
    println!("{}", json);

    // Also print storage info
    let storage_manager = crate::state::Layer1StorageManager::new(&project.project_path);
    let usage = storage_manager.get_storage_usage()?;

    println!("\n--- Storage Info ---");
    println!("Layer 1 files: {}", usage.file_count);
    println!("Layer 1 size: {:.1} MB", usage.total_size_mb);

    // Check for warnings
    let warnings = crate::state::storage::check_storage_health(&project)?;
    if !warnings.is_empty() {
        println!("\n--- Warnings ---");
        for warning in warnings {
            println!("{}", warning);
        }
    }

    Ok(())
}

/// Process audio with AI agent (project-based).
pub fn agent_process(path: &Path, prompt: &str, tool: &str, dry_run: bool) -> Result<()> {
    info!("Agent processing: {} with prompt: {}", path.display(), prompt);

    let project = Project::load(path)?;
    let agent = Agent::new();

    // Decide which tool to use
    let decision = agent.decide_tool(prompt);

    println!("=== Nueva AI Agent ===");
    println!("Project: {}", path.display());
    println!("Prompt: \"{}\"", prompt);
    println!();
    println!("Decision:");
    println!("  Tool: {:?}", decision.tool);
    println!("  Confidence: {:.0}%", decision.confidence * 100.0);
    println!("  Reasoning: {}", decision.reasoning);

    if !decision.recommendations.is_empty() {
        println!("  Recommendations: {:?}", decision.recommendations);
    }

    if decision.ask_clarification {
        println!();
        println!("Agent needs clarification. Please provide more details.");
        return Ok(());
    }

    if dry_run {
        println!();
        println!("[Dry run - no changes made]");
        return Ok(());
    }

    // Execute based on tool type
    match decision.tool {
        ToolType::Neural => {
            println!();
            println!("Invoking ACE-Step 1.5...");

            let ace_step = AceStep::new();

            if !ace_step.is_available() {
                println!("ERROR: ACE-Step not available.");
                println!("Install with: .\\scripts\\install-ace-step.ps1");
                return Ok(());
            }

            // Get Layer 0 audio path from project
            let layer0_path = project.project_path.join("layer0").join("source.wav");
            let output_path = project.project_path.join("layer1").join("ai_output.wav");

            // Create layer1 directory if needed
            std::fs::create_dir_all(output_path.parent().unwrap())?;

            let params = NeuralModelParams::new()
                .with_param("mode", "transform")
                .with_param("prompt", prompt)
                .with_param("intensity", 0.7);

            match ace_step.process(&layer0_path, &output_path, &params) {
                Ok(result) => {
                    println!("Processing complete!");
                    println!("  Message: {}", result.description);
                    if !result.intentional_artifacts.is_empty() {
                        println!("  Intentional artifacts: {:?}", result.intentional_artifacts);
                    }
                    println!("  Output: {}", output_path.display());
                }
                Err(e) => {
                    println!("Processing failed: {}", e);
                }
            }
        }
        ToolType::Dsp => {
            println!();
            println!("Applying DSP effects...");
            println!("(DSP chain execution not yet wired to CLI)");
        }
        ToolType::Both => {
            println!();
            println!("This request needs both Neural and DSP processing.");
            println!("Running Neural first, then DSP...");
            println!("(Full pipeline not yet wired to CLI)");
        }
        ToolType::AskClarification => {
            println!();
            println!("Please provide more details about what you want to do.");
        }
    }

    Ok(())
}

/// Process a standalone audio file with ACE-Step.
pub fn process_audio(
    input: &Path,
    output: Option<&Path>,
    prompt: &str,
    mode: &str,
    intensity: f32,
) -> Result<()> {
    info!("Processing audio: {} with prompt: {}", input.display(), prompt);

    // Validate input
    if !input.exists() {
        println!("ERROR: Input file not found: {}", input.display());
        return Ok(());
    }

    // Determine output path
    let output_path = match output {
        Some(p) => p.to_path_buf(),
        None => {
            let stem = input.file_stem().unwrap().to_str().unwrap();
            let ext = input.extension().unwrap_or_default().to_str().unwrap();
            input.with_file_name(format!("{}_processed.{}", stem, ext))
        }
    };

    println!("=== Nueva Audio Processor ===");
    println!("Input: {}", input.display());
    println!("Output: {}", output_path.display());
    println!("Prompt: \"{}\"", prompt);
    println!("Mode: {}", mode);
    println!("Intensity: {:.0}%", intensity * 100.0);
    println!();

    // Initialize ACE-Step
    let ace_step = AceStep::new();

    if !ace_step.is_available() {
        println!("ERROR: ACE-Step not available.");
        println!();
        println!("To install ACE-Step:");
        println!("  .\\scripts\\install-ace-step.ps1");
        println!();
        println!("Or set NUEVA_ACE_STEP_PATH environment variable.");
        return Ok(());
    }

    println!("Using: {}", ace_step.info().name);
    println!("Processing...");
    println!();

    // Map mode string to AceStepMode
    let ace_mode = match mode {
        "cover" => AceStepMode::Cover,
        "repaint" => AceStepMode::Repaint,
        "extract" => AceStepMode::Extract,
        "layer" => AceStepMode::Layer,
        "complete" => AceStepMode::Complete,
        _ => AceStepMode::Transform,
    };

    let params = NeuralModelParams::new()
        .with_param("mode", ace_mode.to_string())
        .with_param("prompt", prompt)
        .with_param("intensity", intensity);

    match ace_step.process(input, &output_path, &params) {
        Ok(result) => {
            println!("=== Processing Complete ===");
            println!("Message: {}", result.description);

            if !result.intentional_artifacts.is_empty() {
                println!();
                println!("Intentional artifacts (don't correct these):");
                for artifact in &result.intentional_artifacts {
                    println!("  - {}", artifact);
                }
            }

            println!();
            println!("Output saved to: {}", output_path.display());
        }
        Err(e) => {
            println!("ERROR: Processing failed");
            println!("{}", e);
        }
    }

    Ok(())
}
