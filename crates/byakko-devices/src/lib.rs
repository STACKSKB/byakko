//! Native effect boundary. One worker owns the backend; the UI owns no HID I/O.
use byakko_core::{
    Change, State, lighting, macros, picture,
    session::{ApplyFailure, Command, Completion, Recovery},
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
};

pub mod hid;
pub mod macro_files;
pub mod memory;
pub mod nia87;
#[cfg(feature = "research-tools")]
pub mod research_fault;
#[cfg(feature = "research-tools")]
pub mod research_trace;
pub mod storage;

/// Implementations must validate expected state, back up, write and verify.
/// Success means verified device state, not merely successful transmission.
pub trait Device: Send + 'static {
    fn read(&mut self) -> Result<State, String>;
    fn apply(
        &mut self,
        expected: &State,
        changes: &[Change],
        backup_dir: &Path,
    ) -> Result<State, ApplyFailure>;

    fn read_macro(&mut self, _slot: &str) -> Result<macros::Snapshot, String> {
        Err("Macro operations are unsupported by this device".into())
    }

    fn apply_macro(
        &mut self,
        _expected: &macros::Snapshot,
        _desired: &macros::Program,
        _backup_dir: &Path,
    ) -> Result<macros::Snapshot, ApplyFailure> {
        Err(ApplyFailure {
            message: "Macro operations are unsupported by this device".into(),
            recovery: Recovery::NotAttempted,
        })
    }

    fn read_lighting(&mut self) -> Result<lighting::Snapshot, String> {
        Err("Lighting operations are unsupported by this device".into())
    }

    fn apply_lighting(
        &mut self,
        _expected: &lighting::Snapshot,
        _desired: &lighting::Setting,
        _backup_dir: &Path,
    ) -> Result<lighting::Snapshot, ApplyFailure> {
        Err(ApplyFailure {
            message: "Lighting operations are unsupported by this device".into(),
            recovery: Recovery::NotAttempted,
        })
    }

    fn read_picture(&mut self) -> Result<picture::Snapshot, String> {
        Err("Picture operations are unsupported by this device".into())
    }

    fn apply_picture(
        &mut self,
        _expected: &picture::Snapshot,
        _desired: &std::collections::BTreeMap<String, [u8; 3]>,
        _backup_dir: &Path,
    ) -> Result<picture::Snapshot, ApplyFailure> {
        Err(ApplyFailure {
            message: "Picture operations are unsupported by this device".into(),
            recovery: Recovery::NotAttempted,
        })
    }
}

pub use Device as KeymapDevice;

