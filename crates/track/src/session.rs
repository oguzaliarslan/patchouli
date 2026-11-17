//! When to update the list for the episode that's playing
//! (Taiga `track/update_session.cpp`).
//!
//! The session counts one-second ticks while an episode is playing (pausing
//! when the player isn't focused, if set). Once the delay has passed it
//! updates the list, or waits for the player to close, or asks the user,
//! depending on settings and on [`decide_update`]. It doesn't save anything
//! itself: it returns [`Effect`]s for the caller to carry out.

use serde::{Deserialize, Serialize};
use tracker_anitomy::ElementKind;
use tracker_core::{Anime, ListEntry};
use tracker_recognition::Episode;

use crate::decision::{Action, Reason, decide_update, watched_episode_number};
use crate::is_in_list;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateTrigger {
    #[default]
    AfterDelay,
    OnPlayerClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateSettings {
    pub delay_secs: u32,
    pub trigger: UpdateTrigger,
    pub pause_when_unfocused: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            delay_secs: 120,
            trigger: UpdateTrigger::AfterDelay,
            pause_when_unfocused: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    /// Nothing to update.
    #[default]
    Idle,
    /// The update isn't allowed; see `reason`.
    Denied,
    /// The list will be updated after `remaining` seconds.
    Countdown,
    /// The list will be updated when the player closes.
    WaitingForClose,
    /// The list will be updated if the user agrees; see `reason`.
    Confirming,
    Committed,
    /// The user declined.
    Cancelled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateState {
    pub phase: Phase,
    pub reason: Reason,
    pub remaining_secs: u32,
    pub anime_id: i32,
    pub episode: i32,
    pub previous_episode: i32,
    /// The countdown is stopped because the player isn't focused.
    pub paused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Record `episode` as watched (see `entry_with_episode_watched`).
    Commit {
        anime_id: i32,
        episode: i32,
    },
    /// Ask the user, e.g. with a notification.
    ConfirmationRequested(UpdateState),
    StateChanged(UpdateState),
}

/// What the session needs from the database.
pub trait Library {
    fn anime(&self, id: i32) -> Option<Anime>;
    fn entry(&self, id: i32) -> Option<ListEntry>;
}

/// The episode playing and the anime it was identified as, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Current {
    pub episode: Episode,
    pub anime_id: Option<i32>,
    /// The file is outside the library folders while "only update for
    /// episodes in library folders" is on.
    pub outside_library: bool,
}

#[derive(Debug, Default)]
pub struct UpdateSession {
    settings: UpdateSettings,
    current: Option<Current>,
    elapsed: u32,
    paused: bool,
    dismissed: bool,
    committed: bool,
    notified: bool,
    ticking: bool,
    state: UpdateState,
}

fn same_media(previous: &Current, current: &Current) -> bool {
    // Identifying the anime later doesn't make it new media.
    if previous.anime_id != current.anime_id && previous.anime_id.is_some() {
        return false;
    }
    let episodes = |c: &Current| {
        c.episode
            .values(ElementKind::Episode)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    episodes(previous) == episodes(current)
}

impl UpdateSession {
    pub fn new(settings: UpdateSettings) -> Self {
        Self {
            settings,
            ..Self::default()
        }
    }

    pub fn state(&self) -> UpdateState {
        self.state
    }

    /// Ticks are only needed while counting.
    pub fn is_ticking(&self) -> bool {
        self.ticking
    }

    pub fn current(&self) -> Option<&Current> {
        self.current.as_ref()
    }

    /// New settings apply to the current media; time already watched counts.
    pub fn set_settings(
        &mut self,
        settings: UpdateSettings,
        library: &impl Library,
    ) -> Vec<Effect> {
        self.settings = settings;
        if !settings.pause_when_unfocused {
            self.paused = false;
        }
        self.evaluate(library)
    }

    /// The player shows another episode, or `None` when it closed.
    pub fn on_media_changed(
        &mut self,
        current: Option<Current>,
        library: &impl Library,
    ) -> Vec<Effect> {
        let Some(current) = current else {
            return self.on_media_closed(library);
        };
        let same = self
            .current
            .as_ref()
            .is_some_and(|p| same_media(p, &current));
        if !same {
            self.elapsed = 0;
            self.paused = false;
            self.dismissed = false;
            self.committed = false;
            self.notified = false;
        }
        self.current = Some(current);
        if !self.committed {
            self.ticking = true;
        }
        self.evaluate(library)
    }

    /// Called every second while [`is_ticking`](Self::is_ticking). Ticks are
    /// counted rather than timestamps compared, so a skipped tick is a pause.
    pub fn tick(&mut self, player_focused: bool, library: &impl Library) -> Vec<Effect> {
        if !self.ticking {
            return Vec::new();
        }
        // Not in focus is the best guess for the player not playing.
        self.paused = self.settings.pause_when_unfocused && !player_focused;
        if !self.paused {
            self.elapsed += 1;
        }
        self.evaluate(library)
    }

    /// The user agreed to update.
    pub fn accept(&mut self, library: &impl Library) -> Vec<Effect> {
        let Some(current) = self.current.clone().filter(|_| !self.committed) else {
            return Vec::new();
        };
        let Some(anime) = current.anime_id.and_then(|id| library.anime(id)) else {
            return Vec::new();
        };
        let entry = library.entry(anime.id);
        let decision = decide_update(
            &current.episode,
            &anime,
            entry.as_ref(),
            current.outside_library,
        );
        if decision.action == Action::Deny {
            return Vec::new();
        }
        self.commit(&anime, library)
    }

    /// The user declined to update.
    pub fn cancel(&mut self, library: &impl Library) -> Vec<Effect> {
        if self.current.is_none() || self.committed {
            return Vec::new();
        }
        self.dismissed = true;
        self.evaluate(library)
    }

    fn on_media_closed(&mut self, library: &impl Library) -> Vec<Effect> {
        self.ticking = false;
        let mut effects = self.evaluate(library);
        // The committed state stays until the next media.
        if self.committed {
            return effects;
        }
        match self.state.phase {
            Phase::WaitingForClose => {
                if let Some(anime) = self
                    .current
                    .as_ref()
                    .and_then(|c| c.anime_id)
                    .and_then(|id| library.anime(id))
                {
                    effects.extend(self.commit(&anime, library));
                    return effects;
                }
            }
            // Keep asking, unless it was closed too soon.
            Phase::Confirming if self.elapsed >= self.settings.delay_secs => return effects,
            _ => {}
        }
        effects.extend(self.reset());
        effects
    }

    /// Recomputes the state; commits when the countdown is over.
    pub fn evaluate(&mut self, library: &impl Library) -> Vec<Effect> {
        if self.committed {
            return Vec::new();
        }
        let Some((current, anime)) = self
            .current
            .clone()
            .and_then(|c| c.anime_id.and_then(|id| library.anime(id)).map(|a| (c, a)))
        else {
            return self.set_state(UpdateState::default());
        };
        let entry = library.entry(anime.id);
        let decision = decide_update(
            &current.episode,
            &anime,
            entry.as_ref(),
            current.outside_library,
        );
        let mut state = UpdateState {
            reason: decision.reason,
            anime_id: anime.id,
            episode: watched_episode_number(&current.episode, &anime).unwrap_or(0),
            previous_episode: if is_in_list(entry.as_ref()) {
                entry.as_ref().map_or(0, |e| e.watched_episodes)
            } else {
                0
            },
            ..UpdateState::default()
        };
        match decision.action {
            Action::Allow => {
                let remaining = self.settings.delay_secs.saturating_sub(self.elapsed);
                if self.dismissed {
                    state.phase = Phase::Cancelled;
                } else if remaining > 0 {
                    state.phase = Phase::Countdown;
                    state.remaining_secs = remaining;
                    state.paused = self.paused;
                } else if self.settings.trigger == UpdateTrigger::OnPlayerClose {
                    state.phase = Phase::WaitingForClose;
                } else {
                    return self.commit(&anime, library);
                }
            }
            Action::Confirm => {
                state.phase = if self.dismissed {
                    Phase::Cancelled
                } else {
                    Phase::Confirming
                };
            }
            Action::Deny => state.phase = Phase::Denied,
        }
        let mut effects = self.set_state(state);
        if state.phase == Phase::Confirming
            && self.elapsed >= self.settings.delay_secs
            && !self.notified
        {
            self.notified = true;
            effects.push(Effect::ConfirmationRequested(state));
        }
        effects
    }

    fn commit(&mut self, anime: &Anime, library: &impl Library) -> Vec<Effect> {
        let Some(number) = self
            .current
            .as_ref()
            .and_then(|c| watched_episode_number(&c.episode, anime))
        else {
            return Vec::new();
        };
        let entry = library.entry(anime.id);
        let previous = if is_in_list(entry.as_ref()) {
            entry.map_or(0, |e| e.watched_episodes)
        } else {
            0
        };
        self.committed = true;
        self.ticking = false;
        let mut effects = vec![Effect::Commit {
            anime_id: anime.id,
            episode: number,
        }];
        effects.extend(self.set_state(UpdateState {
            phase: Phase::Committed,
            anime_id: anime.id,
            episode: number,
            previous_episode: previous,
            ..UpdateState::default()
        }));
        effects
    }

    fn reset(&mut self) -> Vec<Effect> {
        self.current = None;
        self.dismissed = false;
        self.committed = false;
        self.set_state(UpdateState::default())
    }

    fn set_state(&mut self, state: UpdateState) -> Vec<Effect> {
        if state == self.state {
            return Vec::new();
        }
        self.state = state;
        vec![Effect::StateChanged(state)]
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use tracker_core::ListStatus;

    use super::*;

    #[derive(Default)]
    struct Fake {
        anime: HashMap<i32, Anime>,
        entries: RefCell<HashMap<i32, ListEntry>>,
    }

    impl Library for Fake {
        fn anime(&self, id: i32) -> Option<Anime> {
            self.anime.get(&id).cloned()
        }
        fn entry(&self, id: i32) -> Option<ListEntry> {
            self.entries.borrow().get(&id).cloned()
        }
    }

    fn library(watched: i32) -> Fake {
        let fake = Fake::default();
        let mut fake = fake;
        fake.anime.insert(
            1,
            Anime {
                id: 1,
                episode_count: 12,
                ..Anime::default()
            },
        );
        fake.entries.borrow_mut().insert(
            1,
            ListEntry {
                anime_id: 1,
                watched_episodes: watched,
                status: ListStatus::Watching,
                ..ListEntry::default()
            },
        );
        fake
    }

    fn current(name: &str) -> Option<Current> {
        Some(Current {
            episode: Episode::parse(name),
            anime_id: Some(1),
            outside_library: false,
        })
    }

    fn settings(delay: u32, trigger: UpdateTrigger, pause: bool) -> UpdateSettings {
        UpdateSettings {
            delay_secs: delay,
            trigger,
            pause_when_unfocused: pause,
        }
    }

    fn commits(effects: &[Effect]) -> Vec<(i32, i32)> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::Commit { anime_id, episode } => Some((*anime_id, *episode)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn counts_down_then_commits_once() {
        let lib = library(3);
        let mut s = UpdateSession::new(settings(3, UpdateTrigger::AfterDelay, false));
        s.on_media_changed(current("Show - 04.mkv"), &lib);
        assert_eq!(s.state().phase, Phase::Countdown);
        assert_eq!(s.state().remaining_secs, 3);
        assert!(commits(&s.tick(true, &lib)).is_empty());
        assert!(commits(&s.tick(true, &lib)).is_empty());
        assert_eq!(commits(&s.tick(true, &lib)), [(1, 4)]);
        assert_eq!(s.state().phase, Phase::Committed);
        assert_eq!(s.state().previous_episode, 3);
        assert!(!s.is_ticking());
        // Further ticks and re-detections of the same episode do nothing.
        assert!(s.tick(true, &lib).is_empty());
        assert!(commits(&s.on_media_changed(current("Show - 04.mkv"), &lib)).is_empty());
    }

    #[test]
    fn pauses_while_unfocused() {
        let lib = library(3);
        let mut s = UpdateSession::new(settings(2, UpdateTrigger::AfterDelay, true));
        s.on_media_changed(current("Show - 04.mkv"), &lib);
        s.tick(false, &lib);
        assert!(s.state().paused);
        assert_eq!(s.state().remaining_secs, 2);
        s.tick(true, &lib);
        assert!(!s.state().paused);
        assert_eq!(commits(&s.tick(true, &lib)), [(1, 4)]);
    }

    #[test]
    fn waits_for_the_player_to_close() {
        let lib = library(3);
        let mut s = UpdateSession::new(settings(1, UpdateTrigger::OnPlayerClose, false));
        s.on_media_changed(current("Show - 04.mkv"), &lib);
        assert!(commits(&s.tick(true, &lib)).is_empty());
        assert_eq!(s.state().phase, Phase::WaitingForClose);
        assert_eq!(commits(&s.on_media_changed(None, &lib)), [(1, 4)]);
    }

    #[test]
    fn closing_too_soon_forgets_the_episode() {
        let lib = library(3);
        let mut s = UpdateSession::new(settings(10, UpdateTrigger::AfterDelay, false));
        s.on_media_changed(current("Show - 04.mkv"), &lib);
        s.tick(true, &lib);
        assert!(commits(&s.on_media_changed(None, &lib)).is_empty());
        assert_eq!(s.state().phase, Phase::Idle);
        assert!(s.current().is_none());
    }

    #[test]
    fn skipping_ahead_asks_after_the_delay() {
        let lib = library(3);
        let mut s = UpdateSession::new(settings(2, UpdateTrigger::AfterDelay, false));
        s.on_media_changed(current("Show - 07.mkv"), &lib);
        assert_eq!(s.state().phase, Phase::Confirming);
        assert_eq!(s.state().reason, Reason::SkipsAhead);
        s.tick(true, &lib);
        let effects = s.tick(true, &lib);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::ConfirmationRequested(_)))
        );
        // Asked once only.
        assert!(
            !s.tick(true, &lib)
                .iter()
                .any(|e| matches!(e, Effect::ConfirmationRequested(_)))
        );
        // Still asking after the player closes, since it played long enough.
        s.on_media_changed(None, &lib);
        assert_eq!(s.state().phase, Phase::Confirming);
        assert_eq!(commits(&s.accept(&lib)), [(1, 7)]);
    }

    #[test]
    fn declining_cancels() {
        let lib = library(3);
        let mut s = UpdateSession::new(settings(5, UpdateTrigger::AfterDelay, false));
        s.on_media_changed(current("Show - 04.mkv"), &lib);
        s.cancel(&lib);
        assert_eq!(s.state().phase, Phase::Cancelled);
        for _ in 0..10 {
            assert!(commits(&s.tick(true, &lib)).is_empty());
        }
        // The next episode starts over.
        s.on_media_changed(current("Show - 05.mkv"), &lib);
        assert_eq!(s.state().phase, Phase::Confirming); // 5 skips ahead of 3
    }

    #[test]
    fn identifying_later_keeps_the_time_watched() {
        let lib = library(3);
        let mut s = UpdateSession::new(settings(3, UpdateTrigger::AfterDelay, false));
        let unknown = Some(Current {
            anime_id: None,
            ..current("Show - 04.mkv").unwrap()
        });
        s.on_media_changed(unknown, &lib);
        assert_eq!(s.state().phase, Phase::Idle);
        s.tick(true, &lib);
        s.tick(true, &lib);
        s.on_media_changed(current("Show - 04.mkv"), &lib);
        assert_eq!(s.state().remaining_secs, 1);
    }

    #[test]
    fn already_watched_is_denied() {
        let lib = library(4);
        let mut s = UpdateSession::new(settings(1, UpdateTrigger::AfterDelay, false));
        s.on_media_changed(current("Show - 04.mkv"), &lib);
        assert_eq!(s.state().phase, Phase::Denied);
        assert!(commits(&s.tick(true, &lib)).is_empty());
    }
}
