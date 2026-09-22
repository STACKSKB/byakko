//! Deterministic macro recording rules. UI focus and input translation live in `macro_ui`.

use crate::macros::{self, Macro, MacroEvent};
use std::num::NonZeroU16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DelayPolicy {
    Measured,
    Fixed(NonZeroU16),
}

impl DelayPolicy {
    fn between(self, now: f64, previous: f64) -> Option<u16> {
        match self {
            Self::Measured => elapsed_ms(now, previous),
            Self::Fixed(value) => Some(value.get()),
        }
    }

    fn provisional(self) -> u16 {
        match self {
            Self::Measured => 0,
            Self::Fixed(value) => value.get(),
        }
    }

    fn terminal(self) -> u16 {
        match self {
            Self::Measured => 50,
            Self::Fixed(value) => value.get(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecordError {
    PauseTooLong,
    Capacity,
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::PauseTooLong => "A recording pause exceeded the 65,535 ms delay limit",
            Self::Capacity => "Recording reached the 248-byte safe limit",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Transition {
    Recorded,
    Duplicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StopOutcome {
    Complete,
    PauseTooLong,
}

/// The draft must be encodable when a session starts and remain unchanged except
/// through this recorder: `last_event` indexes an event it appended. Usages
/// come from the trusted physical-input mapper (keys 4..=239, mouse 240..=244).
pub(crate) struct Recorder {
    policy: DelayPolicy,
    last_at: f64,
    last_event: Option<usize>,
    held: Vec<u8>,
}

impl Recorder {
    pub(crate) fn new(start_at: f64, policy: DelayPolicy) -> Self {
        Self {
            policy,
            last_at: start_at,
            last_event: None,
            held: Vec::new(),
        }
    }

    pub(crate) fn transition(
        &mut self,
        draft: &mut Macro,
        usage: u8,
        down: bool,
        now: f64,
    ) -> Result<Transition, RecordError> {
        if self.held.contains(&usage) == down {
            return Ok(Transition::Duplicate);
        }
        let delay = if self.last_event.is_some() {
            self.policy
                .between(now, self.last_at)
                .ok_or(RecordError::PauseTooLong)?
        } else {
            0 // Start-to-first-action idle does not become part of playback.
        };
        let mut held = self.held.clone();
        if down {
            held.push(usage);
        } else {
            held.retain(|key| *key != usage);
        }

        let mut candidate = draft.clone();
        if let Some(index) = self.last_event {
            set_delay(&mut candidate.events[index], delay);
        }
        candidate
            .events
            .push(recorded_event(usage, down, self.policy.provisional()));
        let index = candidate.events.len() - 1;
        macros::encode(&candidate).map_err(|_| RecordError::Capacity)?;
        if held.is_empty() {
            set_delay(&mut candidate.events[index], self.policy.terminal());
        } else {
            for (release_index, usage) in held.iter().rev().enumerate() {
                candidate.events.push(recorded_event(
                    *usage,
                    false,
                    if release_index + 1 == held.len() {
                        self.policy.terminal()
                    } else {
                        0
                    },
                ));
            }
        }
        // Reserve the policy tail and every held release before accepting.
        macros::encode(&candidate).map_err(|_| RecordError::Capacity)?;
        candidate.events.truncate(index + 1);
        set_delay(&mut candidate.events[index], self.policy.provisional());
        *draft = candidate;
        self.held = held;
        self.last_at = now;
        self.last_event = Some(index);
        Ok(Transition::Recorded)
    }

    pub(crate) fn stop(self, draft: &mut Macro, now: f64) -> StopOutcome {
        let pause_too_long =
            !self.held.is_empty() && self.policy.between(now, self.last_at).is_none();
        if !self.held.is_empty()
            && let Some(index) = self.last_event
        {
            set_delay(
                &mut draft.events[index],
                self.policy.between(now, self.last_at).unwrap_or(0),
            );
        }
        let release_count = self.held.len();
        for (index, usage) in self.held.into_iter().rev().enumerate() {
            draft.events.push(recorded_event(
                usage,
                false,
                if index + 1 == release_count {
                    self.policy.terminal()
                } else {
                    0
                },
            ));
        }
        if release_count == 0
            && let Some(index) = self.last_event
        {
            set_delay(&mut draft.events[index], self.policy.terminal());
        }
        debug_assert!(macros::encode(draft).is_ok(), "release space was reserved");
        if pause_too_long {
            StopOutcome::PauseTooLong
        } else {
            StopOutcome::Complete
        }
    }
}

fn elapsed_ms(now: f64, previous: f64) -> Option<u16> {
    if !now.is_finite() || !previous.is_finite() {
        return None;
    }
    let milliseconds = ((now - previous).max(0.0) * 1000.0).round();
    (milliseconds <= f64::from(u16::MAX)).then_some(milliseconds as u16)
}

fn recorded_event(usage: u8, down: bool, delay_ms: u16) -> MacroEvent {
    if (240..=244).contains(&usage) {
        MacroEvent::MouseButton {
            button: usage,
            down,
            delay_ms,
        }
    } else {
        MacroEvent::Key {
            usage,
            down,
            delay_ms,
        }
    }
}

fn set_delay(event: &mut MacroEvent, delay: u16) {
    match event {
        MacroEvent::Key { delay_ms, .. }
        | MacroEvent::MouseButton { delay_ms, .. }
        | MacroEvent::Move { delay_ms, .. } => *delay_ms = delay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> Macro {
        Macro {
            repeat_count: 1,
            events: Vec::new(),
        }
    }

    fn waits(draft: &Macro) -> Vec<u16> {
        draft
            .events
            .iter()
            .map(|event| match event {
                MacroEvent::Key { delay_ms, .. }
                | MacroEvent::MouseButton { delay_ms, .. }
                | MacroEvent::Move { delay_ms, .. } => *delay_ms,
            })
            .collect()
    }

    fn fixed(ms: u16) -> DelayPolicy {
        DelayPolicy::Fixed(NonZeroU16::new(ms).unwrap())
    }

    #[test]
    fn elapsed_rounds_and_rejects_long_pause() {
        assert_eq!(elapsed_ms(1.0504, 1.0), Some(50));
        assert_eq!(elapsed_ms(1.0, 1.0), Some(0));
        assert_eq!(elapsed_ms(65.535, 0.0), Some(u16::MAX));
        assert_eq!(elapsed_ms(65.536, 0.0), None);
        assert_eq!(elapsed_ms(70.0, 0.0), None);
        assert_eq!(elapsed_ms(f64::NAN, 0.0), None);
    }

    #[test]
    fn previous_action_gets_asymmetric_delay_and_initial_idle_is_ignored() {
        let mut draft = empty();
        let mut recorder = Recorder::new(0.0, DelayPolicy::Measured);
        recorder.transition(&mut draft, 4, true, 100.0).unwrap();
        recorder.transition(&mut draft, 4, false, 100.120).unwrap();
        recorder.transition(&mut draft, 5, true, 100.470).unwrap();
        assert!(matches!(
            draft.events[0],
            MacroEvent::Key { delay_ms: 120, .. }
        ));
        assert!(matches!(
            draft.events[1],
            MacroEvent::Key { delay_ms: 350, .. }
        ));
        assert!(matches!(
            draft.events[2],
            MacroEvent::Key { delay_ms: 0, .. }
        ));
    }

    #[test]
    fn append_preserves_prior_draft_and_suppresses_duplicates() {
        let mut draft = empty();
        draft.events.push(MacroEvent::Key {
            usage: 4,
            down: false,
            delay_ms: 33,
        });
        let mut recorder = Recorder::new(0.0, DelayPolicy::Measured);
        recorder.transition(&mut draft, 5, true, 100.0).unwrap();
        assert_eq!(
            recorder.transition(&mut draft, 5, true, 100.01),
            Ok(Transition::Duplicate)
        );
        recorder.transition(&mut draft, 5, false, 100.020).unwrap();
        assert!(matches!(
            draft.events[0],
            MacroEvent::Key { delay_ms: 33, .. }
        ));
        assert!(matches!(
            draft.events[1],
            MacroEvent::Key { delay_ms: 20, .. }
        ));
        assert!(matches!(
            draft.events[2],
            MacroEvent::Key { delay_ms: 0, .. }
        ));
        recorder.stop(&mut draft, 100.030);
        assert_eq!(waits(&draft), vec![33, 20, 50]);
    }

    #[test]
    fn reference_timeline_uses_measured_or_fixed_waits_and_terminal_tail() {
        for (policy, expected) in [
            (DelayPolicy::Measured, vec![150, 50]),
            (fixed(10), vec![10, 10]),
        ] {
            let mut draft = empty();
            let mut recorder = Recorder::new(0.0, policy);
            recorder.transition(&mut draft, 4, true, 100.0).unwrap();
            recorder.transition(&mut draft, 4, false, 100.150).unwrap();
            assert_eq!(recorder.stop(&mut draft, 100.250), StopOutcome::Complete);
            assert_eq!(waits(&draft), expected);
            assert!(macros::encode(&draft).is_ok());
        }
    }

    #[test]
    fn fixed_min_max_and_long_elapsed_are_accepted() {
        for value in [1, u16::MAX] {
            let mut draft = empty();
            let mut recorder = Recorder::new(0.0, fixed(value));
            recorder.transition(&mut draft, 4, true, 100.0).unwrap();
            recorder
                .transition(&mut draft, 4, false, 100_000.0)
                .unwrap();
            assert_eq!(recorder.stop(&mut draft, 200_000.0), StopOutcome::Complete);
            assert_eq!(waits(&draft), vec![value, value]);
        }
    }

    #[test]
    fn stop_without_session_event_preserves_existing_draft() {
        let mut draft = empty();
        draft.events.push(recorded_event(4, false, 27));
        for policy in [DelayPolicy::Measured, fixed(10)] {
            let original = draft.clone();
            assert_eq!(
                Recorder::new(0.0, policy).stop(&mut draft, 100.0),
                StopOutcome::Complete
            );
            assert_eq!(draft, original);
        }
    }

    #[test]
    fn fixed_held_releases_are_simultaneous_except_terminal_tail() {
        let mut draft = empty();
        let mut recorder = Recorder::new(0.0, fixed(10));
        recorder.transition(&mut draft, 4, true, 100.0).unwrap();
        recorder.transition(&mut draft, 5, true, 200.0).unwrap();
        assert_eq!(recorder.stop(&mut draft, 300.0), StopOutcome::Complete);
        assert_eq!(waits(&draft), vec![10, 10, 0, 10]);
        assert!(matches!(
            draft.events[2],
            MacroEvent::Key {
                usage: 5,
                down: false,
                ..
            }
        ));
        assert!(matches!(
            draft.events[3],
            MacroEvent::Key {
                usage: 4,
                down: false,
                ..
            }
        ));
    }

    #[test]
    fn stop_waits_only_while_held_and_releases_in_reverse_order() {
        let mut draft = empty();
        let mut recorder = Recorder::new(0.0, DelayPolicy::Measured);
        recorder.transition(&mut draft, 4, true, 1.0).unwrap();
        recorder.transition(&mut draft, 5, true, 1.1).unwrap();
        assert_eq!(recorder.stop(&mut draft, 1.350), StopOutcome::Complete);
        assert!(matches!(
            draft.events[1],
            MacroEvent::Key {
                usage: 5,
                delay_ms: 250,
                ..
            }
        ));
        assert!(matches!(
            draft.events[2],
            MacroEvent::Key {
                usage: 5,
                down: false,
                delay_ms: 0
            }
        ));
        assert!(matches!(
            draft.events[3],
            MacroEvent::Key {
                usage: 4,
                down: false,
                delay_ms: 50
            }
        ));
        let mut recorder = Recorder::new(0.0, DelayPolicy::Measured);
        let mut draft = empty();
        recorder.transition(&mut draft, 4, true, 1.0).unwrap();
        recorder.transition(&mut draft, 4, false, 1.05).unwrap();
        assert_eq!(recorder.stop(&mut draft, 100.0), StopOutcome::Complete);
        assert!(matches!(
            draft.events[1],
            MacroEvent::Key { delay_ms: 50, .. }
        ));
    }

    #[test]
    fn capacity_failure_is_transactional_and_stop_can_release_held_key() {
        let mut draft = empty();
        for _ in 0..59 {
            draft.events.extend([
                MacroEvent::Key {
                    usage: 4,
                    down: true,
                    delay_ms: 1,
                },
                MacroEvent::Key {
                    usage: 4,
                    down: false,
                    delay_ms: 1,
                },
            ]);
        }
        let mut recorder = Recorder::new(0.0, DelayPolicy::Measured);
        recorder.transition(&mut draft, 240, true, 0.001).unwrap();
        let accepted = draft.clone();
        assert_eq!(
            recorder.transition(&mut draft, 241, true, 0.002),
            Err(RecordError::Capacity)
        );
        assert_eq!(draft, accepted);
        recorder.stop(&mut draft, 0.003);
        assert_eq!(draft.events.len(), accepted.events.len() + 1);
        assert!(matches!(
            draft.events.last(),
            Some(MacroEvent::MouseButton {
                button: 240,
                down: false,
                ..
            })
        ));
        assert!(macros::encode(&draft).is_ok());
    }

    #[test]
    fn fixed_short_wait_uses_available_capacity_and_reserves_final_release() {
        let mut draft = empty();
        for _ in 0..59 {
            draft
                .events
                .extend([recorded_event(4, true, 1), recorded_event(4, false, 1)]);
        }
        let mut recorder = Recorder::new(0.0, fixed(1));
        recorder.transition(&mut draft, 240, true, 0.001).unwrap();
        recorder.transition(&mut draft, 241, true, 70.0).unwrap();
        assert_eq!(recorder.stop(&mut draft, 140.0), StopOutcome::Complete);
        assert_eq!(&waits(&draft)[118..], &[1, 1, 0, 1]);
        assert!(macros::encode(&draft).is_ok());
    }

    #[test]
    fn long_gap_releases_safely_with_diagnostic() {
        let mut draft = empty();
        let mut recorder = Recorder::new(0.0, DelayPolicy::Measured);
        recorder.transition(&mut draft, 4, true, 0.0).unwrap();
        let accepted = draft.clone();
        assert_eq!(
            recorder.transition(&mut draft, 5, true, 70.0),
            Err(RecordError::PauseTooLong)
        );
        assert_eq!(draft, accepted);
        assert_eq!(recorder.stop(&mut draft, 70.0), StopOutcome::PauseTooLong);
        assert!(matches!(
            draft.events[0],
            MacroEvent::Key { delay_ms: 0, .. }
        ));
        assert!(matches!(
            draft.events[1],
            MacroEvent::Key {
                usage: 4,
                down: false,
                ..
            }
        ));
    }
}
