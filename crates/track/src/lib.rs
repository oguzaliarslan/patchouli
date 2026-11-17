//! Decides whether a detected episode should update the list, and drives the
//! countdown/confirm/commit state machine.
//!
//! Phase 5: port Taiga `src/track/update_decision.cpp` and `update_session.cpp`
//! as pure code with an injected clock.

//!
//! The update trigger setting lives in `tracker_settings::UpdateTrigger`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// Nothing to update.
    #[default]
    Idle,
    /// Update is not allowed, see [`Reason`].
    Denied,
    /// List will be updated automatically when the countdown ends.
    Countdown,
    /// List will be updated automatically when the media is closed.
    WaitingForClose,
    /// List will be updated if the user agrees, see [`Reason`].
    Confirming,
    Committed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reason {
    #[default]
    None,
    InvalidEpisode,
    OutsideLibrary,
    AlreadyWatched,
    SkipsAhead,
}
