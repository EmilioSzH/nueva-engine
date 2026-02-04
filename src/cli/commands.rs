//! CLI Command Implementations
//!
//! Implements the actual logic for each CLI command.

use std::path::Path;

use log::{info, warn};

use crate::state::{recover_from_crash, Project, UndoManager};
use crate::Result;

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
