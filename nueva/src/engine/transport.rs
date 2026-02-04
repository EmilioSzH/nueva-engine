//! Transport state machine
//!
//! Controls playback state: Stopped, Playing, Paused, Rendering

/// Transport playback state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    Stopped,
    Playing,
    Paused,
    Rendering,
}

/// Audio transport controller
#[derive(Debug)]
pub struct Transport {
    state: TransportState,
    position_samples: u64,
    sample_rate: u32,
}

impl Transport {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            state: TransportState::Stopped,
            position_samples: 0,
            sample_rate,
        }
    }

    pub fn state(&self) -> TransportState {
        self.state
    }

    pub fn position_seconds(&self) -> f64 {
        self.position_samples as f64 / self.sample_rate as f64
    }

    pub fn play(&mut self) {
        self.state = TransportState::Playing;
    }

    pub fn pause(&mut self) {
        if self.state == TransportState::Playing {
            self.state = TransportState::Paused;
        }
    }

    pub fn stop(&mut self) {
        self.state = TransportState::Stopped;
        self.position_samples = 0;
    }

    pub fn seek(&mut self, position_seconds: f64) {
        self.position_samples = (position_seconds * self.sample_rate as f64) as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_state_transitions() {
        let mut transport = Transport::new(44100);
        assert_eq!(transport.state(), TransportState::Stopped);

        transport.play();
        assert_eq!(transport.state(), TransportState::Playing);

        transport.pause();
        assert_eq!(transport.state(), TransportState::Paused);

        transport.stop();
        assert_eq!(transport.state(), TransportState::Stopped);
    }

    #[test]
    fn test_transport_seek() {
        let mut transport = Transport::new(44100);
        transport.seek(2.5);
        assert!((transport.position_seconds() - 2.5).abs() < 0.001);
    }
}
