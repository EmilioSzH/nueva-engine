//! Transport State Machine for Nueva
//!
//! Manages playback, recording, and seeking operations with special
//! handling for AI agent invocation (auto-pause protocol).
//!
//! See spec section 3.0 for full details.

use std::fmt;

// No-op logging macros when log crate is not available
// These macros compile away to nothing, avoiding the need for the log dependency
macro_rules! log_debug {
    ($($arg:tt)*) => {
        // Logging disabled - no-op
        // To enable logging, add the `log` crate and replace with: log::debug!($($arg)*)
    };
}

macro_rules! log_warn {
    ($($arg:tt)*) => {
        // Logging disabled - no-op
        // To enable logging, add the `log` crate and replace with: log::warn!($($arg)*)
    };
}

/// Transport states representing the current playback mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransportState {
    /// Transport is paused (default state)
    #[default]
    Paused,
    /// Audio is actively playing
    Playing,
    /// Audio is being recorded
    Recording,
}

impl fmt::Display for TransportState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportState::Paused => write!(f, "Paused"),
            TransportState::Playing => write!(f, "Playing"),
            TransportState::Recording => write!(f, "Recording"),
        }
    }
}

/// Manages transport state, playhead position, and agent invocation protocol
///
/// The TransportManager handles:
/// - State transitions (play, pause, stop, record)
/// - Playhead position tracking
/// - Auto-pause on agent invocation (critical for coherent AI processing)
/// - Resume after agent completion
#[derive(Debug, Clone)]
pub struct TransportManager {
    /// Current transport state
    state: TransportState,

    /// Current playhead position in seconds
    playhead_position: f64,

    /// Saved playhead position for agent invocation resume
    saved_playhead_position: f64,

    /// Sample rate for position calculations (default: 48000 Hz)
    sample_rate: u32,

    /// Flag indicating if recording buffer should be kept on stop
    keep_recording_buffer: bool,

    /// Previous state before agent invocation (for logging/debugging)
    state_before_agent: Option<TransportState>,
}

impl Default for TransportManager {
    fn default() -> Self {
        Self::new(48000)
    }
}

impl TransportManager {
    /// Create a new TransportManager with the specified sample rate
    ///
    /// # Arguments
    /// * `sample_rate` - The sample rate in Hz (typically 48000)
    ///
    /// # Example
    /// ```
    /// use nueva::engine::TransportManager;
    /// let transport = TransportManager::new(48000);
    /// assert!(transport.is_paused());
    /// ```
    pub fn new(sample_rate: u32) -> Self {
        Self {
            state: TransportState::Paused,
            playhead_position: 0.0,
            saved_playhead_position: 0.0,
            sample_rate,
            keep_recording_buffer: false,
            state_before_agent: None,
        }
    }

    // ========================================================================
    // Agent Invocation Protocol (Critical)
    // ========================================================================

    /// Called when the AI agent is invoked
    ///
    /// AUTO-PAUSE: Always pauses before processing agent command to ensure
    /// audio state is stable during AI processing.
    ///
    /// Behavior by current state:
    /// - Recording: Stop recording (keep buffer), save position, pause
    /// - Playing: Save position, pause
    /// - Paused: No action needed
    ///
    /// # Example
    /// ```
    /// use nueva::engine::TransportManager;
    /// let mut transport = TransportManager::new(48000);
    /// transport.play();
    /// transport.seek(5.0);
    ///
    /// // Agent invoked - auto-pauses
    /// transport.on_agent_invoked();
    /// assert!(transport.is_paused());
    /// ```
    pub fn on_agent_invoked(&mut self) {
        // Save state before agent for debugging
        self.state_before_agent = Some(self.state);

        match self.state {
            TransportState::Recording => {
                // Save current position for potential resume
                self.saved_playhead_position = self.playhead_position;
                // Stop recording but keep buffer for potential "keep that"
                self.keep_recording_buffer = true;
                self.state = TransportState::Paused;
                log_debug!(
                    "[AUTO-PAUSE] Stopped recording, saved position: {:.3}s",
                    self.saved_playhead_position
                );
            }
            TransportState::Playing => {
                // Save current position for potential resume
                self.saved_playhead_position = self.playhead_position;
                self.state = TransportState::Paused;
                log_debug!(
                    "[AUTO-PAUSE] Paused playback, saved position: {:.3}s",
                    self.saved_playhead_position
                );
            }
            TransportState::Paused => {
                // Already paused - no action needed
                // Still save position in case agent wants to resume from here
                self.saved_playhead_position = self.playhead_position;
                log_debug!(
                    "[AUTO-PAUSE] Already paused at {:.3}s",
                    self.playhead_position
                );
            }
        }
    }

