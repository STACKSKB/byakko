//! A bounded finite-command worker owns one immutable selected device.
use crate::Device;
use byakko_core::contract::{
    ApplyFailure, Command, CommandPayload, Completion, CompletionPayload, FeatureCommand,
    FeatureResult, Recovery,
};
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

pub struct Executor {
    commands: SyncSender<Command>,
    completions: Receiver<Completion>,
    generation: Arc<AtomicU64>,
}

impl Executor {
    pub fn spawn(mut device: impl Device, backup_dir: PathBuf) -> std::io::Result<Self> {
        let (commands, requests) = mpsc::sync_channel::<Command>(1);
        let (responses, completions) = mpsc::sync_channel(1);
        let generation = Arc::new(AtomicU64::new(0));
        let active_generation = Arc::clone(&generation);
        std::thread::Builder::new()
            .name("byakko-device".into())
            .spawn(move || {
                let mut latest = None;
                while let Ok(command) = requests.recv() {
                    let token = (command.generation, command.operation);
                    let completion = if token.0 == 0
                        || token.0 != active_generation.load(Ordering::Acquire)
                        || latest.is_some_and(|previous| token <= previous)
                    {
                        failure(
                            &command,
                            "Stale or duplicate device command".into(),
                            Recovery::NotAttempted,
                        )
                    } else {
                        latest = Some(token);
                        execute(&mut device, &command, &backup_dir)
                    };
                    if responses.send(completion).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            commands,
            completions,
            generation,
        })
    }

    /// Does not interrupt a transaction already in progress.
    pub fn set_generation(&self, generation: u64) {
        self.generation.store(generation, Ordering::Release);
    }

    pub fn try_submit(&self, command: Command) -> Result<(), Box<Completion>> {
        self.commands.try_send(command).map_err(|error| {
            let (command, message) = match error {
                TrySendError::Full(command) => (command, "Device command queue is full"),
                TrySendError::Disconnected(command) => (command, "Device executor is closed"),
            };
            Box::new(failure(&command, message.into(), Recovery::NotAttempted))
        })
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
