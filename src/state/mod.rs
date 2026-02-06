//! State Management Module
//!
//! Provides project state, persistence, undo/redo, autosave,
//! crash recovery, migrations, and storage management.

pub mod autosave;
pub mod crash_recovery;
pub mod error;
pub mod migration;
pub mod project;
pub mod storage;
pub mod undo;

pub use autosave::AutosaveManager;
pub use crash_recovery::{recover_from_crash, RecoveryResult};
pub use error::{NuevaError, Result};
pub use migration::{migrate_project, CURRENT_SCHEMA_VERSION};
pub use project::Project;
pub use storage::Layer1StorageManager;
pub use undo::UndoManager;
