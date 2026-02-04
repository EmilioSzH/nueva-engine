//! Nueva CLI - Audio Processing System
//!
//! Command-line interface for the Nueva audio processing system.

use clap::Parser;
use env_logger::Env;
use log::info;

use nueva_state::cli::{Cli, Commands};
use nueva_state::Result;

fn main() -> Result<()> {
    // Initialize logger
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    info!("Nueva Audio Processor v{}", env!("CARGO_PKG_VERSION"));

    match cli.command {
        Some(cmd) => handle_command(cmd),
        None => {
            // Interactive mode (not implemented yet)
            println!("Nueva Audio Processor v{}", env!("CARGO_PKG_VERSION"));
            println!("Use --help for available commands");
            Ok(())
        }
    }
}

fn handle_command(cmd: Commands) -> Result<()> {
    match cmd {
        Commands::CreateProject { path, input } => {
            nueva_state::cli::commands::create_project(&path, input.as_deref())
        }
        Commands::LoadProject { path } => nueva_state::cli::commands::load_project(&path),
        Commands::SaveState { path } => nueva_state::cli::commands::save_state(&path),
        Commands::Undo { path } => nueva_state::cli::commands::undo(&path),
        Commands::Redo { path } => nueva_state::cli::commands::redo(&path),
        Commands::History { path } => nueva_state::cli::commands::show_history(&path),
        Commands::Bake { path } => nueva_state::cli::commands::bake(&path),
        Commands::PrintState { path } => nueva_state::cli::commands::print_state(&path),
    }
}
