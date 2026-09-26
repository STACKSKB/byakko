use super::*;
use byakko_core::{Change, State};
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
    let overflow = worker.try_submit(command(1, 3)).unwrap_err();
    assert_eq!((overflow.generation, overflow.operation), (1, 3));
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
        Action,
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
