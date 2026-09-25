//! One bounded worker serializes commands against a connected device.
use byakko_core::session::{CommandPayload, CompletionPayload};
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
                return Some(Completion {
                    generation: self.generation,
                    operation: self.operation,
                    payload: CompletionPayload::ReadMacroCatalog {
                        result: Err(reason),
                    },
                });
            }
        }
        (self.snapshots.len() == self.slots.len()).then(|| Completion {
            generation: self.generation,
            operation: self.operation,
            payload: CompletionPayload::ReadMacroCatalog {
                result: Ok(std::mem::take(&mut self.snapshots)),
            },
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
        // One passive catalog scan may be queued alongside a foreground command.
        let (commands, requests) = mpsc::sync_channel::<Request>(2);
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
                            let token = (command.generation, command.operation);
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
                            if let Command {
                                generation,
                                operation,
                                payload: CommandPayload::ReadMacroCatalog { slots },
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
                                        .send(Completion {
                                            generation: completed.generation,
                                            operation: completed.operation,
                                            payload: CompletionPayload::ReadMacroCatalog {
                                                result: Ok(Vec::new()),
                                            },
                                        })
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                                continue;
                            }
                            // Match the session's catalog invalidation policy. Unrelated
                            // foreground operations pause discovery between slots.
                            if matches!(
                                &command.payload,
                                CommandPayload::ApplyMacro { .. }
                                    | CommandPayload::ApplyArchive { .. }
                            ) {
                                scan = None;
                            }
                            let completion = execute(&mut device, &command, &backup_dir);
                            if failed_write(&completion.payload) {
                                scan = None;
                            }
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
        if let Command {
            operation,
            payload: CommandPayload::ReadMacroCatalog { .. },
            ..
        } = &command
        {
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

fn failed_write(payload: &CompletionPayload) -> bool {
    match payload {
        CompletionPayload::Apply { result } => result.is_err(),
        CompletionPayload::ApplyMacro { result, .. } => result.is_err(),
        CompletionPayload::ApplyLighting { result } => result.is_err(),
        CompletionPayload::ApplyPicture { result } => result.is_err(),
        CompletionPayload::ApplySetting { result } => result.is_err(),
        CompletionPayload::ApplyArchive { result } => result.is_err(),
        CompletionPayload::Read { .. }
        | CompletionPayload::ReadMacro { .. }
        | CompletionPayload::ReadMacroCatalog { .. }
        | CompletionPayload::ReadLighting { .. }
        | CompletionPayload::ReadPicture { .. }
        | CompletionPayload::ReadSettings { .. }
        | CompletionPayload::CaptureArchive { .. }
        | CompletionPayload::ReviewArchive { .. } => false,
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

fn failure(command: &Command, message: String, recovery: Recovery) -> Completion {
    let payload = match &command.payload {
        CommandPayload::Read { .. } => CompletionPayload::Read {
            result: Err(message),
        },
        CommandPayload::Apply { .. } => CompletionPayload::Apply {
            result: Err(ApplyFailure { message, recovery }),
        },
        CommandPayload::ReadMacro { slot, .. } => CompletionPayload::ReadMacro {
            slot: slot.clone(),
            result: Err(message),
        },
        CommandPayload::ReadMacroCatalog { .. } => CompletionPayload::ReadMacroCatalog {
            result: Err(message),
        },
        CommandPayload::ApplyMacro { expected, .. } => CompletionPayload::ApplyMacro {
            slot: expected.slot.clone(),
            result: Err(ApplyFailure { message, recovery }),
        },
        CommandPayload::ReadLighting { .. } => CompletionPayload::ReadLighting {
            result: Err(message),
        },
        CommandPayload::ApplyLighting { .. } => CompletionPayload::ApplyLighting {
            result: Err(ApplyFailure { message, recovery }),
        },
        CommandPayload::ReadPicture { .. } => CompletionPayload::ReadPicture {
            result: Err(message),
        },
        CommandPayload::ApplyPicture { .. } => CompletionPayload::ApplyPicture {
            result: Err(ApplyFailure { message, recovery }),
        },
        CommandPayload::ReadSettings { .. } => CompletionPayload::ReadSettings {
            result: Err(message),
        },
        CommandPayload::ApplySetting { .. } => CompletionPayload::ApplySetting {
            result: Err(ApplyFailure { message, recovery }),
        },
        CommandPayload::CaptureArchive { .. } => CompletionPayload::CaptureArchive {
            result: Err(message),
        },
        CommandPayload::ReviewArchive { .. } => CompletionPayload::ReviewArchive {
            result: Err(message),
        },
        CommandPayload::ApplyArchive { .. } => CompletionPayload::ApplyArchive {
            result: Err(ApplyFailure { message, recovery }),
        },
    };
    Completion {
        generation: command.generation,
        operation: command.operation,
        payload,
    }
}

fn execute(device: &mut impl Device, command: &Command, backup_dir: &Path) -> Completion {
    let result = catch_unwind(AssertUnwindSafe(|| match &command.payload {
        CommandPayload::Read {} => CompletionPayload::Read {
            result: device.read(),
        },
        CommandPayload::Apply { expected, changes } => CompletionPayload::Apply {
            result: device.apply(expected, changes, backup_dir),
        },
        CommandPayload::ReadMacro { slot } => CompletionPayload::ReadMacro {
            slot: slot.clone(),
            result: device.read_macro(slot),
        },
        CommandPayload::ReadMacroCatalog { slots } => CompletionPayload::ReadMacroCatalog {
            result: device.read_macro_catalog(slots),
        },
        CommandPayload::ApplyMacro { expected, desired } => CompletionPayload::ApplyMacro {
            slot: expected.slot.clone(),
            result: device.apply_macro(expected, desired, backup_dir),
        },
        CommandPayload::ReadLighting {} => CompletionPayload::ReadLighting {
            result: device.read_lighting(),
        },
        CommandPayload::ApplyLighting { expected, desired } => CompletionPayload::ApplyLighting {
            result: device.apply_lighting(expected, desired, backup_dir),
        },
        CommandPayload::ReadPicture {} => CompletionPayload::ReadPicture {
            result: device.read_picture(),
        },
        CommandPayload::ApplyPicture { expected, desired } => CompletionPayload::ApplyPicture {
            result: device.apply_picture(expected, desired, backup_dir),
        },
        CommandPayload::ReadSettings {} => CompletionPayload::ReadSettings {
            result: device.read_settings(),
        },
        CommandPayload::ApplySetting { expected, edit } => CompletionPayload::ApplySetting {
            result: device.apply_setting(expected, edit, backup_dir),
        },
        CommandPayload::CaptureArchive {} => CompletionPayload::CaptureArchive {
            result: device.capture_archive(),
        },
        CommandPayload::ReviewArchive { target } => CompletionPayload::ReviewArchive {
            result: device.review_archive(target),
        },
        CommandPayload::ApplyArchive { expected, target } => CompletionPayload::ApplyArchive {
            result: device.apply_archive(expected, target, backup_dir),
        },
    }));
    match result {
        Ok(payload) => Completion {
            generation: command.generation,
            operation: command.operation,
            payload,
        },
        Err(_) => failure(
            command,
            "Device executor panicked; state is unverified".into(),
            Recovery::Unverified,
        ),
    }
}

#[cfg(test)]
#[path = "executor/host_tests.rs"]
mod host_tests;
#[cfg(test)]
mod tests;
