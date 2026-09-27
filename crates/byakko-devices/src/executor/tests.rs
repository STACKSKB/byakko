use super::*;
use byakko_core::model::keymap::{Change, State};
use std::{collections::BTreeMap, sync::atomic::AtomicUsize};

struct Probe {
    calls: Arc<AtomicUsize>,
    panic: bool,
}
impl Device for Probe {
    fn read(&mut self) -> Result<State, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(!self.panic, "device panic");
        Ok(State {
            revision: vec![1],
            bindings: BTreeMap::new(),
        })
    }
    fn apply(&mut self, _: &State, _: &[Change], _: &Path) -> Result<State, ApplyFailure> {
        self.read().map_err(|message| ApplyFailure {
            message,
            recovery: Recovery::Unverified,
        })
    }
}
fn command(generation: u64, operation: u64) -> Command {
    Command {
        generation,
        operation,
        payload: CommandPayload::Keymap(FeatureCommand::Read(())),
    }
}
#[test]
fn stale_and_duplicate_commands_are_correlated_without_io() {
    let calls = Arc::new(AtomicUsize::new(0));
    let worker = Executor::spawn(
        Probe {
            calls: Arc::clone(&calls),
            panic: false,
        },
        PathBuf::new(),
    )
    .unwrap();
    worker.set_generation(2);
    for (generation, operation, success) in
        [(1, 1, false), (2, 2, true), (2, 2, false), (2, 1, false)]
    {
        worker.try_submit(command(generation, operation)).unwrap();
        let completion = worker.receive(Some(Duration::from_secs(2))).unwrap();
        assert_eq!(
            (completion.generation, completion.operation),
            (generation, operation)
        );
        assert!(
            matches!(completion.payload, CompletionPayload::Keymap(FeatureResult::Read(result)) if result.is_ok() == success)
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[test]
fn panic_becomes_correlated_completion_and_worker_continues() {
    let worker = Executor::spawn(
        Probe {
            calls: Arc::new(AtomicUsize::new(0)),
            panic: true,
        },
        PathBuf::new(),
    )
    .unwrap();
    worker.set_generation(1);
    for operation in 1..=2 {
        worker.try_submit(command(1, operation)).unwrap();
        let completion = worker.receive(Some(Duration::from_secs(2))).unwrap();
        assert_eq!(completion.operation, operation);
        assert!(matches!(
            completion.payload,
            CompletionPayload::Keymap(FeatureResult::Read(Err(_)))
        ));
    }
}
#[test]
fn receive_can_time_out_without_losing_worker() {
    let worker = Executor::spawn(
        Probe {
            calls: Arc::new(AtomicUsize::new(0)),
            panic: false,
        },
        PathBuf::new(),
    )
    .unwrap();
    assert_eq!(
        worker.receive(Some(Duration::ZERO)),
        Err(RecvTimeoutError::Timeout)
    );
}

struct LockedRead {
    path: PathBuf,
    entered: SyncSender<()>,
    release: Receiver<()>,
    calls: Arc<AtomicUsize>,
}
impl Device for LockedRead {
    fn read(&mut self) -> Result<State, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.path)
            .unwrap();
        lock.try_lock().unwrap();
        self.entered.send(()).unwrap();
        self.release.recv_timeout(Duration::from_secs(2)).unwrap();
        Err("Keyboard disconnected".into())
    }
    fn apply(&mut self, _: &State, _: &[Change], _: &Path) -> Result<State, ApplyFailure> {
        unreachable!("only read commands are submitted")
    }
}

#[test]
fn retirement_waits_for_failed_transaction_and_releases_lock_before_replacement_read() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "byakko-retirement-test-{}-{stamp}",
        std::process::id()
    ));
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, resumed) = mpsc::sync_channel(1);
    let calls = Arc::new(AtomicUsize::new(0));
    let worker = Executor::spawn(
        LockedRead {
            path: path.clone(),
            entered,
            release: resumed,
            calls: Arc::clone(&calls),
        },
        PathBuf::new(),
    )
    .unwrap();
    worker.set_generation(1);
    worker.try_submit(command(1, 1)).unwrap();
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.try_submit(command(1, 2)).unwrap();
    let retirement = worker.retire();
    assert!(!retirement.is_finished());
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    assert!(matches!(
        lock.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    release.send(()).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !retirement.is_finished() {
        assert!(std::time::Instant::now() < deadline, "retirement timeout");
        std::thread::yield_now();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1, "queued read must not run");
    lock.try_lock().unwrap();
    drop(lock);

    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, resumed) = mpsc::sync_channel(1);
    let replacement = Executor::spawn(
        LockedRead {
            path,
            entered,
            release: resumed,
            calls,
        },
        PathBuf::new(),
    )
    .unwrap();
    replacement.set_generation(2);
    replacement.try_submit(command(2, 1)).unwrap();
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    release.send(()).unwrap();
    let completion = replacement.receive(Some(Duration::from_secs(2))).unwrap();
    assert!(matches!(completion.payload,
        CompletionPayload::Keymap(FeatureResult::Read(Err(reason)))
        if reason == "Keyboard disconnected"
    ));
}

