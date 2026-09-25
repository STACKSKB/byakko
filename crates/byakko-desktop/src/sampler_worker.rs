//! Private lifecycle for OS samplers; sampler values stay on their worker thread.
use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc::{self, Receiver, TryRecvError, TrySendError},
    },
    time::{Duration, Instant},
};

const PREPARING: u8 = 0;
const READY: u8 = 1;
const STREAMING: u8 = 2;
const STOPPED: u8 = 3;

pub(crate) enum TerminalDelivery {
    /// Audio retains failure behind a pending frame, until receipt or disconnect.
    Preserve,
    /// Screen capture keeps its existing best-effort terminal notification.
    IfAvailable,
}

pub(crate) struct Worker<Event> {
    state: Arc<AtomicU8>,
    events: Receiver<Event>,
    label: &'static str,
}

impl<Event: Send + 'static> Worker<Event> {
    pub(crate) fn prepare<Next: FnMut() -> Result<Event, String> + 'static>(
        name: &str,
        label: &'static str,
        frame_period: Duration,
        idle_period: Duration,
        terminal: TerminalDelivery,
        failed: fn(String) -> Event,
        prepare: impl FnOnce() -> Result<(Event, Next), String> + Send + 'static,
    ) -> std::io::Result<Self> {
        let (sender, events) = mpsc::sync_channel(1);
        let state = Arc::new(AtomicU8::new(PREPARING));
        let control = state.clone();
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let result = (|| -> Result<(), String> {
                    // Next deliberately need not be Send: its captured sampler is
                    // created, used, and dropped here (including COM audio capture).
                    let (ready, mut next) = prepare()?;
                    if control
                        .compare_exchange(PREPARING, READY, Ordering::AcqRel, Ordering::Acquire)
                        .is_err()
                        || sender.send(ready).is_err()
                    {
                        return Ok(());
                    }
                    while control.load(Ordering::Acquire) != STOPPED {
                        if control.load(Ordering::Acquire) != STREAMING {
                            std::thread::sleep(idle_period);
                            continue;
                        }
                        let began = Instant::now();
                        match sender.try_send(next()?) {
                            Ok(()) | Err(TrySendError::Full(_)) => {}
                            Err(TrySendError::Disconnected(_)) => break,
                        }
                        std::thread::sleep(frame_period.saturating_sub(began.elapsed()));
                    }
                    Ok(())
                })();
                if let Err(reason) = result {
                    control.store(STOPPED, Ordering::Release);
                    match terminal {
                        TerminalDelivery::Preserve => {
                            let _ = sender.send(failed(reason));
                        }
                        TerminalDelivery::IfAvailable => {
                            let _ = sender.try_send(failed(reason));
                        }
                    }
                }
            })?;
        Ok(Self {
            state,
            events,
            label,
        })
    }
}

impl<Event> Worker<Event> {
    pub(crate) fn start(&self) -> Result<(), String> {
        self.state
            .compare_exchange(READY, STREAMING, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|state| match state {
                PREPARING => format!("{} capture is still preparing", self.label),
                STREAMING => format!("{} capture is already streaming", self.label),
                _ => format!("{} capture has stopped", self.label),
            })
    }

    pub(crate) fn stop(&self) {
        self.state.store(STOPPED, Ordering::Release);
    }

    pub(crate) fn try_receive(&self) -> Result<Event, TryRecvError> {
        self.events.try_recv()
    }
}

impl<Event> Drop for Worker<Event> {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{rc::Rc, sync::mpsc::RecvTimeoutError, thread::ThreadId};

    const TIMEOUT: Duration = Duration::from_secs(3);

    #[derive(Debug, PartialEq, Eq)]
    enum Event {
        Ready,
        Frame(usize),
        Failed(String),
    }

