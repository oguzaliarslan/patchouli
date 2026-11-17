//! Detects media players and what they are playing.
//!
//! Phase 5: parse `players.anisthesia` and implement the Windows strategies
//! (window title, open files, UI Automation) behind [`MediaDetector`].

/// A media file or stream that a player is currently showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedMedia {
    pub player: String,
    /// File path, window title or page title, depending on the strategy that matched.
    pub information: String,
    pub focused: bool,
}

pub trait MediaDetector: Send {
    fn poll(&mut self) -> Vec<DetectedMedia>;
}

/// Detector for platforms without an implementation yet.
#[derive(Debug, Default)]
pub struct NullDetector;

impl MediaDetector for NullDetector {
    fn poll(&mut self) -> Vec<DetectedMedia> {
        Vec::new()
    }
}
