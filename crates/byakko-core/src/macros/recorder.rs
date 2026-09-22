//! Deterministic append-only recording. Timestamps are supplied milliseconds.
use super::{Action, Capabilities, Event, Program, validate_program};
use serde::{Deserialize, Serialize};
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DelayPolicy {
    Measured { terminal_ms: u32 },
    Fixed(u32),
}

impl DelayPolicy {
    fn terminal(self) -> u32 {
        match self {
            Self::Measured { terminal_ms } => terminal_ms,
            Self::Fixed(ms) => ms,
        }
    }
    fn provisional(self) -> u32 {
        match self {
            Self::Measured { .. } => 0,
            Self::Fixed(ms) => ms,
        }
    }
    fn interval(self, now: u64, previous: u64) -> Option<u32> {
        match self {
            Self::Measured { .. } => now.checked_sub(previous).and_then(|ms| ms.try_into().ok()),
            Self::Fixed(ms) => Some(ms),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Transition {
    Recorded,
    Duplicate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopOutcome {
    Complete,
    TimingClamped,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Recorder {
    policy: DelayPolicy,
    last: Option<(usize, u64)>,
    held: Vec<Action>,
}

impl Recorder {
    pub fn new(caps: &Capabilities, draft: &Program, policy: DelayPolicy) -> Result<Self, String> {
        validate_program(caps, draft)?;
        if !caps.delays_ms.contains(&0) || !caps.delays_ms.contains(&policy.terminal()) {
            return Err("Recording requires supported zero and terminal waits".into());
        }
        if policy == DelayPolicy::Fixed(0) {
            return Err("Fixed recording delay must be positive".into());
        }
        Ok(Self {
            policy,
            last: None,
            held: vec![],
        })
    }

    pub fn last_timestamp(&self) -> u64 {
        self.last.map_or(0, |(_, at)| at)
    }

    /// Caller lends the sole draft; other edits must stay blocked until stop.
    pub fn transition(
        &mut self,
        caps: &Capabilities,
        draft: &mut Program,
        action: Action,
        now: u64,
    ) -> Result<Transition, String> {
        let (identity, pressed) = identity(&action)?;
        if self.held.contains(&identity) == pressed {
            return Ok(Transition::Duplicate);
        }
        let delay = match self.last {
            Some((_, at)) => self
                .policy
                .interval(now, at)
                .filter(|ms| caps.delays_ms.contains(ms))
                .ok_or("Recording interval exceeds supported delay or timestamp moved backwards")?,
            None => 0,
        };
        let mut held = self.held.clone();
        if pressed {
            held.push(identity);
        } else {
            held.retain(|key| key != &identity);
        }
        let mut candidate = draft.clone();
        if let Some((index, _)) = self.last {
            candidate.events[index].delay_ms = delay;
        }
        candidate.events.push(Event {
            action,
            delay_ms: self.policy.provisional(),
        });
        validate_program(caps, &candidate)?;
        let last = candidate.events.len() - 1;
        // Reserve the largest possible final held interval and all releases.
        candidate.events[last].delay_ms = match (held.is_empty(), self.policy) {
            (false, DelayPolicy::Measured { .. }) => largest_wait_cost(caps),
            _ => self.policy.terminal(),
        };
        append_releases(&mut candidate, &held, self.policy.terminal());
        validate_program(caps, &candidate)?;
        candidate.events.truncate(last + 1);
        candidate.events[last].delay_ms = self.policy.provisional();
        *draft = candidate;
        self.held = held;
        self.last = Some((last, now));
        Ok(Transition::Recorded)
    }

    pub fn stop(
        &self,
        caps: &Capabilities,
        draft: &mut Program,
        now: u64,
    ) -> Result<StopOutcome, String> {
        let Some((index, at)) = self.last else {
            return Ok(StopOutcome::Complete);
        };
        let interval = self
            .policy
            .interval(now, at)
            .filter(|ms| caps.delays_ms.contains(ms));
        let mut candidate = draft.clone();
        candidate.events[index].delay_ms = if self.held.is_empty() {
            self.policy.terminal()
        } else {
            interval.unwrap_or(0)
        };
        append_releases(&mut candidate, &self.held, self.policy.terminal());
        validate_program(caps, &candidate)?;
        *draft = candidate;
        Ok(if !self.held.is_empty() && interval.is_none() {
            StopOutcome::TimingClamped
        } else {
            StopOutcome::Complete
        })
    }
}

fn identity(action: &Action) -> Result<(Action, bool), String> {
    match *action {
        Action::Key { usage, pressed } => Ok((
            Action::Key {
                usage,
                pressed: true,
            },
            pressed,
        )),
        Action::Button { button, pressed } => Ok((
            Action::Button {
                button,
                pressed: true,
            },
            pressed,
        )),
        _ => Err("Only keyboard and pointer button edges can be recorded".into()),
    }
}

fn append_releases(program: &mut Program, held: &[Action], terminal: u32) {
    for (index, action) in held.iter().rev().enumerate() {
        let action = match *action {
            Action::Key { usage, .. } => Action::Key {
                usage,
                pressed: false,
            },
            Action::Button { button, .. } => Action::Button {
                button,
                pressed: false,
            },
            _ => unreachable!("held input contains only validated keyboard/button edges"),
        };
        program.events.push(Event {
            action,
            delay_ms: if index + 1 == held.len() { terminal } else { 0 },
        });
    }
}

fn largest_wait_cost(caps: &Capabilities) -> u32 {
    let min = *caps.delays_ms.start();
    let max = *caps.delays_ms.end();
    match &caps.byte_budget {
        Some(budget) if !budget.inline_delays.contains(&min) => min,
        _ => max,
    }
}
