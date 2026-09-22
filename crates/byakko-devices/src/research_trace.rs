//! Scoped in-memory trace of feature-report attempts for research builds.
use serde::Serialize;
use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    time::Instant,
};

const MAX_EVENTS: usize = 16_384;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Trace {
    pub events: Vec<Event>,
    pub dropped: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub sequence: usize,
    pub operation: Operation,
    pub report: Vec<u8>,
    pub reply: Option<Vec<u8>>,
    pub returned_length: Option<usize>,
    pub elapsed_micros: u128,
    pub outcome: Outcome,
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Setter,
    Getter,
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

/// Capture feature-report attempts on the calling thread. Panics become an unverified
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
/// the attempted operation increments `dropped` without allocating its report.
pub(crate) fn begin(report: &[u8]) -> Option<usize> {
    begin_operation(Operation::Setter, report)
}

fn begin_operation(operation: Operation, report: &[u8]) -> Option<usize> {
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
            operation,
            report: report.to_vec(),
            reply: None,
            returned_length: None,
            elapsed_micros: active.started.elapsed().as_micros(),
            outcome: Outcome::Unfinished,
            detail: None,
        });
        Some(index)
    })
}

/// Trace a getter's request and complete 65-byte receive buffer. A successful
/// transport length is recorded as reported; callers validate its framing.
pub(crate) fn read(
    report: &[u8],
    work: impl FnOnce(&mut [u8; 65]) -> crate::hid::Result<usize>,
) -> crate::hid::Result<(usize, [u8; 65])> {
    let event = begin_operation(Operation::Getter, report);
    let mut reply = [0u8; 65];
    let result = work(&mut reply);
    match &result {
        Ok(length) => finish_getter(event, &reply, Some(*length), Outcome::TransportOk, None),
        Err(error) => finish_getter(
            event,
            &reply,
            None,
            Outcome::TransportErr,
            event.map(|_| error.to_string()),
        ),
    }
    result.map(|length| (length, reply))
}

fn finish_getter(
    index: Option<usize>,
    reply: &[u8; 65],
    returned_length: Option<usize>,
    outcome: Outcome,
    detail: Option<String>,
) {
    with_event(index, |event| {
        event.reply = Some(reply.to_vec());
        event.returned_length = returned_length;
        event.outcome = outcome;
        event.detail = detail;
    });
}

pub(crate) fn finish(index: Option<usize>, outcome: Outcome, detail: Option<String>) {
    with_event(index, |event| {
        event.outcome = outcome;
        event.detail = detail;
    });
}

fn with_event(index: Option<usize>, update: impl FnOnce(&mut Event)) {
    let Some(index) = index else { return };
    ACTIVE.with(|slot| {
        if let Some(event) = slot
            .borrow_mut()
            .as_mut()
            .and_then(|active| active.trace.events.get_mut(index))
        {
            update(event);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::io;

    #[test]
    fn getter_success_keeps_full_reply_and_transport_length() {
        let (result, trace) = with_trace(|| {
            read(&[0, 0x80], |buffer| {
                buffer[0] = 0;
                buffer[1] = 0x80;
                buffer[64] = 0x7f;
                Ok(65)
            })
            .unwrap()
        });
        let (length, buffer) = result.unwrap();
        assert_eq!(length, 65);
        assert_eq!(buffer[64], 0x7f);
        let event = &trace.events[0];
        assert_eq!(event.operation, Operation::Getter);
        assert_eq!(event.report, [0, 0x80]);
        assert_eq!(event.reply.as_ref().unwrap().len(), 65);
        assert_eq!(event.reply.as_ref().unwrap()[64], 0x7f);
        assert_eq!(event.returned_length, Some(65));
        assert_eq!(event.outcome, Outcome::TransportOk);
    }

    #[test]
    fn getter_error_keeps_partial_buffer_and_error() {
        let (result, trace) = with_trace(|| {
            read(&[0, 0x85], |buffer| {
                buffer[0] = 9;
                buffer[1] = 0x85;
                Err(io::Error::other("partial read").into())
            })
        });
        assert!(result.unwrap().is_err());
        let event = &trace.events[0];
        assert_eq!(event.operation, Operation::Getter);
        assert_eq!(event.outcome, Outcome::TransportErr);
        assert_eq!(event.returned_length, None);
        assert_eq!(event.reply.as_ref().unwrap()[..3], [9, 0x85, 0]);
        assert!(event.detail.as_deref().unwrap().contains("partial read"));
    }

    #[test]
    fn malformed_length_is_traced_without_validation() {
        let (result, trace) = with_trace(|| read(&[0, 0x87], |_| Ok(66)));
        assert_eq!(result.unwrap().unwrap().0, 66);
        assert_eq!(trace.events[0].returned_length, Some(66));
        assert_eq!(trace.events[0].outcome, Outcome::TransportOk);
    }

    #[test]
    fn getter_panic_remains_unfinished_and_mixed_order_is_sequential() {
        let (result, trace) = with_trace(|| {
            let setter = begin(&[0, 0x16]);
            finish(setter, Outcome::TransportOk, None);
            read(&[0, 0x80], |buffer| {
                buffer[1] = 0x80;
                Ok(65)
            })
            .unwrap();
            let _ = read(&[0, 0x85], |buffer| {
                buffer[1] = 0x85;
                panic!("getter panic")
            });
        });
        assert!(result.unwrap_err().contains("unverified"));
        assert_eq!(
            trace
                .events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(trace.events[0].operation, Operation::Setter);
        assert_eq!(trace.events[1].operation, Operation::Getter);
        assert_eq!(trace.events[2].operation, Operation::Getter);
        assert_eq!(trace.events[2].outcome, Outcome::Unfinished);
        assert!(trace.events[2].reply.is_none());
        assert!(with_trace(|| ()).0.is_ok());
    }
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
        let getter_ran = Cell::new(false);
        let (result, trace) = with_trace(|| {
            assert!(
                std::thread::spawn(|| begin(&[0, 0x16]))
                    .join()
                    .unwrap()
                    .is_none()
            );
            for _ in 0..MAX_EVENTS - 1 {
                begin(&[0, 0x16]);
            }
            read(&[0, 0x80], |buffer| {
                buffer[1] = 0x80;
                Ok(65)
            })
            .unwrap();
            let (length, reply) = read(&[0, 0x85], |buffer| {
                getter_ran.set(true);
                buffer[1] = 0x85;
                Ok(65)
            })
            .unwrap();
            assert_eq!(length, 65);
            assert_eq!(reply[1], 0x85);
        });
        assert!(result.is_ok());
        assert!(getter_ran.get());
        assert_eq!(trace.events.len(), MAX_EVENTS);
        assert_eq!(trace.dropped, 1);
        assert_eq!(trace.events[MAX_EVENTS - 1].operation, Operation::Getter);
    }
}