    /// Called when the AI agent completes processing
    ///
    /// # Arguments
    /// * `should_resume` - If true, seeks to saved position and plays.
    ///                     If false, stays paused so user can hear the change.
    ///
    /// # Example
    /// ```
    /// use nueva::engine::TransportManager;
    /// let mut transport = TransportManager::new(48000);
    /// transport.play();
    /// transport.seek(10.0);
    ///
    /// transport.on_agent_invoked();
    /// // ... agent processes ...
    /// transport.on_agent_complete(true); // Resume playback
    ///
    /// assert!(transport.is_playing());
    /// assert_eq!(transport.get_playhead_position(), 10.0);
    /// ```
    pub fn on_agent_complete(&mut self, should_resume: bool) {
        if should_resume {
            self.playhead_position = self.saved_playhead_position;
            self.state = TransportState::Playing;
            log_debug!(
                "[AGENT-COMPLETE] Resumed playback at {:.3}s",
                self.playhead_position
            );
        } else {
            // Stay paused so user can hear the change
            log_debug!("[AGENT-COMPLETE] Staying paused for user review");
        }

        // Clear the saved state
        self.state_before_agent = None;
    }

    /// Get the state the transport was in before agent invocation
    ///
    /// Returns None if agent is not currently processing
    pub fn state_before_agent(&self) -> Option<TransportState> {
        self.state_before_agent
    }

    /// Check if a recording buffer should be kept after agent invocation
    pub fn should_keep_recording_buffer(&self) -> bool {
        self.keep_recording_buffer
    }

    /// Clear the recording buffer flag
    pub fn clear_recording_buffer_flag(&mut self) {
        self.keep_recording_buffer = false;
    }

    // ========================================================================
    // Standard Transport Controls
    // ========================================================================

    /// Start playback from current position
    ///
    /// State transition: Paused -> Playing
    ///
    /// # Example
    /// ```
    /// use nueva::engine::TransportManager;
    /// let mut transport = TransportManager::new(48000);
    /// transport.play();
    /// assert!(transport.is_playing());
    /// ```
    pub fn play(&mut self) {
        match self.state {
            TransportState::Paused => {
                self.state = TransportState::Playing;
                log_debug!("[TRANSPORT] Play from {:.3}s", self.playhead_position);
            }
            TransportState::Playing => {
                // Already playing - no action
                log_debug!("[TRANSPORT] Already playing");
            }
            TransportState::Recording => {
                // Cannot play while recording without stopping first
                // This is a no-op to maintain state machine integrity
                log_warn!("[TRANSPORT] Cannot play while recording - stop recording first");
            }
        }
    }

    /// Pause playback or recording
    ///
    /// State transitions:
    /// - Playing -> Paused
    /// - Recording -> Paused (keeps buffer)
    /// - Paused -> Paused (no-op)
    ///
    /// # Example
    /// ```
    /// use nueva::engine::TransportManager;
    /// let mut transport = TransportManager::new(48000);
    /// transport.play();
    /// transport.pause();
    /// assert!(transport.is_paused());
    /// ```
    pub fn pause(&mut self) {
        match self.state {
            TransportState::Playing => {
                self.state = TransportState::Paused;
                log_debug!("[TRANSPORT] Paused at {:.3}s", self.playhead_position);
            }
            TransportState::Recording => {
                // Pausing during recording - keep the buffer
                self.keep_recording_buffer = true;
                self.state = TransportState::Paused;
                log_debug!(
                    "[TRANSPORT] Recording paused at {:.3}s (buffer kept)",
                    self.playhead_position
                );
            }
            TransportState::Paused => {
                // Already paused - no action
                log_debug!("[TRANSPORT] Already paused");
            }
        }
    }

    /// Stop playback/recording and reset playhead to start
    ///
    /// State transitions: Any -> Paused (position reset to 0)
    ///
    /// # Example
    /// ```
    /// use nueva::engine::TransportManager;
    /// let mut transport = TransportManager::new(48000);
    /// transport.play();
    /// transport.seek(10.0);
    /// transport.stop();
    /// assert!(transport.is_paused());
    /// assert_eq!(transport.get_playhead_position(), 0.0);
    /// ```
    pub fn stop(&mut self) {
        let was_recording = self.state == TransportState::Recording;
        self.state = TransportState::Paused;
        self.playhead_position = 0.0;

        if was_recording {
            // Stop clears the recording buffer flag
            self.keep_recording_buffer = false;
            log_debug!("[TRANSPORT] Recording stopped, playhead reset");
        } else {
            log_debug!("[TRANSPORT] Stopped, playhead reset to 0");
        }
    }

