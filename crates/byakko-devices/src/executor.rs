//! A bounded finite-command worker owns one immutable selected device.
use crate::Device;
mod host;
use byakko_core::contract::{
    ApplyFailure, Command, CommandPayload, Completion, CompletionPayload, FeatureCommand,
    FeatureResult, HostEvent, HostStart, HostTicket, Recovery,
};
use host::Host;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError},
    },
    time::Duration,
};

struct Catalog {
    generation: u64,
    operation: u64,
    slots: Vec<String>,
    snapshots: Vec<byakko_core::model::macros::Snapshot>,
}
impl Catalog {
    fn finish(
        self,
        result: Result<Vec<byakko_core::model::macros::Snapshot>, String>,
    ) -> Completion {
        Completion {
            generation: self.generation,
            operation: self.operation,
            payload: CompletionPayload::ReadMacroCatalog { result },
        }
    }
}
pub struct Executor {
    commands: SyncSender<Request>,
    completions: Receiver<Completion>,
    generation: Arc<AtomicU64>,
    catalog_submitted: AtomicU64,
    catalog_cancelled: Arc<AtomicU64>,
    host: Arc<Host>,
}

enum Request {
    Finite(Box<Command>),
    Host(Box<HostStart>),
}

impl Executor {
    pub fn spawn(mut device: impl Device, backup_dir: PathBuf) -> std::io::Result<Self> {
        // One passive scan and one foreground ticket may be queued together.
        let (commands, requests) = mpsc::sync_channel::<Request>(2);
        let (responses, completions) = mpsc::sync_channel(1);
        let generation = Arc::new(AtomicU64::new(0));
        let active_generation = Arc::clone(&generation);
        let catalog_cancelled = Arc::new(AtomicU64::new(0));
        let worker_cancelled = Arc::clone(&catalog_cancelled);
        let host = Arc::new(Host::default());
        let worker_host = Arc::clone(&host);
        std::thread::Builder::new()
            .name("byakko-device".into())
            .spawn(move || {
                let mut latest = None;
                let mut catalog: Option<Catalog> = None;
                loop {
                    let request = if catalog.is_some() {
                        match requests.try_recv() {
                            Ok(command) => Some(command),
                            Err(TryRecvError::Empty) => None,
                            Err(TryRecvError::Disconnected) => break,
                        }
                    } else {
                        match requests.recv() {
                            Ok(command) => Some(command),
                            Err(_) => break,
                        }
                    };
                    if let Some(Request::Host(start)) = request {
                        if let Some(scan) = catalog.take() {
                            let _ =
                                responses.send(scan.finish(Err(
                                    "Macro catalog cancelled for host lighting".into(),
                                )));
                        }
                        let token = (start.ticket.generation, start.ticket.operation);
                        if token.0 == 0
                            || token.0 != active_generation.load(Ordering::Acquire)
                            || latest.is_some_and(|previous| token <= previous)
                        {
                            worker_host.reject(
                                start.ticket,
                                "Stale or duplicate host command",
                                Recovery::NotAttempted,
                            );
                        } else {
                            latest = Some(token);
                            worker_host.run(&mut device, *start, &backup_dir, &active_generation);
                        }
                        continue;
                    }
                    if let Some(Request::Finite(command)) = request {
                        if worker_host.pending() {
                            if responses
                                .send(failure(
                                    &command,
                                    "Host lighting owns the device".into(),
                                    Recovery::NotAttempted,
                                ))
                                .is_err()
                            {
                                break;
                            }
                            continue;
                        }
                        let token = (command.generation, command.operation);
                        if token.0 == 0
                            || token.0 != active_generation.load(Ordering::Acquire)
                            || latest.is_some_and(|previous| token <= previous)
                        {
                            if responses
                                .send(failure(
                                    &command,
                                    "Stale or duplicate device command".into(),
                                    Recovery::NotAttempted,
                                ))
                                .is_err()
                            {
                                break;
                            }
                            continue;
                        }
                        latest = Some(token);
                        if let CommandPayload::ReadMacroCatalog { slots } = &command.payload {
                            if let Some(previous) = catalog.take()
                                && responses
                                    .send(previous.finish(Err("Macro catalog replaced".into())))
                                    .is_err()
                            {
                                break;
                            }
                            catalog = Some(Catalog {
                                generation: command.generation,
                                operation: command.operation,
                                slots: slots.clone(),
                                snapshots: Vec::new(),
                            });
                            continue;
                        }
                        let macro_write = matches!(
                            &command.payload,
                            CommandPayload::Macro(FeatureCommand::Apply { .. })
                        );
                        let completion = execute(&mut device, &command, &backup_dir);
                        let failed_write = failed_write(&completion.payload);
                        if responses.send(completion).is_err() {
                            break;
                        }
                        if (macro_write || failed_write)
                            && let Some(previous) = catalog.take()
                            && responses
                                .send(
                                    previous
                                        .finish(Err("Macro catalog invalidated by write".into())),
                                )
                                .is_err()
                        {
                            break;
                        }
                    } else if let Some(scan) = catalog.as_mut() {
                        let cancelled = scan.generation
                            != active_generation.load(Ordering::Acquire)
                            || scan.operation <= worker_cancelled.load(Ordering::Acquire);
                        let finished = if cancelled {
                            Some(Err("Macro catalog cancelled".into()))
                        } else if scan.snapshots.len() == scan.slots.len() {
                            Some(Ok(std::mem::take(&mut scan.snapshots)))
                        } else {
                            let slot = &scan.slots[scan.snapshots.len()];
                            match catch_unwind(AssertUnwindSafe(|| device.read_macro(slot)))
                                .unwrap_or_else(
                                    |_| Err("Macro catalog device read panicked".into()),
                                ) {
                                Ok(snapshot) => {
                                    scan.snapshots.push(snapshot);
                                    None
                                }
                                Err(reason) => Some(Err(reason)),
                            }
                        };
                        if let Some(result) = finished {
                            let scan = catalog.take().expect("active catalog");
                            if responses.send(scan.finish(result)).is_err() {
                                break;
                            }
                        }
                    }
                }
            })?;
        Ok(Self {
            commands,
            completions,
            generation,
            catalog_submitted: AtomicU64::new(0),
            catalog_cancelled,
            host,
        })
    }