/// Both queues are bounded. Closing the window must wait for an outstanding
/// operation: dropping the executor cannot cancel a write already in progress.
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
mod tests {
    use super::*;
    use std::{
        collections::BTreeMap,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
            mpsc::{self, Receiver, Sender},
        },
        time::Duration,
    };

    struct MemoryDevice {
        state: State,
        writes: Arc<AtomicUsize>,
    }

    struct PanickingDevice;
    impl KeymapDevice for PanickingDevice {
        fn read(&mut self) -> Result<State, String> {
            panic!("read panic")
        }
        fn apply(&mut self, _: &State, _: &[Change], _: &Path) -> Result<State, ApplyFailure> {
            panic!("write panic")
        }
    }

    #[test]
    fn panicked_write_returns_correlated_unverified_completion() {
        let worker = Executor::spawn(PanickingDevice, PathBuf::new()).unwrap();
        worker.set_generation(7);
        worker
            .try_submit(Command::Apply {
                generation: 7,
                operation: 9,
                expected: State {
                    revision: vec![],
                    bindings: BTreeMap::new(),
                },
                changes: vec![],
            })
            .unwrap();
        assert!(matches!(
            worker
                .completions
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            Completion::Apply {
                generation: 7,
                operation: 9,
                result: Err(ApplyFailure {
                    recovery: Recovery::Unverified,
                    ..
                })
            }
        ));
    }
    impl KeymapDevice for MemoryDevice {
        fn read(&mut self) -> Result<State, String> {
            Ok(self.state.clone())
        }
        fn apply(
            &mut self,
            expected: &State,
            _: &[Change],
            _: &Path,
        ) -> Result<State, ApplyFailure> {
            if expected != &self.state {
                return Err(ApplyFailure {
                    message: "Conflict".into(),
                    recovery: Recovery::NotAttempted,
                });
            }
            self.writes.fetch_add(1, Ordering::SeqCst);
            Ok(self.state.clone())
        }
    }

    #[test]
    fn worker_preserves_tokens_and_rejects_duplicate_writes_before_device_access() {
        let writes = Arc::new(AtomicUsize::new(0));
        let state = State {
            revision: vec![42],
            bindings: BTreeMap::new(),
        };
        let worker = Executor::spawn(
            MemoryDevice {
                state: state.clone(),
                writes: writes.clone(),
            },
            PathBuf::new(),
        )
        .unwrap();
        worker.set_generation(1);
        worker
            .try_submit(Command::Read {
                generation: 1,
                operation: 1,
            })
            .unwrap();
        assert_eq!(
            worker
                .completions
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            Completion::Read {
                generation: 1,
                operation: 1,
                result: Ok(state.clone())
            }
        );
        let apply = Command::Apply {
            generation: 1,
            operation: 2,
            expected: state,
            changes: vec![],
        };
        worker.try_submit(apply.clone()).unwrap();
        assert!(matches!(
            worker
                .completions
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            Completion::Apply { result: Ok(_), .. }
        ));
        worker.try_submit(apply).unwrap();
        assert!(matches!(
            worker
                .completions
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            Completion::Apply {
                result: Err(ApplyFailure {
                    recovery: Recovery::NotAttempted,
                    ..
                }),
                ..
            }
        ));
        assert_eq!(writes.load(Ordering::SeqCst), 1);
    }

    struct BlockingDevice {
        state: State,
        writes: Arc<AtomicUsize>,
        entered: Sender<()>,
        release: Receiver<()>,
    }

    impl KeymapDevice for BlockingDevice {
        fn read(&mut self) -> Result<State, String> {
            self.entered.send(()).unwrap();
            self.release
                .recv_timeout(Duration::from_secs(2))
                .map_err(|error| error.to_string())?;
            Ok(self.state.clone())
        }

        fn apply(&mut self, _: &State, _: &[Change], _: &Path) -> Result<State, ApplyFailure> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            Ok(self.state.clone())
        }
    }

    fn blocked_worker() -> (Executor, Sender<()>, Arc<AtomicUsize>, State) {
        let state = State {
            revision: vec![1],
            bindings: BTreeMap::new(),
        };
        let writes = Arc::new(AtomicUsize::new(0));
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = Executor::spawn(
            BlockingDevice {
                state: state.clone(),
                writes: writes.clone(),
                entered: entered_tx,
                release: release_rx,
            },
            PathBuf::new(),
        )
        .unwrap();
        worker.set_generation(1);
        worker
            .try_submit(Command::Read {
                generation: 1,
                operation: 1,
            })
            .unwrap();
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        (worker, release_tx, writes, state)
    }

    #[test]
    fn reconnect_rejects_queued_old_apply_without_writing() {
        let (worker, release, writes, state) = blocked_worker();
        worker
            .try_submit(Command::Apply {
                generation: 1,
                operation: 2,
                expected: state,
                changes: vec![],
            })
            .unwrap();
        worker.set_generation(2);
        release.send(()).unwrap();
        assert!(matches!(
            worker
                .completions
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            Completion::Read {
                generation: 1,
                operation: 1,
                result: Ok(_)
            }
        ));
        assert!(matches!(
            worker
                .completions
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            Completion::Apply {
                generation: 1,
                operation: 2,
                result: Err(ApplyFailure {
                    recovery: Recovery::NotAttempted,
                    ..
                })
            }
        ));
        assert_eq!(writes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn full_queue_returns_correlated_rejection_to_core() {
        let (worker, release, writes, state) = blocked_worker();
        let apply = |operation| Command::Apply {
            generation: 1,
            operation,
            expected: state.clone(),
            changes: vec![],
        };
        worker.try_submit(apply(2)).unwrap();
        assert!(matches!(
            *worker.try_submit(apply(3)).unwrap_err(),
            Completion::Apply {
                generation: 1,
                operation: 3,
                result: Err(ApplyFailure {
                    recovery: Recovery::NotAttempted,
                    ..
                })
            }
        ));
        worker.set_generation(0);
        release.send(()).unwrap();
        worker
            .completions
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        worker
            .completions
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert_eq!(writes.load(Ordering::SeqCst), 0);
    }

    fn macro_snapshot() -> macros::Snapshot {
        macros::Snapshot {
            backend_id: "memory".into(),
            slot: "slot-00".into(),
            revision: vec![1],
            content: macros::Content::Editable(macros::Program {
                repeat_count: 1,
                events: vec![],
            }),
        }
    }

    #[test]
    fn default_macro_operations_are_typed_unsupported_results() {
        let state = State {
            revision: vec![],
            bindings: BTreeMap::new(),
        };
        let worker = Executor::spawn(
            MemoryDevice {
                state,
                writes: Arc::new(AtomicUsize::new(0)),
            },
            PathBuf::new(),
        )
        .unwrap();
        worker.set_generation(1);
        worker
            .try_submit(Command::ReadMacro {
                generation: 1,
                operation: 1,
                slot: "slot-00".into(),
            })
            .unwrap();
        assert!(
            matches!(worker.completions.recv_timeout(Duration::from_secs(2)).unwrap(),
            Completion::ReadMacro { generation: 1, operation: 1, slot, result: Err(message) }
            if slot == "slot-00" && message.contains("unsupported"))
        );
        let expected = macro_snapshot();
        worker
            .try_submit(Command::ApplyMacro {
                generation: 1,
                operation: 2,
                expected: expected.clone(),
                desired: macros::Program {
                    repeat_count: 1,
                    events: vec![],
                },
            })
            .unwrap();
        assert!(
            matches!(worker.completions.recv_timeout(Duration::from_secs(2)).unwrap(),
            Completion::ApplyMacro { generation: 1, operation: 2, slot, result: Err(ApplyFailure { recovery: Recovery::NotAttempted, message }) }
            if slot == expected.slot && message.contains("unsupported"))
        );
    }

    struct OrderedDevice {
        log: Arc<std::sync::Mutex<Vec<&'static str>>>,
    }

    impl Device for OrderedDevice {
        fn read(&mut self) -> Result<State, String> {
            self.log.lock().unwrap().push("keymap read");
            Ok(State {
                revision: vec![],
                bindings: BTreeMap::new(),
            })
        }
        fn apply(&mut self, _: &State, _: &[Change], _: &Path) -> Result<State, ApplyFailure> {
            self.log.lock().unwrap().push("keymap apply");
            Ok(State {
                revision: vec![],
                bindings: BTreeMap::new(),
            })
        }
        fn read_macro(&mut self, _: &str) -> Result<macros::Snapshot, String> {
            self.log.lock().unwrap().push("macro read");
            Ok(macro_snapshot())
        }
        fn apply_macro(
            &mut self,
            _: &macros::Snapshot,
            _: &macros::Program,
            _: &Path,
        ) -> Result<macros::Snapshot, ApplyFailure> {
            self.log.lock().unwrap().push("macro apply");
            Ok(macro_snapshot())
        }
    }

    #[test]
    fn one_worker_orders_keymap_and_macro_operations() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let worker = Executor::spawn(OrderedDevice { log: log.clone() }, PathBuf::new()).unwrap();
        worker.set_generation(1);
        let expected = macro_snapshot();
        let commands = [
            Command::Read {
                generation: 1,
                operation: 1,
            },
            Command::ReadMacro {
                generation: 1,
                operation: 2,
                slot: expected.slot.clone(),
            },
            Command::Apply {
                generation: 1,
                operation: 3,
                expected: State {
                    revision: vec![],
                    bindings: BTreeMap::new(),
                },
                changes: vec![],
            },
            Command::ApplyMacro {
                generation: 1,
                operation: 4,
                expected,
                desired: macros::Program {
                    repeat_count: 1,
                    events: vec![],
                },
            },
        ];
        for command in commands {
            worker.try_submit(command).unwrap();
            worker
                .completions
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
        }
        assert_eq!(
            *log.lock().unwrap(),
            ["keymap read", "macro read", "keymap apply", "macro apply"]
        );
    }

    #[test]
    fn old_generation_macro_apply_is_rejected_before_device_access() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let worker = Executor::spawn(OrderedDevice { log: log.clone() }, PathBuf::new()).unwrap();
        worker.set_generation(2);
        worker
            .try_submit(Command::ApplyMacro {
                generation: 1,
                operation: 1,
                expected: macro_snapshot(),
                desired: macros::Program {
                    repeat_count: 1,
                    events: vec![],
                },
            })
            .unwrap();
        assert!(
            matches!(worker.completions.recv_timeout(Duration::from_secs(2)).unwrap(),
            Completion::ApplyMacro { generation: 1, operation: 1, slot, result: Err(ApplyFailure { recovery: Recovery::NotAttempted, .. }) }
            if slot == "slot-00")
        );
        assert!(log.lock().unwrap().is_empty());
    }

    struct PanickingMacroDevice;

    impl Device for PanickingMacroDevice {
        fn read(&mut self) -> Result<State, String> {
            unreachable!()
        }
        fn apply(&mut self, _: &State, _: &[Change], _: &Path) -> Result<State, ApplyFailure> {
            unreachable!()
        }
        fn apply_macro(
            &mut self,
            _: &macros::Snapshot,
            _: &macros::Program,
            _: &Path,
        ) -> Result<macros::Snapshot, ApplyFailure> {
            panic!("macro write panic")
        }
    }

    #[test]
    fn panicked_macro_write_returns_correlated_unverified_completion() {
        let worker = Executor::spawn(PanickingMacroDevice, PathBuf::new()).unwrap();
        worker.set_generation(7);
        worker
            .try_submit(Command::ApplyMacro {
                generation: 7,
                operation: 9,
                expected: macro_snapshot(),
                desired: macros::Program {
                    repeat_count: 1,
                    events: vec![],
                },
            })
            .unwrap();
        assert!(
            matches!(worker.completions.recv_timeout(Duration::from_secs(2)).unwrap(),
            Completion::ApplyMacro { generation: 7, operation: 9, slot, result: Err(ApplyFailure { recovery: Recovery::Unverified, .. }) }
            if slot == "slot-00")
        );
    }
}
