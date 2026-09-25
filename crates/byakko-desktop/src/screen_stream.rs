//! Screen sampling is an OS effect, never a HID owner. The UI forwards frames.
use crate::sampler_worker::{TerminalDelivery, Worker};
use byakko_devices::screen_sample::{ScreenCapture, ScreenSampler};
use std::time::Duration;

pub enum Event {
    Ready,
    Frame([u8; 3]),
    Failed(String),
}

pub(crate) type ScreenStream = Worker<Event>;

impl ScreenStream {
    pub fn spawn(capture: ScreenCapture) -> std::io::Result<Self> {
        Self::prepare(
            "byakko-screen-sample",
            "Screen",
            Duration::from_millis(40),
            Duration::from_millis(20),
            TerminalDelivery::IfAvailable,
            Event::Failed,
            move || {
                let mut sampler = ScreenSampler::with_capture(capture)?;
                let mut first_frame = Some(sampler.sample()?);
                Ok((Event::Ready, move || {
                    let frame = match first_frame.take() {
                        Some(first) => first,
                        None => sampler.sample()?,
                    };
                    Ok(Event::Frame(frame))
                }))
            },
        )
    }
}