    /// Does not interrupt a transaction already in progress.
    pub fn set_generation(&self, generation: u64) {
        self.generation.store(generation, Ordering::Release);
        self.host.generation_changed();
    }

    pub fn try_submit(&self, command: Command) -> Result<(), Box<Completion>> {
        if self.host.pending() {
            return Err(Box::new(failure(
                &command,
                "Host lighting owns the device".into(),
                Recovery::NotAttempted,
            )));
        }
        if matches!(command.payload, CommandPayload::ReadMacroCatalog { .. }) {
            self.catalog_submitted
                .fetch_max(command.operation, Ordering::AcqRel);
        }
        self.commands
            .try_send(Request::Finite(Box::new(command)))
            .map_err(|error| {
                let (command, message) = match error {
                    TrySendError::Full(Request::Finite(command)) => {
                        (command, "Device command queue is full")
                    }
                    TrySendError::Disconnected(Request::Finite(command)) => {
                        (command, "Device executor is closed")
                    }
                    _ => unreachable!("submitted finite request"),
                };
                Box::new(failure(&command, message.into(), Recovery::NotAttempted))
            })
    }

    // Return the correlated terminal event directly, matching the host completion API.
    #[allow(clippy::result_large_err)]
    pub fn start_host(&self, start: HostStart) -> Result<(), HostEvent> {
        self.host
            .reserve(start.ticket, self.generation.load(Ordering::Acquire))?;
        self.cancel_catalog();
        let ticket = start.ticket;
        if self
            .commands
            .try_send(Request::Host(Box::new(start)))
            .is_err()
        {
            self.host.unreserve();
            return Err(host::rejected(
                ticket,
                "Device executor queue unavailable",
                Recovery::NotAttempted,
            ));
        }
        Ok(())
    }
    pub fn send_host_frame(
        &self,
        ticket: HostTicket,
        frame: crate::HostFrame,
    ) -> Result<(), String> {
        if ticket.generation != self.generation.load(Ordering::Acquire) {
            return Err("Stale host lighting generation".into());
        }
        self.host.frame(ticket, frame)
    }
    pub fn stop_host(&self, ticket: HostTicket, problem: Option<String>) {
        self.host.stop(ticket, problem);
    }
    pub fn try_receive_host(&self) -> Result<HostEvent, TryRecvError> {
        self.host.receive()
    }

    /// Stop a submitted scan after its current slot and emit a correlated error.
    pub fn cancel_catalog(&self) {
        self.catalog_cancelled.fetch_max(
            self.catalog_submitted.load(Ordering::Acquire),
            Ordering::AcqRel,
        );
    }

    pub fn try_receive(&self) -> Result<Completion, TryRecvError> {
        self.completions.try_recv()
    }
    pub fn receive(&self, timeout: Option<Duration>) -> Result<Completion, RecvTimeoutError> {
        match timeout {
            Some(timeout) => self.completions.recv_timeout(timeout),
            None => self
                .completions
                .recv()
                .map_err(|_| RecvTimeoutError::Disconnected),
        }
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        self.generation.store(0, Ordering::Release);
        self.host.close();
    }
}
/// One dispatcher handles execution, preflight rejection, and panic completion.
/// The rejected path cannot invoke a device closure.
enum DeviceCall<'a> {
    Run {
        device: &'a mut dyn Device,
        backup: &'a Path,
    },
    Rejected(ApplyFailure),
}

