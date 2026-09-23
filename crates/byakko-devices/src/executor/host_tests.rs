use super::*;
use crate::HostActivity;
use byakko_core::{
    Change, State,
    lighting::{Content, Setting},
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::Duration,
};

struct FakeDevice {
    entered: Sender<()>,
    start_release: Receiver<()>,
    frame_seen: Sender<()>,
    frame_release: Option<Receiver<()>>,
    restored: Sender<()>,
    frames: Arc<AtomicUsize>,
    restores: Arc<AtomicUsize>,
    fail_frame: bool,
    block_frame: bool,
    fail_restore: bool,
}

struct FakeActivity {
    baseline: lighting::Snapshot,
    frame_seen: Sender<()>,
    frame_release: Receiver<()>,
    restored: Sender<()>,
    frames: Arc<AtomicUsize>,
    restores: Arc<AtomicUsize>,
    fail_frame: bool,
    block_frame: bool,
    fail_restore: bool,
}

impl HostActivity for FakeActivity {
    fn send_frame(&mut self, frame: HostFrame) -> Result<(), String> {
        assert_eq!(frame, HostFrame::Rgb([4, 5, 6]));
        self.frames.fetch_add(1, Ordering::SeqCst);
        self.frame_seen.send(()).unwrap();
        if self.block_frame {
            self.frame_release
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
        }
        if self.fail_frame {
            Err("fake frame failure".into())
        } else {
            Ok(())
        }
    }

    fn finish(self: Box<Self>) -> Result<lighting::Snapshot, ApplyFailure> {
        self.restores.fetch_add(1, Ordering::SeqCst);
        self.restored.send(()).unwrap();
        if self.fail_restore {
            Err(ApplyFailure {
                message: "fake restoration failed".into(),
                recovery: Recovery::Failed,
            })
        } else {
            Ok(self.baseline)
        }
    }
}

impl Device for FakeDevice {
    fn read(&mut self) -> Result<State, String> {
        Ok(State {
            revision: vec![],
            bindings: BTreeMap::new(),
        })
    }

    fn apply(&mut self, _: &State, _: &[Change], _: &Path) -> Result<State, ApplyFailure> {
        panic!("finite write reached the fake device while host lighting was active")
    }

    fn start_host_lighting(
        &mut self,
        mode: HostMode,
        expected: &lighting::Snapshot,
        _: &Path,
    ) -> Result<Box<dyn HostActivity>, ApplyFailure> {
        assert_eq!(mode, HostMode::Screen);
        self.entered.send(()).unwrap();
        self.start_release
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        Ok(Box::new(FakeActivity {
            baseline: expected.clone(),
            frame_seen: self.frame_seen.clone(),
            frame_release: self.frame_release.take().unwrap(),
            restored: self.restored.clone(),
            frames: self.frames.clone(),
            restores: self.restores.clone(),
            fail_frame: self.fail_frame,
            block_frame: self.block_frame,
            fail_restore: self.fail_restore,
        }))
    }
}

struct Harness {
    executor: Executor,
    entered: Receiver<()>,
    start_release: Sender<()>,
    frame_seen: Receiver<()>,
    frame_release: Sender<()>,
    restored: Receiver<()>,
    frames: Arc<AtomicUsize>,
    restores: Arc<AtomicUsize>,
}

fn baseline() -> lighting::Snapshot {
    lighting::Snapshot {
        backend_id: "fake".into(),
        revision: vec![1],
        content: Content::Editable(Setting {
            effect: "steady".into(),
            brightness: None,
            speed: None,
            option: None,
            color: None,
        }),
    }
}

fn harness(fail_frame: bool, block_frame: bool, fail_restore: bool) -> Harness {
    let (entered_tx, entered) = mpsc::channel();
    let (start_release, start_rx) = mpsc::channel();
    let (frame_tx, frame_seen) = mpsc::channel();
    let (frame_release, frame_rx) = mpsc::channel();
    let (restored_tx, restored) = mpsc::channel();
    let frames = Arc::new(AtomicUsize::new(0));
    let restores = Arc::new(AtomicUsize::new(0));
    let executor = Executor::spawn(
        FakeDevice {
            entered: entered_tx,
            start_release: start_rx,
            frame_seen: frame_tx,
            frame_release: Some(frame_rx),
            restored: restored_tx,
            frames: frames.clone(),
            restores: restores.clone(),
            fail_frame,
            block_frame,
            fail_restore,
        },
        PathBuf::new(),
    )
    .unwrap();
    executor.set_generation(7);
    Harness {
        executor,
        entered,
        start_release,
        frame_seen,
        frame_release,
        restored,
        frames,
        restores,
    }
}