struct GatedProbe {
    entered: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
    calls: Arc<AtomicUsize>,
}
impl Device for GatedProbe {
    fn read(&mut self) -> Result<State, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.send(()).unwrap();
        self.release.recv().unwrap();
        Ok(State {
            revision: vec![1],
            bindings: BTreeMap::new(),
        })
    }
    fn apply(&mut self, _: &State, _: &[Change], _: &Path) -> Result<State, ApplyFailure> {
        unreachable!("only read commands are submitted")
    }
}
#[test]
fn bounded_queue_rejects_overflow_and_rechecks_generation_before_io() {
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, resumed) = mpsc::sync_channel(1);
    let calls = Arc::new(AtomicUsize::new(0));
    let worker = Executor::spawn(
        GatedProbe {
            entered,
            release: resumed,
            calls: Arc::clone(&calls),
        },
        PathBuf::new(),
    )
    .unwrap();
    worker.set_generation(1);
    worker.try_submit(command(1, 1)).unwrap();
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.try_submit(command(1, 2)).unwrap();
    worker.try_submit(command(1, 3)).unwrap();
    let overflow = worker.try_submit(command(1, 4)).unwrap_err();
    assert_eq!((overflow.generation, overflow.operation), (1, 4));
    assert!(matches!(
        overflow.payload,
        CompletionPayload::Keymap(FeatureResult::Read(Err(_)))
    ));
    worker.set_generation(2);
    release.send(()).unwrap();
    let first = worker.receive(Some(Duration::from_secs(2))).unwrap();
    assert_eq!(first.operation, 1);
    let queued = worker.receive(Some(Duration::from_secs(2))).unwrap();
    assert_eq!(queued.operation, 2);
    assert!(matches!(
        queued.payload,
        CompletionPayload::Keymap(FeatureResult::Read(Err(_)))
    ));
    let queued = worker.receive(Some(Duration::from_secs(2))).unwrap();
    assert_eq!(queued.operation, 3);
    assert!(matches!(
        queued.payload,
        CompletionPayload::Keymap(FeatureResult::Read(Err(_)))
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

struct CountedMemory {
    device: crate::memory::MemoryDevice,
    reads: Arc<AtomicUsize>,
    applies: Arc<AtomicUsize>,
}
impl Device for CountedMemory {
    fn read(&mut self) -> Result<State, String> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.device.read()
    }
    fn apply(
        &mut self,
        expected: &State,
        desired: &[Change],
        backup: &Path,
    ) -> Result<State, ApplyFailure> {
        self.applies.fetch_add(1, Ordering::SeqCst);
        self.device.apply(expected, desired, backup)
    }
}
#[test]
fn public_session_save_delivers_one_apply_without_an_extra_read_command() {
    use byakko_core::{
        model::keymap::Action,
        session::{Outcome, Session},
    };
    let device = crate::memory::demo().unwrap();
    let mut session = Session::new(device.descriptor().clone()).unwrap();
    let reads = Arc::new(AtomicUsize::new(0));
    let applies = Arc::new(AtomicUsize::new(0));
    let worker = Executor::spawn(
        CountedMemory {
            device,
            reads: Arc::clone(&reads),
            applies: Arc::clone(&applies),
        },
        PathBuf::new(),
    )
    .unwrap();
    worker.set_generation(session.connect().unwrap());
    worker.try_submit(session.read().unwrap()).unwrap();
    assert_eq!(
        session.accept(worker.receive(Some(Duration::from_secs(2))).unwrap()),
        Outcome::Loaded
    );
    session
        .edit(Change {
            layer: "Studio".into(),
            key: "Alpha".into(),
            action: Action::Key(5),
        })
        .unwrap();
    worker.try_submit(session.save().unwrap()).unwrap();
    assert_eq!(
        session.accept(worker.receive(Some(Duration::from_secs(2))).unwrap()),
        Outcome::Saved
    );
    assert!(!session.keymap().dirty());
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(applies.load(Ordering::SeqCst), 1);
}

struct CatalogProbe {
    entered: mpsc::SyncSender<String>,
    release: mpsc::Receiver<()>,
    reads: Arc<AtomicUsize>,
    fail_apply: bool,
}
impl Device for CatalogProbe {
    fn read(&mut self) -> Result<State, String> {
        self.entered.send("foreground".into()).unwrap();
        Ok(State {
            revision: vec![1],
            bindings: BTreeMap::new(),
        })
    }
    fn apply(&mut self, _: &State, _: &[Change], _: &Path) -> Result<State, ApplyFailure> {
        if self.fail_apply {
            return Err(ApplyFailure {
                message: "Write failed".into(),
                recovery: Recovery::Failed,
            });
        }
        self.read().map_err(|message| ApplyFailure {
            message,
            recovery: Recovery::Unverified,
        })
    }
    fn read_macro(&mut self, slot: &str) -> Result<byakko_core::model::macros::Snapshot, String> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.entered.send(slot.into()).unwrap();
        self.release.recv().unwrap();
        Ok(byakko_core::model::macros::Snapshot {
            backend_id: "probe".into(),
            slot: slot.into(),
            revision: vec![1],
            content: byakko_core::model::macros::Content::Editable(
                byakko_core::model::macros::Program {
                    repeat_count: 1,
                    events: vec![],
                },
            ),
        })
    }
}
fn catalog_probe(
    fail_apply: bool,
) -> (
    Executor,
    mpsc::Receiver<String>,
    mpsc::SyncSender<()>,
    Arc<AtomicUsize>,
) {
    let (entered, events) = mpsc::sync_channel(4);
    let (release, resumed) = mpsc::sync_channel(1);
    let reads = Arc::new(AtomicUsize::new(0));
    let worker = Executor::spawn(
        CatalogProbe {
            entered,
            release: resumed,
            reads: Arc::clone(&reads),
            fail_apply,
        },
        PathBuf::new(),
    )
    .unwrap();
    worker.set_generation(1);
    worker
        .try_submit(Command {
            generation: 1,
            operation: 1,
            payload: CommandPayload::ReadMacroCatalog {
                slots: vec!["first".into(), "second".into()],
            },
        })
        .unwrap();
    assert_eq!(
        events.recv_timeout(Duration::from_secs(2)).unwrap(),
        "first"
    );
    (worker, events, release, reads)
}
#[test]
fn foreground_runs_between_catalog_slots_without_restarting_scan() {
    let (worker, events, release, reads) = catalog_probe(false);
    worker
        .try_submit(Command {
            generation: 1,
            operation: 2,
            payload: CommandPayload::Keymap(FeatureCommand::Apply {
                expected: State {
                    revision: vec![1],
                    bindings: BTreeMap::new(),
                },
                desired: Vec::new(),
            }),
        })
        .unwrap();
    release.send(()).unwrap();
    assert_eq!(
        events.recv_timeout(Duration::from_secs(2)).unwrap(),
        "foreground"
    );
    assert_eq!(
        worker
            .receive(Some(Duration::from_secs(2)))
            .unwrap()
            .operation,
        2
    );
    assert_eq!(
        events.recv_timeout(Duration::from_secs(2)).unwrap(),
        "second"
    );
    release.send(()).unwrap();
    let completed = worker.receive(Some(Duration::from_secs(2))).unwrap();
    assert_eq!(completed.operation, 1);
    assert!(
        matches!(completed.payload, CompletionPayload::ReadMacroCatalog { result: Ok(snapshots) } if snapshots.len() == 2)
    );
    assert_eq!(reads.load(Ordering::SeqCst), 2);
}
#[test]
fn cancelling_catalog_finishes_current_slot_and_releases_correlation() {
    let (worker, _events, release, reads) = catalog_probe(false);
    worker.cancel_catalog();
    release.send(()).unwrap();
    let completed = worker.receive(Some(Duration::from_secs(2))).unwrap();
    assert_eq!(completed.operation, 1);
    assert!(matches!(
        completed.payload,
        CompletionPayload::ReadMacroCatalog { result: Err(_) }
    ));
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}
#[test]
fn generation_change_stops_catalog_before_next_slot() {
    let (worker, _events, release, reads) = catalog_probe(false);
    worker.set_generation(2);
    release.send(()).unwrap();
    assert!(matches!(
        worker
            .receive(Some(Duration::from_secs(2)))
            .unwrap()
            .payload,
        CompletionPayload::ReadMacroCatalog { result: Err(_) }
    ));
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_foreground_write_stops_catalog_without_reading_next_slot() {
    let (worker, _events, release, reads) = catalog_probe(true);
    worker
        .try_submit(Command {
            generation: 1,
            operation: 2,
            payload: CommandPayload::Keymap(FeatureCommand::Apply {
                expected: State {
                    revision: vec![1],
                    bindings: BTreeMap::new(),
                },
                desired: Vec::new(),
            }),
        })
        .unwrap();
    release.send(()).unwrap();
    let write = worker.receive(Some(Duration::from_secs(2))).unwrap();
    assert_eq!(write.operation, 2);
    assert!(matches!(
        write.payload,
        CompletionPayload::Keymap(FeatureResult::Apply(Err(_)))
    ));
    let cancelled = worker.receive(Some(Duration::from_secs(2))).unwrap();
    assert_eq!(cancelled.operation, 1);
    assert!(matches!(
        cancelled.payload,
        CompletionPayload::ReadMacroCatalog { result: Err(_) }
    ));
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[test]
fn queued_catalog_and_foreground_fit_before_worker_receives_them() {
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, resumed) = mpsc::sync_channel(1);
    let worker = Executor::spawn(
        GatedProbe {
            entered,
            release: resumed,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        PathBuf::new(),
    )
    .unwrap();
    worker.set_generation(1);
    worker.try_submit(command(1, 1)).unwrap();
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    // The first device read holds the worker, so neither queued command can be received yet.
    worker
        .try_submit(Command {
            generation: 1,
            operation: 2,
            payload: CommandPayload::ReadMacroCatalog {
                slots: vec!["one".into()],
            },
        })
        .unwrap();
    worker.try_submit(command(1, 3)).unwrap();
    release.send(()).unwrap();
    assert_eq!(
        worker
            .receive(Some(Duration::from_secs(2)))
            .unwrap()
            .operation,
        1
    );
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    release.send(()).unwrap();
    assert_eq!(
        worker
            .receive(Some(Duration::from_secs(2)))
            .unwrap()
            .operation,
        3
    );
    let catalog = worker.receive(Some(Duration::from_secs(2))).unwrap();
    assert_eq!(catalog.operation, 2);
    assert!(matches!(
        catalog.payload,
        CompletionPayload::ReadMacroCatalog { result: Err(_) }
    ));
}
