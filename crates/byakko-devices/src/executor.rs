//! One bounded worker serializes commands against a connected device.
mod host;
pub use host::{HostEvent, HostFrameError, HostStopResult, HostSubmitError, HostTicket};

use crate::{Device, HostFrame};
use byakko_core::lighting::{self, HostMode};
use byakko_core::session::{ApplyFailure, Command, Completion, Recovery};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
};

enum Request {
    Finite(Command),
    HostStart {
        ticket: HostTicket,
        mode: Box<HostMode>,
        setting: Option<lighting::Setting>,
        expected: lighting::Snapshot,
    },
}

struct CatalogScan {
    generation: u64,
    operation: u64,
    slots: Vec<String>,
    snapshots: Vec<byakko_core::macros::Snapshot>,
}

impl CatalogScan {
    fn step(&mut self, device: &mut impl Device) -> Option<Completion> {
        let slot = &self.slots[self.snapshots.len()];
        let result = catch_unwind(AssertUnwindSafe(|| device.read_macro(slot)))
            .unwrap_or_else(|_| Err("Macro catalog device read panicked".into()));
        match result {
            Ok(snapshot) => self.snapshots.push(snapshot),
            Err(reason) => {
                return Some(Completion::ReadMacroCatalog {
                    generation: self.generation,
                    operation: self.operation,
                    result: Err(reason),
                });
            }
        }
        (self.snapshots.len() == self.slots.len()).then(|| Completion::ReadMacroCatalog {
            generation: self.generation,
            operation: self.operation,
            result: Ok(std::mem::take(&mut self.snapshots)),
        })
    }
}

/// Commands, finite completions, and host frames use bounded queues. Closing
/// the window must wait for restoration; dropping an executor requests Stop.
pub struct Executor {
    commands: SyncSender<Request>,
    pub(crate) completions: Receiver<Completion>,
    frames: SyncSender<host::Frame>,
    host_events: Receiver<HostEvent>,
    host: Arc<host::Control>,
    generation: Arc<AtomicU64>,
    catalog_submitted: AtomicU64,
    catalog_cancelled: Arc<AtomicU64>,
}

