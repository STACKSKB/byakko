//! Screen sampling is an OS effect, never a HID owner. The UI forwards frames.
use byakko_devices::screen_sample::ScreenSampler;
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

pub enum Event {
    Ready,
    Frame([u8; 3]),
    Failed(String),
}

pub struct ScreenStream {
    state: Arc<AtomicU8>,
    events: Receiver<Event>,
}

impl ScreenStream {
    pub fn spawn() -> std::io::Result<Self> {
        let (sender, events) = mpsc::sync_channel(1);
        let state = Arc::new(AtomicU8::new(PREPARING));
        let control = state.clone();
        std::thread::Builder::new()
            .name("byakko-screen-sample".into())
            .spawn(move || {
                let result = (|| -> Result<(), String> {
                    let mut sampler = ScreenSampler::new()?;
                    let first = sampler.sample()?;
                    if control
                        .compare_exchange(PREPARING, READY, Ordering::AcqRel, Ordering::Acquire)
                        .is_err()
                    {
                        return Ok(());
                    }
                    if sender.send(Event::Ready).is_err() {
                        return Ok(());
                    }
                    let mut first_frame = Some(first);
                    while control.load(Ordering::Acquire) != STOPPED {
                        if control.load(Ordering::Acquire) != STREAMING {
                            std::thread::sleep(Duration::from_millis(20));
                            continue;
                        }
                        let began = Instant::now();
                        let frame = match first_frame.take() {
                            Some(first) => first,
                            None => sampler.sample()?,
                        };
                        match sender.try_send(Event::Frame(frame)) {
                            Ok(()) | Err(TrySendError::Full(_)) => {}
                            Err(TrySendError::Disconnected(_)) => break,
                        }
                        std::thread::sleep(
                            Duration::from_millis(40).saturating_sub(began.elapsed()),
                        );
                    }
                    Ok(())
                })();
                if let Err(reason) = result {
                    control.store(STOPPED, Ordering::Release);
                    let _ = sender.try_send(Event::Failed(reason));
                }
            })?;
        Ok(Self { state, events })
    }

    pub fn start(&self) -> Result<(), String> {
        self.state
            .compare_exchange(READY, STREAMING, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|state| match state {
                PREPARING => "Screen capture is still preparing".into(),
                STREAMING => "Screen capture is already streaming".into(),
                _ => "Screen capture has stopped".into(),
            })
    }
    pub fn stop(&self) {
        self.state.store(STOPPED, Ordering::Release);
    }
    pub fn try_receive(&self) -> Result<Event, TryRecvError> {
        self.events.try_recv()
    }
}

impl Drop for ScreenStream {
    fn drop(&mut self) {
        self.stop();
    }
}