    fn worker<Next: FnMut() -> Result<Event, String> + 'static>(
        terminal: TerminalDelivery,
        prepare: impl FnOnce() -> Result<(Event, Next), String> + Send + 'static,
    ) -> Worker<Event> {
        Worker::prepare(
            "sampler-test",
            "Test",
            Duration::ZERO,
            Duration::from_millis(1),
            terminal,
            Event::Failed,
            prepare,
        )
        .unwrap()
    }

    fn await_failure(worker: &Worker<Event>) {
        let began = Instant::now();
        while worker.state.load(Ordering::Acquire) != STOPPED {
            assert!(began.elapsed() < TIMEOUT, "worker did not fail");
            std::thread::yield_now();
        }
    }

    // The second frame is dropped while the first occupies the bounded queue.
    // A subsequent error must obey the caller's explicit terminal policy.
    fn failing_worker(terminal: TerminalDelivery) -> Worker<Event> {
        let worker = worker(terminal, || {
            let mut frame = 0;
            Ok((Event::Ready, move || {
                frame += 1;
                match frame {
                    1 | 2 => Ok(Event::Frame(frame)),
                    _ => Err("sample failed".into()),
                }
            }))
        });
        assert_eq!(worker.events.recv_timeout(TIMEOUT), Ok(Event::Ready));
        worker.start().unwrap();
        await_failure(&worker);
        worker
    }

    #[test]
    fn audio_terminal_failure_is_retained_behind_pending_frame() {
        let worker = failing_worker(TerminalDelivery::Preserve);
        assert_eq!(worker.events.recv_timeout(TIMEOUT), Ok(Event::Frame(1)));
        assert_eq!(
            worker.events.recv_timeout(TIMEOUT),
            Ok(Event::Failed("sample failed".into()))
        );
        assert_eq!(
            worker.events.recv_timeout(TIMEOUT),
            Err(RecvTimeoutError::Disconnected)
        );
        assert_eq!(worker.start(), Err("Test capture has stopped".into()));
    }

    #[test]
    fn screen_terminal_failure_may_be_dropped_behind_pending_frame() {
        let worker = failing_worker(TerminalDelivery::IfAvailable);
        // STOPPED is published before terminal delivery. Wait for the producer
        // to disconnect without opening a queue slot and changing the outcome.
        let began = Instant::now();
        while Arc::strong_count(&worker.state) != 1 {
            assert!(began.elapsed() < TIMEOUT, "worker did not finish");
            std::thread::yield_now();
        }
        assert_eq!(worker.events.recv_timeout(TIMEOUT), Ok(Event::Frame(1)));
        assert_eq!(worker.try_receive(), Err(TryRecvError::Disconnected));
    }

    #[test]
    fn preparation_failure_is_delivered_by_both_policies() {
        for terminal in [TerminalDelivery::Preserve, TerminalDelivery::IfAvailable] {
            let worker =
                worker::<fn() -> Result<Event, String>>(terminal, || Err("prepare failed".into()));
            assert_eq!(
                worker.events.recv_timeout(TIMEOUT),
                Ok(Event::Failed("prepare failed".into()))
            );
        }
    }

    #[test]
    fn stop_before_ready_does_not_publish_ready_or_allow_restart() {
        let (release, wait) = mpsc::channel();
        let worker = worker(TerminalDelivery::Preserve, move || {
            wait.recv().unwrap();
            Ok((Event::Ready, || Ok(Event::Frame(0))))
        });
        assert_eq!(
            worker.start(),
            Err("Test capture is still preparing".into())
        );
        worker.stop();
        release.send(()).unwrap();
        assert_eq!(
            worker.events.recv_timeout(TIMEOUT),
            Err(RecvTimeoutError::Disconnected)
        );
        assert_eq!(worker.start(), Err("Test capture has stopped".into()));
    }

    #[test]
    fn repeated_start_is_rejected_and_stop_is_idempotent() {
        let worker = worker(TerminalDelivery::Preserve, || {
            Ok((Event::Ready, || Ok(Event::Frame(0))))
        });
        assert_eq!(worker.events.recv_timeout(TIMEOUT), Ok(Event::Ready));
        worker.start().unwrap();
        assert_eq!(
            worker.start(),
            Err("Test capture is already streaming".into())
        );
        worker.stop();
        worker.stop();
        assert_eq!(worker.start(), Err("Test capture has stopped".into()));
    }

    struct ThreadOwnedSampler {
        // Models a sampler that cannot cross threads, like COM capture.
        owner: Rc<ThreadId>,
        dropped: mpsc::Sender<ThreadId>,
    }

    impl Drop for ThreadOwnedSampler {
        fn drop(&mut self) {
            assert_eq!(*self.owner, std::thread::current().id());
            let _ = self.dropped.send(std::thread::current().id());
        }
    }

    #[test]
    fn dropping_receiver_releases_sampler_on_its_worker_thread() {
        let (dropped, received) = mpsc::channel();
        let worker = worker(TerminalDelivery::Preserve, move || {
            let sampler = ThreadOwnedSampler {
                owner: Rc::new(std::thread::current().id()),
                dropped,
            };
            Ok((Event::Ready, move || {
                assert_eq!(*sampler.owner, std::thread::current().id());
                Ok(Event::Frame(0))
            }))
        });
        assert_eq!(worker.events.recv_timeout(TIMEOUT), Ok(Event::Ready));
        worker.start().unwrap();
        drop(worker);
        assert_ne!(
            received.recv_timeout(TIMEOUT).unwrap(),
            std::thread::current().id()
        );
    }

    #[test]
    fn receiver_drop_unblocks_pending_audio_failure() {
        let worker = failing_worker(TerminalDelivery::Preserve);
        let state = worker.state.clone();
        drop(worker);
        let began = Instant::now();
        while Arc::strong_count(&state) != 1 {
            assert!(began.elapsed() < TIMEOUT, "terminal send did not unblock");
            std::thread::yield_now();
        }
    }
}
