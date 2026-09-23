use crate::{Device, HostFrame};
use byakko_core::{
    lighting::{self, HostMode},
    session::{ApplyFailure, Recovery},
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, RecvTimeoutError, Sender},
    },
    time::Duration,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostTicket {
    pub generation: u64,
    pub operation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostEvent {
    Started {
        ticket: HostTicket,
    },
    StartFailed {
        ticket: HostTicket,
        failure: ApplyFailure,
    },
    Finished {
        ticket: HostTicket,
        result: Result<lighting::Snapshot, ApplyFailure>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostSubmitError {
    Busy,
    Stale,
    QueueFull,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostFrameError {
    NotActive,
    WrongTicket,
    NotReady,
    Stopping,
    QueueFull,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostStopResult {
    Requested,
    AlreadyRequested,
    NotActive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Phase {
    Idle,
    Starting(HostTicket),
    StartStopping(HostTicket),
    Streaming(HostTicket),
    Stopping(HostTicket),
}

pub(super) struct Control {
    pub phase: Mutex<Phase>,
}

impl Control {
    pub fn new() -> Self {
        Self {
            phase: Mutex::new(Phase::Idle),
        }
    }

    pub fn stop(&self, ticket: HostTicket) -> HostStopResult {
        let mut phase = self.phase.lock().unwrap();
        *phase = match *phase {
            Phase::Starting(active) if active == ticket => Phase::StartStopping(active),
            Phase::Streaming(active) if active == ticket => Phase::Stopping(active),
            Phase::StartStopping(active) | Phase::Stopping(active) if active == ticket => {
                return HostStopResult::AlreadyRequested;
            }
            _ => return HostStopResult::NotActive,
        };
        HostStopResult::Requested
    }

    pub fn mark_started(&self, ticket: HostTicket) {
        let mut phase = self.phase.lock().unwrap();
        *phase = match *phase {
            Phase::Starting(active) if active == ticket => Phase::Streaming(ticket),
            Phase::StartStopping(active) if active == ticket => Phase::Stopping(ticket),
            _ => Phase::Stopping(ticket),
        };
    }

    pub fn stopping(&self, ticket: HostTicket) -> bool {
        !matches!(*self.phase.lock().unwrap(), Phase::Streaming(active) if active == ticket)
    }

    pub fn clear(&self) {
        *self.phase.lock().unwrap() = Phase::Idle;
    }
}

pub(super) struct Frame {
    pub ticket: HostTicket,
    pub value: HostFrame,
}

pub(super) struct Run<'a> {
    pub ticket: HostTicket,
    pub backup_dir: &'a Path,
    pub generation: &'a AtomicU64,
    pub control: &'a Control,
    pub frames: &'a Receiver<Frame>,
    pub events: &'a Sender<HostEvent>,
}

pub(super) fn run(
    device: &mut impl Device,
    mode: HostMode,
    expected: &lighting::Snapshot,
    run: Run<'_>,
) {
    let Run {
        ticket,
        backup_dir,
        generation,
        control,
        frames,
        events,
    } = run;
    let mut activity = match catch_unwind(AssertUnwindSafe(|| {
        device.start_host_lighting(mode, expected, backup_dir)
    })) {
        Ok(Ok(activity)) => activity,
        Ok(Err(failure)) => {
            control.clear();
            let _ = events.send(HostEvent::StartFailed { ticket, failure });
            return;
        }
        Err(_) => {
            control.clear();
            let _ = events.send(HostEvent::StartFailed {
                ticket,
                failure: ApplyFailure {
                    message: "Host lighting setup panicked; device state is unverified".into(),
                    recovery: Recovery::Unverified,
                },
            });
            return;
        }
    };
    control.mark_started(ticket);
    let _ = events.send(HostEvent::Started { ticket });
    let mut frame_error = None;
    while ticket.generation == generation.load(Ordering::Acquire) && !control.stopping(ticket) {
        match frames.recv_timeout(Duration::from_millis(20)) {
            Ok(frame) if frame.ticket == ticket => {
                if control.stopping(ticket) {
                    break;
                }
                match catch_unwind(AssertUnwindSafe(|| activity.send_frame(frame.value))) {
                    Ok(Ok(())) => {}
                    Ok(Err(message)) => {
                        frame_error = Some(message);
                        break;
                    }
                    Err(_) => {
                        frame_error = Some("Host frame send panicked".into());
                        break;
                    }
                }
            }
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    // Restoration is attempted before this worker accepts another command.
    let restored = catch_unwind(AssertUnwindSafe(|| activity.finish())).unwrap_or_else(|_| {
        Err(ApplyFailure {
            message: "Host lighting restoration panicked; device state is unverified".into(),
            recovery: Recovery::Unverified,
        })
    });
    let result = match (frame_error, restored) {
        (None, result) => result,
        (Some(message), Ok(_)) => Err(ApplyFailure {
            message: format!("Host frame failed: {message}; saved lighting restored"),
            recovery: Recovery::Verified,
        }),
        (Some(message), Err(mut failure)) => {
            failure.message = format!("Host frame failed: {message}; {}", failure.message);
            Err(failure)
        }
    };
    control.clear();
    let _ = events.send(HostEvent::Finished { ticket, result });
}