    /// Start recording
    ///
    /// State transitions:
    /// - Paused -> Recording
    /// - Playing -> Recording (continues from current position)
    ///
    /// # Example
    /// ```
    /// use nueva::engine::TransportManager;
    /// let mut transport = TransportManager::new(48000);
    /// transport.record();
    /// assert!(transport.is_recording());
    /// ```
    pub fn record(&mut self) {
        match self.state {
            TransportState::Paused => {
                self.keep_recording_buffer = false; // New recording
                self.state = TransportState::Recording;
                log_debug!(
                    "[TRANSPORT] Recording started at {:.3}s",
                    self.playhead_position
                );
            }
            TransportState::Playing => {
                // Punch-in recording while playing
                self.keep_recording_buffer = false; // New recording
                self.state = TransportState::Recording;
                log_debug!(
                    "[TRANSPORT] Recording (punch-in) at {:.3}s",
                    self.playhead_position
                );
            }
            TransportState::Recording => {
                // Already recording - no action
                log_debug!("[TRANSPORT] Already recording");
            }
        }
    }

    /// Seek to a specific position in seconds
    ///
    /// # Arguments
    /// * `position` - Target position in seconds (clamped to >= 0)
    ///
    /// # Example
    /// ```
    /// use nueva::engine::TransportManager;
    /// let mut transport = TransportManager::new(48000);
    /// transport.seek(5.0);
    /// assert_eq!(transport.get_playhead_position(), 5.0);
    /// ```
    pub fn seek(&mut self, position: f64) {
        // Clamp to non-negative
        self.playhead_position = position.max(0.0);
        log_debug!("[TRANSPORT] Seek to {:.3}s", self.playhead_position);
    }

    /// Get the current playhead position in seconds
    pub fn get_playhead_position(&self) -> f64 {
        self.playhead_position
    }

    /// Get the saved playhead position (for agent resume)
    pub fn get_saved_playhead_position(&self) -> f64 {
        self.saved_playhead_position
    }

    /// Get the current playhead position in samples
    pub fn get_playhead_position_samples(&self) -> u64 {
        (self.playhead_position * self.sample_rate as f64) as u64
    }

    /// Update the playhead position (called during playback/recording)
    ///
    /// # Arguments
    /// * `samples_elapsed` - Number of samples that have elapsed
    pub fn advance_playhead(&mut self, samples_elapsed: u64) {
        if self.state == TransportState::Playing || self.state == TransportState::Recording {
            self.playhead_position += samples_elapsed as f64 / self.sample_rate as f64;
        }
    }

    // ========================================================================
    // State Queries
    // ========================================================================

    /// Check if transport is currently playing
    pub fn is_playing(&self) -> bool {
        self.state == TransportState::Playing
    }

    /// Check if transport is currently recording
    pub fn is_recording(&self) -> bool {
        self.state == TransportState::Recording
    }

    /// Check if transport is currently paused
    pub fn is_paused(&self) -> bool {
        self.state == TransportState::Paused
    }

    /// Get the current transport state
    pub fn state(&self) -> TransportState {
        self.state
    }

