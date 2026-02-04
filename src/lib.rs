//! Nueva State Management Library
//!
//! This crate provides state management, persistence, and CLI functionality
//! for the Nueva audio processing system.
//!
//! # Modules
//!
//! - `state`: Core state management (project, undo/redo, autosave, migrations)
//! - `cli`: Command-line interface

pub mod cli;
pub mod state;

pub use state::error::{NuevaError, Result};
pub use state::project::Project;
