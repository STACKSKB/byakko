// SPDX-License-Identifier: GPL-3.0-or-later
//! Selected, read-only native keyboard input for the local tester.
//!
//! A sampler exists only while the tester is armed. Its id names one OS input
//! endpoint, independently of the configurator's HID collection.
use byakko_core::model::tester::Input;
use std::{
    cell::Cell,
    fmt,
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    thread::JoinHandle,
};

#[cfg(target_os = "windows")]
#[path = "tester_input/windows.rs"]
mod platform;
#[cfg(target_os = "linux")]
#[path = "tester_input/linux.rs"]
mod platform;

pub use platform::{TIMESTAMP_SOURCE, devices};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputDevice {
    pub id: String,
    pub label: String,
    pub identity: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputError {
    DeviceUnavailable,
    PermissionDenied,
    RegistrationConflict,
    Io(String),
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceUnavailable => write!(f, "Selected keyboard input is unavailable"),
            Self::PermissionDenied => write!(f, "Permission denied for selected keyboard input"),
            Self::RegistrationConflict => write!(
                f,
                "Keyboard input registration changed while the tester was active"
            ),
            Self::Io(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for InputError {}

#[derive(Clone, Debug, PartialEq)]
pub enum Sample {
    Input(Input),
    /// Lost, suppressed, or otherwise discontinuous input; clear held keys and timing.
    Discontinuity,
    Error(InputError),
}

const CAPACITY: usize = 512;

/// The owner consumes events without blocking Iced. Drop joins the short-lived
/// worker and ends collection; no events are saved or transmitted elsewhere.
pub struct Sampler {
    receiver: Receiver<Sample>,
    _worker: platform::Worker,
    terminal_reported: Cell<bool>,
}

impl Sampler {
    pub fn start(device: &InputDevice) -> Result<Self, InputError> {
        let (sender, receiver) = mpsc::sync_channel(CAPACITY);
        let worker = platform::Worker::start(device, sender)?;
        Ok(Self {
            receiver,
            _worker: worker,
            terminal_reported: Cell::new(false),
        })
    }

    pub fn try_recv(&self) -> Option<Sample> {
        match self.receiver.try_recv() {
            Ok(sample) => {
                if matches!(sample, Sample::Error(_)) {
                    self.terminal_reported.set(true);
                }
                Some(sample)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) if !self.terminal_reported.replace(true) => {
                Some(Sample::Error(InputError::DeviceUnavailable))
            }
            Err(TryRecvError::Disconnected) => None,
        }
    }

    pub fn stop(self) {}
}

/// On overflow, no later input may be trusted until the consumer sees a
/// discontinuity. The producer keeps the marker pending until capacity returns.
pub(crate) struct Sink {
    sender: SyncSender<Sample>,
    lost: bool,
}
impl Sink {
    fn new(sender: SyncSender<Sample>) -> Self {
        Self {
            sender,
            lost: false,
        }
    }

    fn flush_gap(&mut self) -> bool {
        if !self.lost {
            return true;
        }
        match self.sender.try_send(Sample::Discontinuity) {
            Ok(()) => {
                self.lost = false;
                true
            }
            Err(TrySendError::Full(_)) => true,
            Err(TrySendError::Disconnected(_)) => false,
        }
    }

    fn send(&mut self, sample: Sample) -> bool {
        if !self.flush_gap() {
            return false;
        }
        if self.lost {
            return true;
        }
        match self.sender.try_send(sample) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                self.lost = true;
                true
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }
}

fn spawn(name: &str, run: impl FnOnce() + Send + 'static) -> Result<JoinHandle<()>, InputError> {
    std::thread::Builder::new()
        .name(name.into())
        .spawn(run)
        .map_err(|error| InputError::Io(format!("Could not start keyboard input: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_marks_a_gap_before_any_later_key() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut sink = Sink::new(sender);
        assert!(sink.send(Sample::Discontinuity));
        assert!(sink.send(Sample::Discontinuity));
        assert_eq!(receiver.try_recv().unwrap(), Sample::Discontinuity);
        assert!(sink.send(Sample::Discontinuity));
        assert_eq!(receiver.try_recv().unwrap(), Sample::Discontinuity);
        assert!(sink.send(Sample::Discontinuity));
        assert_eq!(receiver.try_recv().unwrap(), Sample::Discontinuity);
    }
}
