//! CLI Module
//!
//! Command-line interface for Nueva audio processing system.

pub mod commands;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Nueva Audio Processor - AI-powered audio processing system
#[derive(Parser, Debug)]
#[command(name = "nueva")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create a new project directory
    #[command(name = "create-project")]
    CreateProject {
        /// Path for the new project
        path: PathBuf,

        /// Input audio file (optional)
        #[arg(short, long)]
        input: Option<PathBuf>,
    },

    /// Load an existing project
    #[command(name = "project")]
    LoadProject {
        /// Path to the project
        path: PathBuf,
    },

    /// Save current project state
    #[command(name = "save-state")]
    SaveState {
        /// Path to the project
        path: PathBuf,
    },

    /// Undo the last action
    #[command(name = "undo")]
    Undo {
        /// Path to the project
        #[arg(short, long)]
        path: PathBuf,
    },

    /// Redo the last undone action
    #[command(name = "redo")]
    Redo {
        /// Path to the project
        #[arg(short, long)]
        path: PathBuf,
    },

    /// Show action history
    #[command(name = "history")]
    History {
        /// Path to the project
        #[arg(short, long)]
        path: PathBuf,
    },

    /// Bake all layers (destructive flatten)
    #[command(name = "bake")]
    Bake {
        /// Path to the project
        #[arg(short, long)]
        path: PathBuf,
    },

    /// Print current project state
    #[command(name = "print-state")]
    PrintState {
        /// Path to the project
        path: PathBuf,
    },

    /// Process audio with AI agent
    #[command(name = "agent")]
    Agent {
        /// Path to the project
        #[arg(short, long)]
        path: PathBuf,

        /// Natural language prompt describing desired transformation
        #[arg(short = 'm', long)]
        prompt: String,

        /// Force tool type: auto, dsp, neural
        #[arg(short, long, default_value = "auto")]
        tool: String,

        /// Dry run - show what would be done without executing
        #[arg(long)]
        dry_run: bool,
    },

    /// Process a standalone audio file (no project)
    #[command(name = "process")]
    Process {
        /// Input audio file
        input: PathBuf,

        /// Output audio file
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Natural language prompt
        #[arg(short = 'm', long)]
        prompt: String,

        /// Processing mode: transform, cover, repaint, extract
        #[arg(long, default_value = "transform")]
        mode: String,

        /// Transformation intensity (0.0 - 1.0)
        #[arg(short, long, default_value = "0.7")]
        intensity: f32,
    },
}