impl Executor {
    pub fn spawn(mut device: impl Device, backup_dir: PathBuf) -> std::io::Result<Self> {
        let (commands, requests) = mpsc::sync_channel::<Request>(1);
        let (responses, completions) = mpsc::sync_channel(1);
        let (frames, incoming_frames) = mpsc::sync_channel(1);
        // A Started event cannot block restoration if the caller stops polling.
        let (host_responses, host_events) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let active_generation = generation.clone();
        let catalog_cancelled = Arc::new(AtomicU64::new(0));
        let worker_catalog_cancelled = catalog_cancelled.clone();
        let host = Arc::new(host::Control::new());
        let active_host = host.clone();
        std::thread::Builder::new()
            .name("byakko-device".into())
            .spawn(move || {
                let mut latest = None;
                let mut scan: Option<CatalogScan> = None;
                loop {
                    let request = if scan.is_some() {
                        match requests.try_recv() {
                            Ok(request) => Some(request),
                            Err(TryRecvError::Empty) => None,
                            Err(TryRecvError::Disconnected) => break,
                        }
                    } else {
                        match requests.recv() {
                            Ok(request) => Some(request),
                            Err(_) => break,
                        }
                    };
                    let Some(request) = request else {
                        let pending = scan.as_mut().expect("scan remains active");
                        if pending.generation != active_generation.load(Ordering::Acquire)
                            || pending.operation <= worker_catalog_cancelled.load(Ordering::Acquire)
                        {
                            scan = None;
                            continue;
                        }
                        if let Some(completion) = pending.step(&mut device) {
                            scan = None;
                            if responses.send(completion).is_err() {
                                break;
                            }
                        }
                        continue;
                    };
                    match request {
                        Request::Finite(command) => {
                            let token = token(&command);
                            if token.0 == 0
                                || token.0 != active_generation.load(Ordering::Acquire)
                                || latest.is_some_and(|previous| token <= previous)
                            {
                                let completion = failure(
                                    &command,
                                    "Stale or duplicate device command".into(),
                                    Recovery::NotAttempted,
                                );
                                if responses.send(completion).is_err() {
                                    break;
                                }
                                continue;
                            }
                            latest = Some(token);
                            if let Command::ReadMacroCatalog {
                                generation,
                                operation,
                                slots,
                            } = command
                            {
                                if operation <= worker_catalog_cancelled.load(Ordering::Acquire) {
                                    continue;
                                }
                                scan = Some(CatalogScan {
                                    generation,
                                    operation,
                                    slots,
                                    snapshots: Vec::new(),
                                });
                                if scan.as_ref().is_some_and(|scan| scan.slots.is_empty()) {
                                    let completed = scan.take().unwrap();
                                    if responses
                                        .send(Completion::ReadMacroCatalog {
                                            generation: completed.generation,
                                            operation: completed.operation,
                                            result: Ok(Vec::new()),
                                        })
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                                continue;
                            }
                            if matches!(
                                command,
                                Command::Read { .. }
                                    | Command::Apply { .. }
                                    | Command::ApplyMacro { .. }
                                    | Command::ApplyLighting { .. }
                                    | Command::ApplyPicture { .. }
                                    | Command::ApplySetting { .. }
                                    | Command::ApplyArchive { .. }
                            ) {
                                scan = None;
                            }
                            let completion = execute(&mut device, &command, &backup_dir);
                            if responses.send(completion).is_err() {
                                break;
                            }
                        }
                        Request::HostStart {
                            ticket,
                            mode,
                            setting,
                            expected,
                        } => {
                            scan = None;
                            if ticket.generation != active_generation.load(Ordering::Acquire)
                                || latest.is_some_and(|previous| {
                                    (ticket.generation, ticket.operation) <= previous
                                })
                            {
                                active_host.clear();
                                let _ = host_responses.send(HostEvent::StartFailed {
                                    ticket,
                                    failure: ApplyFailure {
                                        message: "Stale or duplicate host activity".into(),
                                        recovery: Recovery::NotAttempted,
                                    },
                                });
                            } else {
                                latest = Some((ticket.generation, ticket.operation));
                                host::run(
                                    &mut device,
                                    *mode,
                                    setting,
                                    &expected,
                                    host::Run {
                                        ticket,
                                        backup_dir: &backup_dir,
                                        generation: &active_generation,
                                        control: &active_host,
                                        frames: &incoming_frames,
                                        events: &host_responses,
                                    },
                                );
                            }
                            active_host.clear();
                        }
                    }
                }
            })?;
        Ok(Self {
            commands,
            completions,
            frames,
            host_events,
            host,
            generation,
            catalog_submitted: AtomicU64::new(0),
            catalog_cancelled,
        })
    }

    /// Publish the core's connection generation; zero disables queued commands.
    /// This cannot interrupt a transaction that has already started.
    pub fn set_generation(&self, generation: u64) {
        self.generation.store(generation, Ordering::Release);
    }

    /// Stop a pending or active catalog sweep after its current slot. Recording
    /// stays local and does not wait for the USB slot already in progress.
    pub fn cancel_macro_catalog(&self) {
        self.catalog_cancelled.fetch_max(
            self.catalog_submitted.load(Ordering::Acquire),
            Ordering::AcqRel,
        );
    }

    /// Feed a rejected completion back into the core just like a worker result,
    /// so a full/closed queue cannot strand its pending operation.
    pub fn try_submit(&self, command: Command) -> Result<(), Box<Completion>> {
        if let Command::ReadMacroCatalog { operation, .. } = &command {
            self.catalog_submitted
                .fetch_max(*operation, Ordering::AcqRel);
        }
        let phase = self.host.phase.lock().unwrap();
        if !matches!(*phase, host::Phase::Idle) {
            return Err(Box::new(failure(
                &command,
                "Host lighting is active".into(),
                Recovery::NotAttempted,
            )));
        }
        self.commands
            .try_send(Request::Finite(command))
            .map_err(|error| {
                let (command, message) = match error {
                    TrySendError::Full(Request::Finite(command)) => {
                        (command, "Device command queue is full")
                    }
                    TrySendError::Disconnected(Request::Finite(command)) => {
                        (command, "Device executor is closed")
                    }
                    _ => unreachable!("only finite commands were submitted"),
                };
                Box::new(failure(&command, message.into(), Recovery::NotAttempted))
            })
    }

    pub fn try_start_host(
        &self,
        ticket: HostTicket,
        mode: HostMode,
        setting: Option<lighting::Setting>,
        expected: lighting::Snapshot,
    ) -> Result<(), HostSubmitError> {
        if ticket.generation == 0 || ticket.generation != self.generation.load(Ordering::Acquire) {
            return Err(HostSubmitError::Stale);
        }
        let mut phase = self.host.phase.lock().unwrap();
        if !matches!(*phase, host::Phase::Idle) {
            return Err(HostSubmitError::Busy);
        }
        *phase = host::Phase::Starting(ticket);
        if let Err(error) = self.commands.try_send(Request::HostStart {
            ticket,
            mode: Box::new(mode),
            setting,
            expected,
        }) {
            *phase = host::Phase::Idle;
            return Err(match error {
                TrySendError::Full(_) => HostSubmitError::QueueFull,
                TrySendError::Disconnected(_) => HostSubmitError::Closed,
            });
        }
        Ok(())
    }

    pub fn try_send_host_frame(
        &self,
        ticket: HostTicket,
        frame: HostFrame,
    ) -> Result<(), HostFrameError> {
        let phase = self.host.phase.lock().unwrap();
        match *phase {
            host::Phase::Streaming(active) if active == ticket => {}
            host::Phase::Starting(active) if active == ticket => {
                return Err(HostFrameError::NotReady);
            }
            host::Phase::StartStopping(active) | host::Phase::Stopping(active)
                if active == ticket =>
            {
                return Err(HostFrameError::Stopping);
            }
            host::Phase::Idle => return Err(HostFrameError::NotActive),
            _ => return Err(HostFrameError::WrongTicket),
        }
        self.frames
            .try_send(host::Frame {
                ticket,
                value: frame,
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => HostFrameError::QueueFull,
                TrySendError::Disconnected(_) => HostFrameError::Closed,
            })
    }

    pub fn try_stop_host(&self, ticket: HostTicket) -> HostStopResult {
        self.host.stop(ticket)
    }

    pub fn try_receive_host(&self) -> Result<HostEvent, TryRecvError> {
        self.host_events.try_recv()
    }

    pub fn try_receive(&self) -> Result<Completion, TryRecvError> {
        self.completions.try_recv()
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        self.generation.store(0, Ordering::Release);
        if let Ok(phase) = self.host.phase.lock() {
            let ticket = match *phase {
                host::Phase::Starting(ticket)
                | host::Phase::StartStopping(ticket)
                | host::Phase::Streaming(ticket)
                | host::Phase::Stopping(ticket) => Some(ticket),
                host::Phase::Idle => None,
            };
            drop(phase);
            if let Some(ticket) = ticket {
                self.host.stop(ticket);
            }
        }
    }
}

fn token(command: &Command) -> (u64, u64) {
    match command {
        Command::Read {
            generation,
            operation,
        }
        | Command::Apply {
            generation,
            operation,
            ..
        }
        | Command::ReadMacro {
            generation,
            operation,
            ..
        }
        | Command::ReadMacroCatalog {
            generation,
            operation,
            ..
        }
        | Command::ApplyMacro {
            generation,
            operation,
            ..
        }
        | Command::ReadLighting {
            generation,
            operation,
        }
        | Command::ApplyLighting {
            generation,
            operation,
            ..
        } => (*generation, *operation),
        Command::ReadPicture {
            generation,
            operation,
        }
        | Command::ApplyPicture {
            generation,
            operation,
            ..
        } => (*generation, *operation),
        Command::ReadSettings {
            generation,
            operation,
        }
        | Command::ApplySetting {
            generation,
            operation,
            ..
        } => (*generation, *operation),
        Command::CaptureArchive {
            generation,
            operation,
        }
        | Command::ReviewArchive {
            generation,
            operation,
            ..
        } => (*generation, *operation),
        Command::ApplyArchive {
            generation,
            operation,
            ..
        } => (*generation, *operation),
    }
}

fn failure(command: &Command, message: String, recovery: Recovery) -> Completion {
    let (generation, operation) = token(command);
    match command {
        Command::Read { .. } => Completion::Read {
            generation,
            operation,
            result: Err(message),
        },
        Command::Apply { .. } => Completion::Apply {
            generation,
            operation,
            result: Err(ApplyFailure { message, recovery }),
        },
        Command::ReadMacro { slot, .. } => Completion::ReadMacro {
            generation,
            operation,
            slot: slot.clone(),
            result: Err(message),
        },
        Command::ReadMacroCatalog { .. } => Completion::ReadMacroCatalog {
            generation,
            operation,
            result: Err(message),
        },
        Command::ApplyMacro { expected, .. } => Completion::ApplyMacro {
            generation,
            operation,
            slot: expected.slot.clone(),
            result: Err(ApplyFailure { message, recovery }),
        },
        Command::ReadLighting { .. } => Completion::ReadLighting {
            generation,
            operation,
            result: Err(message),
        },
        Command::ApplyLighting { .. } => Completion::ApplyLighting {
            generation,
            operation,
            result: Err(ApplyFailure { message, recovery }),
        },
        Command::ReadPicture { .. } => Completion::ReadPicture {
            generation,
            operation,
            result: Err(message),
        },
        Command::ApplyPicture { .. } => Completion::ApplyPicture {
            generation,
            operation,
            result: Err(ApplyFailure { message, recovery }),
        },
        Command::ReadSettings { .. } => Completion::ReadSettings {
            generation,
            operation,
            result: Err(message),
        },
        Command::ApplySetting { .. } => Completion::ApplySetting {
            generation,
            operation,
            result: Err(ApplyFailure { message, recovery }),
        },
        Command::CaptureArchive { .. } => Completion::CaptureArchive {
            generation,
            operation,
            result: Err(message),
        },
        Command::ReviewArchive { .. } => Completion::ReviewArchive {
            generation,
            operation,
            result: Err(message),
        },
        Command::ApplyArchive { .. } => Completion::ApplyArchive {
            generation,
            operation,
            result: Err(ApplyFailure { message, recovery }),
        },
    }
}

fn execute(device: &mut impl Device, command: &Command, backup_dir: &Path) -> Completion {
    let (generation, operation) = token(command);
    catch_unwind(AssertUnwindSafe(|| match command {
        Command::Read { .. } => Completion::Read {
            generation,
            operation,
            result: device.read(),
        },
        Command::Apply {
            expected, changes, ..
        } => Completion::Apply {
            generation,
            operation,
            result: device.apply(expected, changes, backup_dir),
        },
        Command::ReadMacro { slot, .. } => Completion::ReadMacro {
            generation,
            operation,
            slot: slot.clone(),
            result: device.read_macro(slot),
        },
        Command::ReadMacroCatalog { slots, .. } => Completion::ReadMacroCatalog {
            generation,
            operation,
            result: device.read_macro_catalog(slots),
        },
        Command::ApplyMacro {
            expected, desired, ..
        } => Completion::ApplyMacro {
            generation,
            operation,
            slot: expected.slot.clone(),
            result: device.apply_macro(expected, desired, backup_dir),
        },
        Command::ReadLighting { .. } => Completion::ReadLighting {
            generation,
            operation,
            result: device.read_lighting(),
        },
        Command::ApplyLighting {
            expected, desired, ..
        } => Completion::ApplyLighting {
            generation,
            operation,
            result: device.apply_lighting(expected, desired, backup_dir),
        },
        Command::ReadPicture { .. } => Completion::ReadPicture {
            generation,
            operation,
            result: device.read_picture(),
        },
        Command::ApplyPicture {
            expected, desired, ..
        } => Completion::ApplyPicture {
            generation,
            operation,
            result: device.apply_picture(expected, desired, backup_dir),
        },
        Command::ReadSettings { .. } => Completion::ReadSettings {
            generation,
            operation,
            result: device.read_settings(),
        },
        Command::ApplySetting { expected, edit, .. } => Completion::ApplySetting {
            generation,
            operation,
            result: device.apply_setting(expected, edit, backup_dir),
        },
        Command::CaptureArchive { .. } => Completion::CaptureArchive {
            generation,
            operation,
            result: device.capture_archive(),
        },
        Command::ReviewArchive { target, .. } => Completion::ReviewArchive {
            generation,
            operation,
            result: device.review_archive(target),
        },
        Command::ApplyArchive {
            expected, target, ..
        } => Completion::ApplyArchive {
            generation,
            operation,
            result: device.apply_archive(expected, target, backup_dir),
        },
    }))
    .unwrap_or_else(|_| {
        failure(
            command,
            "Device executor panicked; state is unverified".into(),
            Recovery::Unverified,
        )
    })
}

#[cfg(test)]
#[path = "executor/host_tests.rs"]
mod host_tests;
#[cfg(test)]
mod tests;
