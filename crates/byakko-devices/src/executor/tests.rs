use crate::KeymapDevice;
use byakko_core::{Change, State, macros};

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

struct CatalogDevice {
    reads: Arc<AtomicUsize>,
}

impl KeymapDevice for CatalogDevice {
    fn read(&mut self) -> Result<State, String> {
        unreachable!()
    }
    fn apply(&mut self, _: &State, _: &[Change], _: &Path) -> Result<State, ApplyFailure> {
        unreachable!()
    }
    fn read_macro(&mut self, slot: &str) -> Result<macros::Snapshot, String> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(macros::Snapshot {
            backend_id: "test".into(),
            slot: slot.into(),
            revision: vec![1],
            content: macros::Content::Editable(macros::Program {
                repeat_count: 1,
                events: vec![],
            }),
        })
    }
}

#[test]
fn catalog_reads_every_requested_slot_in_one_correlated_command() {
    let reads = Arc::new(AtomicUsize::new(0));
    let worker = Executor::spawn(
        CatalogDevice {
            reads: reads.clone(),
        },
        PathBuf::new(),
    )
    .unwrap();
    worker.set_generation(3);
    worker
        .try_submit(Command::ReadMacroCatalog {
            generation: 3,
            operation: 5,
            slots: vec!["first".into(), "second".into()],
        })
        .unwrap();
    let Completion::ReadMacroCatalog {
        generation,
        operation,
        result,
    } = worker
        .completions
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
    else {
        unreachable!()
    };
    assert_eq!((generation, operation), (3, 5));
    assert_eq!(
        result
            .unwrap()
            .iter()
            .map(|snapshot| snapshot.slot.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second"]
    );
    assert_eq!(reads.load(Ordering::SeqCst), 2);
}
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
    fn apply(&mut self, expected: &State, _: &[Change], _: &Path) -> Result<State, ApplyFailure> {
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
