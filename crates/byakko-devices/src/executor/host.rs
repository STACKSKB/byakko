//! Latest-frame delivery and restoration on the selected-device worker.
use super::*;
use byakko_core::contract::HostEventKind;
use std::{
    collections::VecDeque,
    sync::{Condvar, Mutex},
};

#[derive(Default)]
pub(super) struct Host {
    state: Mutex<State>,
    changed: Condvar,
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::model::{keymap, lighting};
    use std::time::Instant;
    const TIMEOUT: Duration = Duration::from_secs(2);
    struct Probe {
        events: SyncSender<&'static str>,
        release: Receiver<()>,
        fault: &'static str,
    }
    struct Activity {
        events: SyncSender<&'static str>,
        snapshot: lighting::Snapshot,
        fault: &'static str,
    }
    impl crate::HostActivity for Activity {
        fn send_frame(&mut self, _: crate::HostFrame) -> Result<(), String> {
            self.events.send("frame").unwrap();
            match self.fault {
                "frame panic" => panic!("frame"),
                "frame error" => Err("frame failed".into()),
                _ => Ok(()),
            }
        }
        fn update_parameters(&mut self, setting: lighting::Setting) -> Result<(), String> {
            assert_eq!(setting.brightness, Some(3));
            self.events.send("parameters").unwrap();
            Ok(())
        }
        fn finish(self: Box<Self>) -> Result<lighting::Snapshot, ApplyFailure> {
            self.events.send("finish").unwrap();
            if self.fault == "finish panic" {
                panic!("finish");
            }
            Ok(self.snapshot.clone())
        }
    }
    impl Device for Probe {
        fn read(&mut self) -> Result<keymap::State, String> {
            panic!("finite IO during host")
        }
        fn apply(
            &mut self,
            _: &keymap::State,
            _: &[keymap::Change],
            _: &Path,
        ) -> Result<keymap::State, ApplyFailure> {
            unreachable!()
        }
        fn start_host_lighting(
            &mut self,
            _: lighting::HostMode,
            _: Option<lighting::Setting>,
            expected: &lighting::Snapshot,
            _: &Path,
        ) -> Result<Box<dyn crate::HostActivity>, ApplyFailure> {
            self.events.send("start").unwrap();
            self.release.recv_timeout(TIMEOUT).unwrap();
            if self.fault == "start panic" {
                panic!("start");
            }
            Ok(Box::new(Activity {
                events: self.events.clone(),
                snapshot: expected.clone(),
                fault: self.fault,
            }))
        }
    }
    fn fixture(
        fault: &'static str,
    ) -> (Executor, HostStart, Receiver<&'static str>, SyncSender<()>) {
        let (events, received) = mpsc::sync_channel(8);
        let (release, waiting) = mpsc::sync_channel(1);
        let worker = Executor::spawn(
            Probe {
                events,
                release: waiting,
                fault,
            },
            PathBuf::new(),
        )
        .unwrap();
        worker.set_generation(1);
        let mut demo = crate::memory::demo().unwrap();
        let start = HostStart {
            ticket: HostTicket {
                generation: 1,
                operation: 1,
            },
            mode: lighting::HostMode {
                requires_enabled_setting: None,
                id: "screen".into(),
                label: "Screen".into(),
                source: lighting::HostSource::ScreenAverage,
                parameters: None,
            },
            setting: None,
            expected: demo.read_lighting().unwrap(),
        };
        (worker, start, received, release)
    }
    fn event(worker: &Executor) -> HostEvent {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Ok(event) = worker.try_receive_host() {
                return event;
            }
            assert!(Instant::now() < deadline, "host event timeout");
            std::thread::yield_now();
        }
    }
    #[test]
    fn parameter_updates_coalesce_reject_stale_and_preserve_restoration() {
        let (worker, start, events, release) = fixture("");
        let ticket = start.ticket;
        let expected = start.expected.clone();
        worker.start_host(start).unwrap();
        assert_eq!(events.recv_timeout(TIMEOUT).unwrap(), "start");
        let update = |brightness| byakko_core::contract::HostUpdate {
            ticket,
            setting: lighting::Setting {
                effect: "music".into(),
                brightness: Some(brightness),
                speed: None,
                option: None,
                color: None,
            },
        };
        for value in 1..=3 {
            worker.update_host(update(value)).unwrap();
        }
        let mut stale = update(3);
        stale.ticket.operation += 1;
        assert!(worker.update_host(stale).is_err());
        assert_eq!(
            worker.host.state.lock().unwrap().setting,
            Some(update(3).setting)
        );
        release.send(()).unwrap();
        assert_eq!(events.recv_timeout(TIMEOUT).unwrap(), "parameters");
        worker
            .send_host_frame(ticket, crate::HostFrame::Rgb([1; 3]))
            .unwrap();
        assert_eq!(events.recv_timeout(TIMEOUT).unwrap(), "frame");
        worker.stop_host(ticket, None);
        assert!(worker.update_host(update(3)).is_err());
        assert_eq!(events.recv_timeout(TIMEOUT).unwrap(), "finish");
        assert!(matches!(event(&worker).kind, HostEventKind::Started));
        assert!(
            matches!(event(&worker).kind, HostEventKind::Finished { restored: Ok(snapshot), problem: None } if snapshot == expected)
        );
        assert!(events.try_recv().is_err());
    }
    #[test]
    fn startup_stop_restores_without_frames_or_draining_notifications() {
        let (worker, start, events, release) = fixture("");
        let ticket = start.ticket;
        worker.start_host(start.clone()).unwrap();
        assert_eq!(events.recv_timeout(TIMEOUT).unwrap(), "start");
        assert!(worker.start_host(start.clone()).is_err());
        assert!(
            worker
                .try_submit(Command {
                    generation: 1,
                    operation: 2,
                    payload: CommandPayload::Keymap(FeatureCommand::Read(()))
                })
                .is_err()
        );
        worker
            .send_host_frame(ticket, crate::HostFrame::Rgb([1; 3]))
            .unwrap();
        worker.stop_host(ticket, Some("sampler failed".into()));
        release.send(()).unwrap();
        assert_eq!(events.recv_timeout(TIMEOUT).unwrap(), "finish");
        assert!(events.try_recv().is_err());
        assert!(matches!(event(&worker).kind, HostEventKind::Started));
        assert!(
            matches!(event(&worker).kind, HostEventKind::Finished { restored: Ok(_), problem: Some(problem) } if problem == "sampler failed")
        );
        assert!(worker.start_host(start).is_err());
    }
    #[test]
    fn startup_frames_coalesce_to_one_latest_frame() {
        let (worker, start, events, release) = fixture("");
        let ticket = start.ticket;
        worker.start_host(start).unwrap();
        assert_eq!(events.recv_timeout(TIMEOUT).unwrap(), "start");
        for value in 1..=3 {
            worker
                .send_host_frame(ticket, crate::HostFrame::Rgb([value; 3]))
                .unwrap();
        }
        assert_eq!(
            worker.host.state.lock().unwrap().frame,
            Some(crate::HostFrame::Rgb([3; 3]))
        );
        release.send(()).unwrap();
        assert_eq!(events.recv_timeout(TIMEOUT).unwrap(), "frame");
        worker.stop_host(ticket, None);
        assert_eq!(events.recv_timeout(TIMEOUT).unwrap(), "finish");
        assert!(events.try_recv().is_err());
        assert!(matches!(event(&worker).kind, HostEventKind::Started));
        assert!(matches!(
            event(&worker).kind,
            HostEventKind::Finished {
                restored: Ok(_),
                problem: None
            }
        ));
    }
    #[test]
    fn queued_stop_problem_survives_generation_change_during_startup() {
        let (worker, start, events, release) = fixture("");
        let ticket = start.ticket;
        worker.start_host(start).unwrap();
        assert_eq!(events.recv_timeout(TIMEOUT).unwrap(), "start");
        worker.stop_host(ticket, Some("sampler failed".into()));
        worker.set_generation(2);
        release.send(()).unwrap();
        assert_eq!(events.recv_timeout(TIMEOUT).unwrap(), "finish");
        assert!(matches!(event(&worker).kind, HostEventKind::Started));
        assert!(matches!(event(&worker).kind, HostEventKind::Finished {
            restored: Ok(_), problem: Some(problem),
        } if problem == "sampler failed"));
    }
    #[test]
    fn generation_change_and_drop_restore_during_startup() {
        for drop_worker in [false, true] {
            let (worker, start, events, release) = fixture("");
            worker.start_host(start).unwrap();
            assert_eq!(events.recv_timeout(TIMEOUT).unwrap(), "start");
            if drop_worker {
                drop(worker);
            } else {
                worker.set_generation(2);
            }
            release.send(()).unwrap();
            assert_eq!(events.recv_timeout(TIMEOUT).unwrap(), "finish");
        }
    }
    #[test]
    fn panic_and_frame_failure_keep_correlated_terminal_results() {
        for fault in ["start panic", "frame panic", "frame error", "finish panic"] {
            let (worker, start, events, release) = fixture(fault);
            let ticket = start.ticket;
            worker.start_host(start).unwrap();
            assert_eq!(events.recv_timeout(TIMEOUT).unwrap(), "start");
            release.send(()).unwrap();
            if fault != "start panic" {
                assert!(matches!(event(&worker).kind, HostEventKind::Started));
                if fault == "finish panic" {
                    worker.stop_host(ticket, None);
                } else {
                    worker
                        .send_host_frame(ticket, crate::HostFrame::Rgb([2; 3]))
                        .unwrap();
                }
            }
            let terminal = event(&worker);
            assert_eq!(terminal.ticket, ticket);
            match terminal.kind {
                HostEventKind::Finished { restored, problem } => {
                    if fault == "start panic" || fault == "finish panic" {
                        assert_eq!(restored.unwrap_err().recovery, Recovery::Unverified);
                    } else {
                        assert!(restored.is_ok());
                        assert!(problem.is_some());
                    }
                }
                _ => panic!("missing terminal event"),
            }
            assert!(worker.try_receive_host().is_err());
            assert!(
                worker
                    .send_host_frame(ticket, crate::HostFrame::Rgb([3; 3]))
                    .is_err()
            );
        }
    }
}
#[derive(Default)]
struct State {
    ticket: Option<HostTicket>,
    latest: Option<(u64, u64)>,
    frame: Option<crate::HostFrame>,
    setting: Option<byakko_core::model::lighting::Setting>,
    stop: Option<Option<String>>,
    events: VecDeque<HostEvent>,
    closed: bool,
}
pub(super) fn rejected(ticket: HostTicket, message: &str, recovery: Recovery) -> HostEvent {
    HostEvent {
        ticket,
        kind: HostEventKind::Finished {
            restored: Err(ApplyFailure {
                message: message.into(),
                recovery,
            }),
            problem: None,
        },
    }
}
impl Host {
    // Rejections have the same owned shape as delivered host terminal events.
    #[allow(clippy::result_large_err)]
    pub(super) fn reserve(&self, ticket: HostTicket, generation: u64) -> Result<(), HostEvent> {
        let mut state = self.state.lock().unwrap();
        let token = (ticket.generation, ticket.operation);
        if state.closed
            || state.ticket.is_some()
            || ticket.generation == 0
            || ticket.generation != generation
            || state.latest.is_some_and(|previous| token <= previous)
        {
            return Err(rejected(
                ticket,
                "Host lighting ticket unavailable or stale",
                Recovery::NotAttempted,
            ));
        }
        state.latest = Some(token);
        state.ticket = Some(ticket);
        state.stop = None;
        state.frame = None;
        state.setting = None;
        Ok(())
    }
    pub(super) fn unreserve(&self) {
        self.state.lock().unwrap().ticket = None;
    }
    pub(super) fn pending(&self) -> bool {
        self.state.lock().unwrap().ticket.is_some()
    }
    pub(super) fn frame(&self, ticket: HostTicket, frame: crate::HostFrame) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        if state.ticket != Some(ticket)
            || state.stop.is_some()
            || state.closed
            || state
                .events
                .iter()
                .any(|event| matches!(event.kind, HostEventKind::Finished { .. }))
        {
            return Err("Host lighting is not accepting frames".into());
        }
        state.frame = Some(frame);
        self.changed.notify_one();
        Ok(())
    }
    pub(super) fn update(&self, update: byakko_core::contract::HostUpdate) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        if state.ticket != Some(update.ticket)
            || state.stop.is_some()
            || state.closed
            || state
                .events
                .iter()
                .any(|event| matches!(event.kind, HostEventKind::Finished { .. }))
        {
            return Err("Host lighting is not accepting parameter updates".into());
        }
        state.setting = Some(update.setting);
        self.changed.notify_one();
        Ok(())
    }
    pub(super) fn stop(&self, ticket: HostTicket, problem: Option<String>) {
        let mut state = self.state.lock().unwrap();
        if state.ticket == Some(ticket) && state.stop.is_none() {
            state.stop = Some(problem);
            state.frame = None;
            state.setting = None;
            self.changed.notify_one();
        }
    }
    pub(super) fn generation_changed(&self) {
        let _state = self.state.lock().unwrap();
        self.changed.notify_one();
    }
    pub(super) fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        self.changed.notify_one();
    }
    pub(super) fn receive(&self) -> Result<HostEvent, TryRecvError> {
        let mut state = self.state.lock().unwrap();
        let event = state.events.pop_front().ok_or(TryRecvError::Empty)?;
        if matches!(event.kind, HostEventKind::Finished { .. }) {
            state.ticket = None;
            state.frame = None;
            state.setting = None;
            state.stop = None;
        }
        Ok(event)
    }
    fn emit(&self, event: HostEvent) {
        let mut state = self.state.lock().unwrap();
        // One reservation has at most Started and Finished. Neither requires a reader.
        state.events.push_back(event);
    }
    pub(super) fn reject(&self, ticket: HostTicket, message: &str, recovery: Recovery) {
        self.emit(rejected(ticket, message, recovery));
    }
    pub(super) fn run(
        &self,
        device: &mut dyn Device,
        start: HostStart,
        backup: &Path,
        generation: &AtomicU64,
    ) {
        let ticket = start.ticket;
        if generation.load(Ordering::Acquire) != ticket.generation {
            self.reject(
                ticket,
                "Stale host lighting generation",
                Recovery::NotAttempted,
            );
            return;
        }
        let started = catch_unwind(AssertUnwindSafe(|| {
            device.start_host_lighting(start.mode, start.setting, &start.expected, backup)
        }))
        .unwrap_or_else(|_| {
            Err(ApplyFailure {
                message: "Host lighting start panicked".into(),
                recovery: Recovery::Unverified,
            })
        });
        let mut activity = match started {
            Ok(activity) => activity,
            Err(error) => {
                self.emit(HostEvent {
                    ticket,
                    kind: HostEventKind::Finished {
                        restored: Err(error),
                        problem: None,
                    },
                });
                return;
            }
        };
        self.emit(HostEvent {
            ticket,
            kind: HostEventKind::Started,
        });
        enum Input {
            Frame(crate::HostFrame),
            Parameters(byakko_core::model::lighting::Setting),
        }
        let problem = loop {
            let frame = {
                let mut state = self.state.lock().unwrap();
                loop {
                    if let Some(problem) = state.stop.take() {
                        break Some(Err(problem));
                    }
                    if state.closed || generation.load(Ordering::Acquire) != ticket.generation {
                        break None;
                    }
                    if let Some(setting) = state.setting.take() {
                        break Some(Ok(Input::Parameters(setting)));
                    }
                    if let Some(frame) = state.frame.take() {
                        break Some(Ok(Input::Frame(frame)));
                    }
                    state = self.changed.wait(state).unwrap();
                }
            };
            match frame {
                None => break None,
                Some(Err(problem)) => break problem,
                Some(Ok(input)) => {
                    let result = catch_unwind(AssertUnwindSafe(|| match input {
                        Input::Frame(frame) => activity.send_frame(frame),
                        Input::Parameters(setting) => activity.update_parameters(setting),
                    }))
                    .unwrap_or_else(|_| Err("Host lighting frame panicked".into()));
                    if let Err(problem) = result {
                        break Some(problem);
                    }
                }
            }
        };
        {
            let mut state = self.state.lock().unwrap();
            state.stop = Some(problem.clone());
            state.frame = None;
            state.setting = None;
        }
        let restored = catch_unwind(AssertUnwindSafe(|| activity.finish())).unwrap_or_else(|_| {
            Err(ApplyFailure {
                message: "Host lighting restoration panicked".into(),
                recovery: Recovery::Unverified,
            })
        });
        self.emit(HostEvent {
            ticket,
            kind: HostEventKind::Finished { restored, problem },
        });
    }
}
