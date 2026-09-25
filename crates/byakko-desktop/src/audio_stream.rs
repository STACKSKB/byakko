//! System playback sampling is an OS effect. It never opens or owns HID.
use crate::sampler_worker::{TerminalDelivery, Worker};
use byakko_devices::{audio_bands::AudioBands, audio_sample::AudioSampler};
use std::time::Duration;

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
pub(crate) type AudioStream = Worker<Event>;

impl AudioStream {
    pub fn spawn() -> std::io::Result<Self> {
        Self::prepare(
            "byakko-audio-sample",
            "Audio",
            Duration::from_millis(30),
            Duration::from_millis(10),
            TerminalDelivery::Preserve,
            Event::Failed,
            || {
                let mut sampler = AudioSampler::new()?;
                let sample_rate = sampler.sample_rate();
                let mut bands = AudioBands::new(sample_rate)?;
                let probe = sampler.sample()?;
                if !probe.is_empty() {
                    bands.push(&probe);
                }
                Ok((Event::Ready { sample_rate }, move || {
                    let samples = sampler.sample()?;
                    if samples.is_empty() {
                        bands.silence((sample_rate / 33) as usize);
                    } else {
                        bands.push(&samples);
                    }
                    Ok(Event::Frame(bands.frame()))
                }))
            },
        )
    }
}