fn ticket() -> HostTicket {
    HostTicket {
        generation: 7,
        operation: 1,
    }
}

fn start(h: &Harness) {
    h.executor
        .try_start_host(ticket(), HostMode::Screen, baseline())
        .unwrap();
    h.entered.recv_timeout(Duration::from_secs(2)).unwrap();
    h.start_release.send(()).unwrap();
    assert_eq!(
        h.executor
            .host_events
            .recv_timeout(Duration::from_secs(2))
            .unwrap(),
        HostEvent::Started { ticket: ticket() }
    );
}

#[test]
fn stop_during_start_restores_before_any_frame() {
    let h = harness(false, false, false);
    h.executor
        .try_start_host(ticket(), HostMode::Screen, baseline())
        .unwrap();
    h.entered.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(
        h.executor.try_stop_host(ticket()),
        HostStopResult::Requested
    );
    assert_eq!(
        h.executor.try_stop_host(ticket()),
        HostStopResult::AlreadyRequested
    );
    assert!(
        h.executor
            .try_submit(Command::Read {
                generation: 7,
                operation: 2
            })
            .is_err()
    );
    h.start_release.send(()).unwrap();
    assert_eq!(
        h.executor
            .host_events
            .recv_timeout(Duration::from_secs(2))
            .unwrap(),
        HostEvent::Started { ticket: ticket() }
    );
    assert_eq!(
        h.executor
            .host_events
            .recv_timeout(Duration::from_secs(2))
            .unwrap(),
        HostEvent::Finished {
            ticket: ticket(),
            result: Ok(baseline())
        }
    );
    assert_eq!(h.frames.load(Ordering::SeqCst), 0);
    assert_eq!(h.restores.load(Ordering::SeqCst), 1);
}

#[test]
fn bounded_frame_queue_cannot_delay_stop() {
    let h = harness(false, true, false);
    start(&h);
    h.executor
        .try_send_host_frame(ticket(), HostFrame::Rgb([4, 5, 6]))
        .unwrap();
    h.frame_seen.recv_timeout(Duration::from_secs(2)).unwrap();
    h.executor
        .try_send_host_frame(ticket(), HostFrame::Rgb([4, 5, 6]))
        .unwrap();
    assert_eq!(
        h.executor
            .try_send_host_frame(ticket(), HostFrame::Rgb([4, 5, 6])),
        Err(HostFrameError::QueueFull)
    );
    assert_eq!(
        h.executor.try_stop_host(ticket()),
        HostStopResult::Requested
    );
    h.frame_release.send(()).unwrap();
    assert_eq!(
        h.executor
            .host_events
            .recv_timeout(Duration::from_secs(2))
            .unwrap(),
        HostEvent::Finished {
            ticket: ticket(),
            result: Ok(baseline())
        }
    );
    assert_eq!(h.frames.load(Ordering::SeqCst), 1);
    assert_eq!(h.restores.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_frame_restores_and_reports_failure() {
    let h = harness(true, false, false);
    start(&h);
    h.executor
        .try_send_host_frame(ticket(), HostFrame::Rgb([4, 5, 6]))
        .unwrap();
    assert!(
        matches!(h.executor.host_events.recv_timeout(Duration::from_secs(2)).unwrap(), HostEvent::Finished {
        ticket: t, result: Err(ApplyFailure { recovery: Recovery::Verified, .. })
    } if t == ticket())
    );
    assert_eq!(h.restores.load(Ordering::SeqCst), 1);
}

#[test]
fn generation_change_and_drop_each_restore_active_session() {
    let h = harness(false, false, false);
    start(&h);
    h.executor.set_generation(8);
    assert_eq!(
        h.executor
            .host_events
            .recv_timeout(Duration::from_secs(2))
            .unwrap(),
        HostEvent::Finished {
            ticket: ticket(),
            result: Ok(baseline())
        }
    );
    assert_eq!(h.restores.load(Ordering::SeqCst), 1);

    let h = harness(false, false, false);
    start(&h);
    drop(h.executor);
    h.restored.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(h.restores.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_restore_is_reported_as_failed_recovery() {
    let h = harness(false, false, true);
    start(&h);
    assert_eq!(
        h.executor.try_stop_host(ticket()),
        HostStopResult::Requested
    );
    assert!(matches!(
        h.executor.host_events.recv_timeout(Duration::from_secs(2)).unwrap(),
        HostEvent::Finished { ticket: t, result: Err(ApplyFailure { recovery: Recovery::Failed, .. }) } if t == ticket()
    ));
    assert_eq!(h.restores.load(Ordering::SeqCst), 1);
}
