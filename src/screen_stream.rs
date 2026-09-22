//! Opt-in native screen sampling, with exclusive HID access and restoration.
use crate::{device, lighting::Lighting, screen_sample::ScreenSampler};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub fn run(expected: &Lighting, backups: &Path, stop: &AtomicBool) -> device::Result<Lighting> {
    let mut sampler = ScreenSampler::new()?;
    // Reject inaccessible capture before changing the device's stored mode.
    let first = sampler.sample()?;
    if stop.load(Ordering::Relaxed) {
        return Ok(expected.clone());
    }
    let session = device::ScreenSession::start(expected, backups)?;
    let stream = (|| -> device::Result<()> {
        session.send_color(first)?;
        while !stop.load(Ordering::Relaxed) {
            let began = Instant::now();
            session.send_color(sampler.sample()?)?;
            std::thread::sleep(Duration::from_millis(40).saturating_sub(began.elapsed()));
        }
        Ok(())
    })();
    let restored = session.finish();
    match (stream, restored) {
        (Ok(()), Ok(lighting)) => Ok(lighting),
        (Err(error), Ok(_)) => Err(format!("Screen sampling stopped: {error}; saved lighting restored").into()),
        (stream, Err(error)) => Err(format!("Screen stream: {stream:?}; lighting restoration failed: {error}. Use the saved lighting backup.").into()),
    }
}
