//! System playback sampling is an OS effect. It never opens or owns HID.
use byakko_devices::{audio_bands::AudioBands, audio_sample::AudioSampler};
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
const FRAME_PERIOD: Duration = Duration::from_millis(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Ready { sample_rate: u32 },
    Frame([u8; 32]),
    Failed(String),
}

/// One audio worker with a single pending event slot.
///
/// `spawn` only prepares and probes playback capture. Call `start` after
/// receiving `Ready`; `stop` is safe in every state and never waits for the
/// worker. Dropping the stream also requests a stop.
pub struct AudioStream {
    state: Arc<AtomicU8>,
    events: Receiver<Event>,
}

impl AudioStream {
    pub fn spawn() -> std::io::Result<Self> {
        let (sender, events) = mpsc::sync_channel(1);
        let state = Arc::new(AtomicU8::new(PREPARING));
        let control = state.clone();
        std::thread::Builder::new()
            .name("byakko-audio-sample".into())
            .spawn(move || {
                let result = (|| -> Result<(), String> {
                    // AudioSampler is created, used, and dropped on this
                    // thread, which preserves COM/thread ownership on Windows.
                    let mut sampler = AudioSampler::new()?;
                    let sample_rate = sampler.sample_rate();
                    let mut bands = AudioBands::new(sample_rate)?;
                    let probe = sampler.sample()?;
                    if !probe.is_empty() {
                        bands.push(&probe);
                    }
                    if control
                        .compare_exchange(PREPARING, READY, Ordering::AcqRel, Ordering::Acquire)
                        .is_err()
                    {
                        return Ok(());
                    }
                    if sender.send(Event::Ready { sample_rate }).is_err() {
                        return Ok(());
                    }
                    while control.load(Ordering::Acquire) != STOPPED {
                        if control.load(Ordering::Acquire) != STREAMING {
                            std::thread::sleep(Duration::from_millis(10));
                            continue;
                        }
                        let began = Instant::now();
                        let samples = sampler.sample()?;
                        if samples.is_empty() {
                            bands.silence((sample_rate / 33) as usize);
                        } else {
                            bands.push(&samples);
                        }
                        match sender.try_send(Event::Frame(bands.frame())) {
                            Ok(()) | Err(TrySendError::Full(_)) => {}
                            Err(TrySendError::Disconnected(_)) => break,
                        }
                        std::thread::sleep(FRAME_PERIOD.saturating_sub(began.elapsed()));
                    }
                    Ok(())
                })();
                if let Err(reason) = result {
                    control.store(STOPPED, Ordering::Release);
                    // Preserve the terminal outcome even when one frame is
                    // pending. This remains bounded and unblocks if the
                    // receiver is dropped.
                    let _ = sender.send(Event::Failed(reason));
                }
            })?;
        Ok(Self { state, events })
    }

    /// Begin frame production after a successful preflight.
    pub fn start(&self) -> Result<(), String> {
        self.state
            .compare_exchange(READY, STREAMING, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|state| match state {
                PREPARING => "Audio capture is still preparing".into(),
                STREAMING => "Audio capture is already streaming".into(),
                _ => "Audio capture has stopped".into(),
            })
    }

    pub fn stop(&self) {
        self.state.store(STOPPED, Ordering::Release);
    }

    pub fn try_receive(&self) -> Result<Event, TryRecvError> {
        self.events.try_recv()
    }
}

impl Drop for AudioStream {
    fn drop(&mut self) {
        self.stop();
    }
}
