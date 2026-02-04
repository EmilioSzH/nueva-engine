//! Layer Model Module
//!
//! Implements the three-layer architecture:
//! - Layer 0: Immutable source storage
//! - Layer 1: AI state buffer
//! - Layer 2: DSP chain (real-time)

mod layer0;
mod layer1;
mod layer2;
mod project;

pub use layer0::{AudioFormat, Layer0};
pub use layer1::{Layer1, Layer1Metadata};
pub use layer2::{EffectState, Layer2};
pub use project::{LayerPreservationPolicy, Project, ProjectStateSummary};