    /// Get the sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Set the sample rate
    ///
    /// # Arguments
    /// * `sample_rate` - New sample rate in Hz
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate;
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------------
    // Basic State Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_default_state_is_paused() {
        let transport = TransportManager::new(48000);
        assert!(transport.is_paused());
        assert!(!transport.is_playing());
        assert!(!transport.is_recording());
        assert_eq!(transport.state(), TransportState::Paused);
    }

    #[test]
    fn test_default_playhead_position() {
        let transport = TransportManager::new(48000);
        assert_eq!(transport.get_playhead_position(), 0.0);
    }

    #[test]
    fn test_default_sample_rate() {
        let transport = TransportManager::default();
        assert_eq!(transport.sample_rate(), 48000);
    }

    // ------------------------------------------------------------------------
    // State Transition Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_paused_to_playing() {
        let mut transport = TransportManager::new(48000);
        assert!(transport.is_paused());

        transport.play();
        assert!(transport.is_playing());
        assert!(!transport.is_paused());
    }

    #[test]
    fn test_playing_to_paused() {
        let mut transport = TransportManager::new(48000);
        transport.play();
        assert!(transport.is_playing());

        transport.pause();
        assert!(transport.is_paused());
        assert!(!transport.is_playing());
    }

    #[test]
    fn test_paused_to_recording() {
        let mut transport = TransportManager::new(48000);
        assert!(transport.is_paused());

        transport.record();
        assert!(transport.is_recording());
        assert!(!transport.is_paused());
    }

    #[test]
    fn test_playing_to_recording() {
        let mut transport = TransportManager::new(48000);
        transport.play();
        assert!(transport.is_playing());

        transport.record();
        assert!(transport.is_recording());
        assert!(!transport.is_playing());
    }

    #[test]
    fn test_recording_to_paused() {
        let mut transport = TransportManager::new(48000);
        transport.record();
        assert!(transport.is_recording());

        transport.pause();
        assert!(transport.is_paused());
        assert!(!transport.is_recording());
        // Buffer should be kept on pause
        assert!(transport.should_keep_recording_buffer());
    }

    #[test]
    fn test_stop_resets_playhead() {
        let mut transport = TransportManager::new(48000);
        transport.play();
        transport.seek(10.0);
        assert_eq!(transport.get_playhead_position(), 10.0);

        transport.stop();
        assert!(transport.is_paused());
        assert_eq!(transport.get_playhead_position(), 0.0);
    }

    #[test]
    fn test_stop_during_recording() {
        let mut transport = TransportManager::new(48000);
        transport.record();
        transport.seek(5.0);

        transport.stop();
        assert!(transport.is_paused());
        assert_eq!(transport.get_playhead_position(), 0.0);
        // Buffer should be cleared on stop
        assert!(!transport.should_keep_recording_buffer());
    }

    // ------------------------------------------------------------------------
    // Seek Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_seek_positive() {
        let mut transport = TransportManager::new(48000);
        transport.seek(5.5);
        assert_eq!(transport.get_playhead_position(), 5.5);
    }

    #[test]
    fn test_seek_negative_clamped() {
        let mut transport = TransportManager::new(48000);
        transport.seek(-10.0);
        assert_eq!(transport.get_playhead_position(), 0.0);
    }

    #[test]
    fn test_seek_during_playback() {
        let mut transport = TransportManager::new(48000);
        transport.play();
        transport.seek(20.0);
        assert!(transport.is_playing());
        assert_eq!(transport.get_playhead_position(), 20.0);
    }

    // ------------------------------------------------------------------------
    // Playhead Advance Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_advance_playhead_while_playing() {
        let mut transport = TransportManager::new(48000);
        transport.play();

        // Advance by 1 second worth of samples
        transport.advance_playhead(48000);
        assert!((transport.get_playhead_position() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_advance_playhead_while_recording() {
        let mut transport = TransportManager::new(48000);
        transport.record();

        // Advance by 2 seconds worth of samples
        transport.advance_playhead(96000);
        assert!((transport.get_playhead_position() - 2.0).abs() < 0.0001);
    }

    #[test]
    fn test_advance_playhead_while_paused_does_nothing() {
        let mut transport = TransportManager::new(48000);
        assert!(transport.is_paused());

        transport.advance_playhead(48000);
        assert_eq!(transport.get_playhead_position(), 0.0);
    }

    #[test]
    fn test_playhead_position_samples() {
        let mut transport = TransportManager::new(48000);
        transport.seek(1.5); // 1.5 seconds = 72000 samples at 48kHz
        assert_eq!(transport.get_playhead_position_samples(), 72000);
    }

    // ------------------------------------------------------------------------
    // Agent Invocation Protocol Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_agent_invoked_while_playing() {
        let mut transport = TransportManager::new(48000);
        transport.play();
        transport.seek(10.0);
        assert!(transport.is_playing());

        transport.on_agent_invoked();

        // Should be paused
        assert!(transport.is_paused());
        // Position should be saved
        assert_eq!(transport.get_saved_playhead_position(), 10.0);
        // State before agent should be recorded
        assert_eq!(
            transport.state_before_agent(),
            Some(TransportState::Playing)
        );
    }

    #[test]
    fn test_agent_invoked_while_recording() {
        let mut transport = TransportManager::new(48000);
        transport.record();
        transport.seek(5.0);
        assert!(transport.is_recording());

        transport.on_agent_invoked();

        // Should be paused
        assert!(transport.is_paused());
        // Position should be saved
        assert_eq!(transport.get_saved_playhead_position(), 5.0);
        // Recording buffer should be kept
        assert!(transport.should_keep_recording_buffer());
        // State before agent should be recorded
        assert_eq!(
            transport.state_before_agent(),
            Some(TransportState::Recording)
        );
    }

    #[test]
    fn test_agent_invoked_while_paused() {
        let mut transport = TransportManager::new(48000);
        transport.seek(3.0);
        assert!(transport.is_paused());

        transport.on_agent_invoked();

        // Should still be paused
        assert!(transport.is_paused());
        // Position should be saved
        assert_eq!(transport.get_saved_playhead_position(), 3.0);
        // State before agent should be recorded
        assert_eq!(transport.state_before_agent(), Some(TransportState::Paused));
    }

    #[test]
    fn test_agent_complete_with_resume() {
        let mut transport = TransportManager::new(48000);
        transport.play();
        transport.seek(15.0);

        transport.on_agent_invoked();
        assert!(transport.is_paused());

        // Simulate agent modifying audio, position might have been reset
        transport.seek(0.0);

        transport.on_agent_complete(true); // Resume requested

        // Should be playing at saved position
        assert!(transport.is_playing());
        assert_eq!(transport.get_playhead_position(), 15.0);
        // State before agent should be cleared
        assert_eq!(transport.state_before_agent(), None);
    }

    #[test]
    fn test_agent_complete_without_resume() {
        let mut transport = TransportManager::new(48000);
        transport.play();
        transport.seek(15.0);

        transport.on_agent_invoked();
        assert!(transport.is_paused());

        transport.on_agent_complete(false); // No resume

        // Should stay paused at current position
        assert!(transport.is_paused());
        // Position should remain unchanged from invocation
        assert_eq!(transport.get_playhead_position(), 15.0);
    }

    #[test]
    fn test_full_agent_workflow() {
        let mut transport = TransportManager::new(48000);

        // User is playing audio
        transport.play();
        transport.seek(30.0);
        transport.advance_playhead(48000); // 1 second
        assert!((transport.get_playhead_position() - 31.0).abs() < 0.0001);

        // Agent invoked - auto-pause
        transport.on_agent_invoked();
        assert!(transport.is_paused());
        assert!((transport.get_saved_playhead_position() - 31.0).abs() < 0.0001);

        // Agent processes... user decides to preview from start
        transport.seek(0.0);
        transport.play();
        assert!(transport.is_playing());

        // User pauses after preview
        transport.pause();

        // Agent completes with resume
        transport.on_agent_complete(true);

        // Should resume from saved position
        assert!(transport.is_playing());
        assert!((transport.get_playhead_position() - 31.0).abs() < 0.0001);
    }

    // ------------------------------------------------------------------------
    // Edge Cases
    // ------------------------------------------------------------------------

    #[test]
    fn test_double_play_no_op() {
        let mut transport = TransportManager::new(48000);
        transport.play();
        assert!(transport.is_playing());

        transport.play(); // Should be no-op
        assert!(transport.is_playing());
    }

    #[test]
    fn test_double_pause_no_op() {
        let mut transport = TransportManager::new(48000);
        assert!(transport.is_paused());

        transport.pause(); // Should be no-op
        assert!(transport.is_paused());
    }

    #[test]
    fn test_double_record_no_op() {
        let mut transport = TransportManager::new(48000);
        transport.record();
        assert!(transport.is_recording());

        transport.record(); // Should be no-op
        assert!(transport.is_recording());
    }

    #[test]
    fn test_play_while_recording_blocked() {
        let mut transport = TransportManager::new(48000);
        transport.record();
        assert!(transport.is_recording());

        transport.play(); // Should be blocked (no-op)
        assert!(transport.is_recording()); // Still recording
    }

    #[test]
    fn test_transport_state_display() {
        assert_eq!(format!("{}", TransportState::Paused), "Paused");
        assert_eq!(format!("{}", TransportState::Playing), "Playing");
        assert_eq!(format!("{}", TransportState::Recording), "Recording");
    }

    #[test]
    fn test_set_sample_rate() {
        let mut transport = TransportManager::new(48000);
        assert_eq!(transport.sample_rate(), 48000);

        transport.set_sample_rate(44100);
        assert_eq!(transport.sample_rate(), 44100);
    }

    #[test]
    fn test_clear_recording_buffer_flag() {
        let mut transport = TransportManager::new(48000);
        transport.record();
        transport.pause();
        assert!(transport.should_keep_recording_buffer());

        transport.clear_recording_buffer_flag();
        assert!(!transport.should_keep_recording_buffer());
    }
}
