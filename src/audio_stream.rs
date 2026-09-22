//! Opt-in playback loopback to music lighting, with mode restoration.
use crate::{
    audio_bands::AudioBands,
    audio_sample::AudioSampler,
    device,
    lighting::{Lighting, LightingSetting},
};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub fn run(
    expected: &Lighting,
    desired: &LightingSetting,
    backups: &Path,
    stop: &AtomicBool,
) -> device::Result<Lighting> {
    if !matches!(desired.effect_id, 20 | 22) {
        return Err("Select a music effect before starting audio lighting".into());
    }
    let mut sampler = AudioSampler::new()?;
    let rate = sampler.sample_rate();
    let mut bands = AudioBands::new(rate)?;
    if stop.load(Ordering::Relaxed) {
        return Ok(expected.clone());
    }
    let session = device::HostLightingSession::start_mode(expected, desired, backups)?;
    let stream = (|| -> device::Result<()> {
        while !stop.load(Ordering::Relaxed) {
            let began = Instant::now();
            let samples = sampler.sample()?;
            if samples.is_empty() {
                bands.silence((rate / 33) as usize);
            } else {
                bands.push(&samples);
            }
            session.send_music(bands.frame())?;
            std::thread::sleep(Duration::from_millis(30).saturating_sub(began.elapsed()));
        }
        Ok(())
    })();
    let restored = session.finish();
    match (stream, restored) {
        (Ok(()), Ok(lighting)) => Ok(lighting),
        (Err(error), Ok(_)) => Err(format!("Audio sampling stopped: {error}; saved lighting restored").into()),
        (stream, Err(error)) => Err(format!("Audio stream: {stream:?}; lighting restoration failed: {error}. Use the saved lighting backup.").into()),
    }
}
