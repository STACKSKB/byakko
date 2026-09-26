//! OS capture stays on its worker thread; one pending frame keeps the latest sample.
use byakko_core::model::lighting::{HostFrame, HostSource};
use byakko_devices::{
    audio_bands::AudioBands,
    audio_sample::AudioSampler,
    screen_sample::{ScreenCapture, ScreenSampler},
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Condvar, Mutex, mpsc::TryRecvError},
    time::{Duration, Instant},
};

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Ready,
    Frame(HostFrame),
    Failed(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Preparing,
    Ready,
    Streaming,
    Stopped,
}

enum Notice {
    Ready,
    Failed(String),
}

struct State {
    phase: Phase,
    notice: Option<Notice>,
    frame: Option<HostFrame>,
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

pub struct Sampler {
    shared: Arc<Shared>,
}

impl Sampler {
    pub fn prepare(source: HostSource, screen: ScreenCapture) -> Result<Self, String> {
        match source {
            HostSource::ScreenAverage => Self::spawn(Duration::from_millis(40), move || {
                let mut sampler = ScreenSampler::with_capture(screen)?;
                // Probe capture before permitting the caller to start the device.
                let mut first = Some(sampler.sample()?);
                Ok(move || {
                    let rgb = match first.take() {
                        Some(rgb) => rgb,
                        None => sampler.sample()?,
                    };
                    Ok(HostFrame::Rgb(rgb))
                })
            }),
            HostSource::PlaybackAudio { bands: 32 } => {
                Self::spawn(Duration::from_millis(30), || {
                    let mut sampler = AudioSampler::new()?;
                    let rate = sampler.sample_rate();
                    let mut bands = AudioBands::new(rate)?;
                    bands.push(&sampler.sample()?);
                    Ok(move || {
                        let samples = sampler.sample()?;
                        if samples.is_empty() {
                            bands.silence((rate / 33) as usize);
                        } else {
                            bands.push(&samples);
                        }
                        Ok(HostFrame::Bands(bands.frame().to_vec()))
                    })
                })
            }
            HostSource::PlaybackAudio { .. } => {
                Err("Playback capture supports 32 audio bands".into())
            }
        }
    }

