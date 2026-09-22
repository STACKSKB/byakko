//! Scoped in-memory trace of outgoing setter attempts for research builds.
use serde::Serialize;
use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    time::Instant,
};

const MAX_EVENTS: usize = 4096;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Trace {
    pub events: Vec<Event>,
    pub dropped: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub sequence: usize,
    pub report: Vec<u8>,
    pub elapsed_micros: u128,
    pub outcome: Outcome,
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Unfinished,
    TransportOk,
    TransportErr,
    BeforeDeliveryInjected,
    AfterDeliveryInjected,
}

struct Active {
    started: Instant,
    next_sequence: usize,
    trace: Trace,
}
thread_local! { static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) }; }

/// Capture setter attempts on the calling thread. Panics become an unverified
/// error while preserving any event that was in flight as `Unfinished`.
pub fn with_trace<T>(work: impl FnOnce() -> T) -> (Result<T, String>, Trace) {
    let nested = ACTIVE.with(|slot| {
        let mut active = slot.borrow_mut();
        if active.is_some() {
            true
        } else {
            *active = Some(Active {
                started: Instant::now(),
                next_sequence: 0,
                trace: Trace::default(),
            });
            false
        }
    });
    if nested {
        return (Err("nested research trace scope".into()), Trace::default());
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ACTIVE.with(|slot| {
                slot.borrow_mut().take();
            });
        }
    }
    let reset = Reset;
    let result = catch_unwind(AssertUnwindSafe(work))
        .map_err(|_| "Research trace work panicked; device state is unverified".to_owned());
    let trace = ACTIVE.with(|slot| slot.borrow_mut().take().expect("active trace").trace);
    drop(reset);
    (result, trace)
}

/// Returns an event index only when an active trace has capacity. Otherwise
/// the attempted setter increments `dropped` without allocating its report.
pub(crate) fn begin(report: &[u8]) -> Option<usize> {
    ACTIVE.with(|slot| {
        let mut active = slot.borrow_mut();
        let active = active.as_mut()?;
        let sequence = active.next_sequence;
        active.next_sequence += 1;
        if active.trace.events.len() == MAX_EVENTS {
            active.trace.dropped += 1;
            return None;
        }
        let index = active.trace.events.len();
        active.trace.events.push(Event {
            sequence,
            report: report.to_vec(),
            elapsed_micros: active.started.elapsed().as_micros(),
            outcome: Outcome::Unfinished,
            detail: None,
        });
        Some(index)
    })
}

pub(crate) fn finish(index: Option<usize>, outcome: Outcome, detail: Option<String>) {
    let Some(index) = index else { return };
    ACTIVE.with(|slot| {
        if let Some(event) = slot
            .borrow_mut()
            .as_mut()
            .and_then(|active| active.trace.events.get_mut(index))
        {
            event.outcome = outcome;
            event.detail = detail;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_scope_does_not_reset_outer() {
        let (result, trace) = with_trace(|| {
            assert!(with_trace(|| ()).0.is_err());
            let event = begin(&[0, 0x16]);
            finish(event, Outcome::TransportOk, None);
        });
        assert!(result.is_ok());
        assert_eq!(trace.events.len(), 1);
    }
    #[test]
    fn panic_retains_unfinished_and_resets() {
        let (result, trace) = with_trace(|| {
            begin(&[0, 0x16]);
            panic!("test panic");
        });
        assert!(result.unwrap_err().contains("unverified"));
        assert_eq!(trace.events[0].outcome, Outcome::Unfinished);
        assert!(with_trace(|| ()).0.is_ok());
    }
    #[test]
    fn scope_is_thread_local_and_capped() {
        let (result, trace) = with_trace(|| {
            assert!(
                std::thread::spawn(|| begin(&[0, 0x16]))
                    .join()
                    .unwrap()
                    .is_none()
            );
            for _ in 0..MAX_EVENTS + 1 {
                begin(&[0, 0x16]);
            }
        });
        assert!(result.is_ok());
        assert_eq!(trace.events.len(), MAX_EVENTS);
        assert_eq!(trace.dropped, 1);
    }
}