impl DeviceCall<'_> {
    fn read<T>(
        &mut self,
        read: impl FnOnce(&mut dyn Device) -> Result<T, String>,
    ) -> Result<T, String> {
        match self {
            Self::Run { device, .. } => read(*device),
            Self::Rejected(failure) => Err(failure.message.clone()),
        }
    }

    fn apply<T>(
        &mut self,
        apply: impl FnOnce(&mut dyn Device, &Path) -> Result<T, ApplyFailure>,
    ) -> Result<T, ApplyFailure> {
        match self {
            Self::Run { device, backup } => apply(*device, backup),
            Self::Rejected(failure) => Err(failure.clone()),
        }
    }

    fn feature<S, E, R>(
        &mut self,
        command: &FeatureCommand<S, E, R>,
        read: impl FnOnce(&mut dyn Device, &R) -> Result<S, String>,
        apply: impl FnOnce(&mut dyn Device, &S, &E, &Path) -> Result<S, ApplyFailure>,
        wrap: impl FnOnce(FeatureResult<S>) -> CompletionPayload,
    ) -> CompletionPayload {
        let result = match command {
            FeatureCommand::Read(input) => {
                FeatureResult::Read(self.read(|device| read(device, input)))
            }
            FeatureCommand::Apply { expected, desired } => FeatureResult::Apply(
                self.apply(|device, backup| apply(device, expected, desired, backup)),
            ),
        };
        wrap(result)
    }

    fn dispatch(&mut self, command: &Command) -> Completion {
        let payload = match &command.payload {
            CommandPayload::Keymap(request) => self.feature(
                request,
                |d, ()| d.read(),
                |d, expected, desired, backup| d.apply(expected, desired, backup),
                CompletionPayload::Keymap,
            ),
            CommandPayload::Macro(request) => {
                let slot = match request {
                    FeatureCommand::Read(slot) => slot.clone(),
                    FeatureCommand::Apply { expected, .. } => expected.slot.clone(),
                };
                self.feature(
                    request,
                    |d, slot| d.read_macro(slot),
                    |d, expected, desired, backup| d.apply_macro(expected, desired, backup),
                    |result| CompletionPayload::Macro { slot, result },
                )
            }
            CommandPayload::Lighting(request) => self.feature(
                request,
                |d, ()| d.read_lighting(),
                |d, expected, desired, backup| d.apply_lighting(expected, desired, backup),
                CompletionPayload::Lighting,
            ),
            CommandPayload::Picture(request) => self.feature(
                request,
                |d, ()| d.read_picture(),
                |d, expected, desired, backup| d.apply_picture(expected, desired, backup),
                CompletionPayload::Picture,
            ),
            CommandPayload::Settings(request) => self.feature(
                request,
                |d, ()| d.read_settings(),
                |d, expected, desired, backup| d.apply_setting(expected, desired, backup),
                CompletionPayload::Settings,
            ),
            CommandPayload::Archive(request) => self.feature(
                request,
                |d, ()| d.capture_archive(),
                |d, expected, desired, backup| d.apply_archive(expected, desired, backup),
                CompletionPayload::Archive,
            ),
            CommandPayload::ReadMacroCatalog { slots } => CompletionPayload::ReadMacroCatalog {
                result: self.read(|d| d.read_macro_catalog(slots)),
            },
            CommandPayload::ReviewArchive { target } => CompletionPayload::ReviewArchive {
                result: self.read(|d| d.review_archive(target)),
            },
        };
        Completion {
            generation: command.generation,
            operation: command.operation,
            payload,
        }
    }
}

fn failed_write(payload: &CompletionPayload) -> bool {
    match payload {
        CompletionPayload::Keymap(result) => result.failed_write(),
        CompletionPayload::Macro { result, .. } => result.failed_write(),
        CompletionPayload::Lighting(result) => result.failed_write(),
        CompletionPayload::Picture(result) => result.failed_write(),
        CompletionPayload::Settings(result) => result.failed_write(),
        CompletionPayload::Archive(result) => result.failed_write(),
        CompletionPayload::ReadMacroCatalog { .. } | CompletionPayload::ReviewArchive { .. } => {
            false
        }
    }
}
fn failure(command: &Command, message: String, recovery: Recovery) -> Completion {
    DeviceCall::Rejected(ApplyFailure { message, recovery }).dispatch(command)
}

fn execute(device: &mut dyn Device, command: &Command, backup: &Path) -> Completion {
    catch_unwind(AssertUnwindSafe(|| {
        DeviceCall::Run { device, backup }.dispatch(command)
    }))
    .unwrap_or_else(|_| {
        DeviceCall::Rejected(ApplyFailure {
            message: "Device executor panicked; state is unverified".into(),
            recovery: Recovery::Unverified,
        })
        .dispatch(command)
    })
}

#[cfg(test)]
mod tests;