    // The returned closure need not be Send: it is created, used and dropped here.
    pub(crate) fn spawn<Next>(
        period: Duration,
        prepare: impl FnOnce() -> Result<Next, String> + Send + 'static,
    ) -> Result<Self, String>
    where
        Next: FnMut() -> Result<HostFrame, String> + 'static,
    {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                phase: Phase::Preparing,
                notice: None,
                frame: None,
            }),
            changed: Condvar::new(),
        });
        let worker = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("byakko-sampler".into())
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    let next = prepare()?;
                    run(&worker, period, next)
                }))
                .unwrap_or_else(|_| Err("Capture worker panicked".into()));
                let mut state = worker.state.lock().unwrap();
                if state.phase != Phase::Stopped {
                    state.phase = Phase::Stopped;
                    if let Err(reason) = result {
                        state.notice = Some(Notice::Failed(reason));
                    }
                    worker.changed.notify_all();
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self { shared })
    }

    pub fn begin(&self) -> Result<(), String> {
        let mut state = self.shared.state.lock().unwrap();
        match state.phase {
            Phase::Ready => {
                state.phase = Phase::Streaming;
                self.shared.changed.notify_all();
                Ok(())
            }
            Phase::Preparing => Err("Capture is still preparing".into()),
            Phase::Streaming => Err("Capture is already streaming".into()),
            Phase::Stopped => Err("Capture has stopped".into()),
        }
    }

    /// Cancels controlled waits immediately. An OS call already running finishes
    /// on its worker; its result is discarded and its resources are dropped there.
    pub fn stop(&self) {
        let mut state = self.shared.state.lock().unwrap();
        state.phase = Phase::Stopped;
        state.frame = None;
        // Keep a failure until received, even if Stop races its delivery.
        if matches!(state.notice, Some(Notice::Ready)) {
            state.notice = None;
        }
        self.shared.changed.notify_all();
    }

    pub fn try_receive(&self) -> Result<Event, TryRecvError> {
        let mut state = self.shared.state.lock().unwrap();
        if matches!(state.notice, Some(Notice::Ready)) {
            state.notice = None;
            return Ok(Event::Ready);
        }
        if let Some(frame) = state.frame.take() {
            return Ok(Event::Frame(frame));
        }
        if let Some(Notice::Failed(reason)) = state.notice.take() {
            return Ok(Event::Failed(reason));
        }
        Err(if state.phase == Phase::Stopped {
            TryRecvError::Disconnected
        } else {
            TryRecvError::Empty
        })
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(
    shared: &Shared,
    period: Duration,
    mut next: impl FnMut() -> Result<HostFrame, String>,
) -> Result<(), String> {
    let mut state = shared.state.lock().unwrap();
    if state.phase == Phase::Stopped {
        return Ok(());
    }
    state.phase = Phase::Ready;
    state.notice = Some(Notice::Ready);
    shared.changed.notify_all();
    state = shared
        .changed
        .wait_while(state, |state| state.phase == Phase::Ready)
        .unwrap();
    while state.phase == Phase::Streaming {
        drop(state);
        let began = Instant::now();
        let frame = next()?;
        state = shared.state.lock().unwrap();
        if state.phase != Phase::Streaming {
            break;
        }
        state.frame = Some(frame);
        shared.changed.notify_all();
        (state, _) = shared
            .changed
            .wait_timeout_while(state, period.saturating_sub(began.elapsed()), |state| {
                state.phase == Phase::Streaming
            })
            .unwrap();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc, sync::mpsc, thread};

    const TIMEOUT: Duration = Duration::from_secs(2);

    fn wait_for(sampler: &Sampler, predicate: impl Fn(&State) -> bool) {
        let state = sampler.shared.state.lock().unwrap();
        let (state, timeout) = sampler
            .shared
            .changed
            .wait_timeout_while(state, TIMEOUT, |state| !predicate(state))
            .unwrap();
        assert!(
            !timeout.timed_out() || predicate(&state),
            "Worker timed out"
        );
    }

    fn ready(sampler: &Sampler) {
        wait_for(sampler, |state| state.phase == Phase::Ready);
        assert_eq!(sampler.try_receive(), Ok(Event::Ready));
    }

    #[test]
    fn preparation_failure_and_panic_are_retained_once() {
        for panic in [false, true] {
            let sampler = Sampler::spawn(Duration::ZERO, move || {
                assert!(!panic, "Preparation panicked");
                Err::<fn() -> Result<HostFrame, String>, _>("Preparation failed".into())
            })
            .unwrap();
            wait_for(&sampler, |state| state.phase == Phase::Stopped);
            assert!(matches!(sampler.try_receive(), Ok(Event::Failed(_))));
            assert_eq!(sampler.try_receive(), Err(TryRecvError::Disconnected));
            assert!(sampler.begin().is_err());
        }
    }

    #[test]
    fn non_send_sampler_runs_and_drops_on_its_creator_thread() {
        struct Local {
            created: thread::ThreadId,
            count: Rc<Cell<u8>>,
            dropped: mpsc::Sender<thread::ThreadId>,
        }
        impl Drop for Local {
            fn drop(&mut self) {
                assert_eq!(self.created, thread::current().id());
                let _ = self.dropped.send(thread::current().id());
            }
        }
        let caller = thread::current().id();
        let (dropped, receipt) = mpsc::channel();
        let sampler = Sampler::spawn(Duration::from_secs(60), move || {
            let local = Local {
                created: thread::current().id(),
                count: Rc::new(Cell::new(0)),
                dropped,
            };
            Ok(move || {
                assert_eq!(local.created, thread::current().id());
                local.count.set(local.count.get() + 1);
                Ok(HostFrame::Rgb([local.count.get(); 3]))
            })
        })
        .unwrap();
        ready(&sampler);
        assert_eq!(sampler.try_receive(), Err(TryRecvError::Empty));
        sampler.begin().unwrap();
        assert!(sampler.begin().is_err());
        wait_for(&sampler, |state| state.frame.is_some());
        assert_eq!(
            sampler.try_receive(),
            Ok(Event::Frame(HostFrame::Rgb([1; 3])))
        );
        drop(sampler);
        assert_ne!(receipt.recv_timeout(TIMEOUT).unwrap(), caller);
    }

    #[test]
    fn cancelling_during_preparation_discards_ready_without_starting_frames() {
        let (entered, start) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let (cleaned, finished) = mpsc::channel();
        struct Cleanup(mpsc::Sender<()>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let sampler = Sampler::spawn(Duration::ZERO, move || {
            let cleanup = Cleanup(cleaned);
            entered.send(()).unwrap();
            blocked.recv_timeout(TIMEOUT).unwrap();
            Ok(move || {
                let _keep = &cleanup;
                panic!("Cancelled preparation cannot start sampling");
            })
        })
        .unwrap();
        start.recv_timeout(TIMEOUT).unwrap();
        assert!(sampler.begin().is_err());
        sampler.stop();
        release.send(()).unwrap();
        finished.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(sampler.try_receive(), Err(TryRecvError::Disconnected));
    }

    #[test]
    fn latest_frame_replaces_older_and_terminal_failure_survives_it() {
        let sampler = Sampler::spawn(Duration::ZERO, || {
            let mut count = 0;
            Ok(move || {
                count += 1;
                match count {
                    1..=3 => Ok(HostFrame::Rgb([count; 3])),
                    _ => Err("Sample failed".into()),
                }
            })
        })
        .unwrap();
        ready(&sampler);
        assert_eq!(sampler.try_receive(), Err(TryRecvError::Empty));
        sampler.begin().unwrap();
        wait_for(&sampler, |state| state.phase == Phase::Stopped);
        assert_eq!(
            sampler.try_receive(),
            Ok(Event::Frame(HostFrame::Rgb([3; 3])))
        );
        assert_eq!(
            sampler.try_receive(),
            Ok(Event::Failed("Sample failed".into()))
        );
        assert_eq!(sampler.try_receive(), Err(TryRecvError::Disconnected));
    }
}
