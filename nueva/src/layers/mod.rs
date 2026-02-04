//! Layer Management
//!
//! Three-layer audio model:
//! - Layer 0: Immutable source (original audio)
//! - Layer 1: AI state buffer (neural processing results)
//! - Layer 2: DSP chain (real-time effects)
//!
//! Implementation: wt-engine worktree

use crate::audio::AudioBuffer;

/// Layer identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// Layer 0: Immutable source
    Source = 0,
    /// Layer 1: AI processed
    AiState = 1,
    /// Layer 2: DSP chain output
    DspOutput = 2,
}

/// Manages the three-layer audio model
#[derive(Debug)]
pub struct LayerManager {
    /// Original source audio (immutable after load)
    source: Option<AudioBuffer>,
    /// AI-processed audio
    ai_state: Option<AudioBuffer>,
    /// Whether AI state is dirty (needs re-render)
    ai_dirty: bool,
}

impl LayerManager {
    pub fn new() -> Self {
        Self {
            source: None,
            ai_state: None,
            ai_dirty: false,
        }
    }

    /// Load source audio into Layer 0
    pub fn load_source(&mut self, buffer: AudioBuffer) {
        self.source = Some(buffer);
        self.ai_state = None;
        self.ai_dirty = true;
    }

    /// Get reference to source audio
    pub fn source(&self) -> Option<&AudioBuffer> {
        self.source.as_ref()
    }

    /// Get reference to AI-processed audio
    pub fn ai_state(&self) -> Option<&AudioBuffer> {
        self.ai_state.as_ref()
    }

    /// Set AI-processed audio
    pub fn set_ai_state(&mut self, buffer: AudioBuffer) {
        self.ai_state = Some(buffer);
        self.ai_dirty = false;
    }

    /// Check if source is loaded
    pub fn has_source(&self) -> bool {
        self.source.is_some()
    }

    /// Get the active audio (AI state if available, otherwise source)
    pub fn active_audio(&self) -> Option<&AudioBuffer> {
        self.ai_state.as_ref().or(self.source.as_ref())
    }
}

impl Default for LayerManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_manager_creation() {
        let manager = LayerManager::new();
        assert!(!manager.has_source());
        assert!(manager.source().is_none());
        assert!(manager.ai_state().is_none());
    }

    #[test]
    fn test_load_source() {
        let mut manager = LayerManager::new();
        let buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);
        manager.load_source(buffer);

        assert!(manager.has_source());
        assert!(manager.source().is_some());
    }

    #[test]
    fn test_active_audio_fallback() {
        let mut manager = LayerManager::new();
        let buffer = AudioBuffer::sine_wave(440.0, 1.0, 44100);
        manager.load_source(buffer);

        // Should return source when no AI state
        assert!(manager.active_audio().is_some());

        // After setting AI state, should return that
        let ai_buffer = AudioBuffer::sine_wave(880.0, 1.0, 44100);
        manager.set_ai_state(ai_buffer);
        assert!(manager.active_audio().is_some());
    }
}
