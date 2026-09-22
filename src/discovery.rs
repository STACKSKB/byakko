//! Bounded read-only discovery retry scheduling, independent of UI and transport.
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct RetrySchedule {
    failures: u32,
    next: Option<Instant>,
}

impl RetrySchedule {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn failed(&mut self, now: Instant) {
        let seconds = match self.failures {
            0 => 5,
            1 => 10,
            2 => 20,
            _ => 30,
        };
        self.failures = self.failures.saturating_add(1);
        self.next = Some(now + Duration::from_secs(seconds));
    }

    pub fn started(&mut self) {
        self.next = None;
    }

    pub fn succeeded(&mut self) {
        *self = Self::default();
    }

    pub fn due(&self, now: Instant) -> bool {
        self.next.is_some_and(|next| now >= next)
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_back_off_without_polling_while_worker_runs() {
        let mut schedule = RetrySchedule::new();
        let mut now = Instant::now();
        assert!(!schedule.due(now));
        for seconds in [5, 10, 20, 30, 30, 30] {
            schedule.failed(now);
            assert!(!schedule.due(now + Duration::from_secs(seconds - 1)));
            now += Duration::from_secs(seconds);
            assert!(schedule.due(now));
            schedule.started();
            assert!(!schedule.due(now + Duration::from_secs(100)));
        }
        schedule.succeeded();
        assert_eq!(schedule.deadline(), None);
        schedule.failed(now);
        assert_eq!(schedule.deadline(), Some(now + Duration::from_secs(5)));
    }
}
