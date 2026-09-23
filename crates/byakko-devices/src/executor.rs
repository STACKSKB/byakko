//! One bounded worker serializes commands against a connected device.
use crate::Device;
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

/// Both queues are bounded. Closing the window must wait for an outstanding
/// operation: dropping the executor cannot cancel a write already in progress.
pub struct Executor {
    commands: SyncSender<Command>,
    pub(crate) completions: Receiver<Completion>,
    generation: Arc<AtomicU64>,
}

impl Executor {
    pub fn spawn(mut device: impl Device, backup_dir: PathBuf) -> std::io::Result<Self> {
        let (commands, requests) = mpsc::sync_channel::<Command>(1);
        let (responses, completions) = mpsc::sync_channel(1);
        let generation = Arc::new(AtomicU64::new(0));
        let active_generation = generation.clone();
        std::thread::Builder::new()
            .name("byakko-device".into())
            .spawn(move || {
                let mut latest = None;
                for command in requests {
                    let token = token(&command);
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

    /// Publish the core's connection generation; zero disables queued commands.
    /// This cannot interrupt a transaction that has already started.
    pub fn set_generation(&self, generation: u64) {
        self.generation.store(generation, Ordering::Release);
    }

    /// Feed a rejected completion back into the core just like a worker result,
    /// so a full/closed queue cannot strand its pending operation.
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
}

impl Drop for Executor {
    fn drop(&mut self) {
        self.generation.store(0, Ordering::Release);
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
mod tests;
