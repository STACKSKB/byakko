//! Read-only WASAPI loopback probe. Prints metadata and levels, never audio.
#[path = "../src/audio_sample.rs"]
mod audio_sample;

fn main() -> Result<(), String> {
    let mut sampler = audio_sample::AudioSampler::new()?;
    println!("render endpoint sample rate: {} Hz", sampler.sample_rate());
    let mut frames = 0usize;
    let mut peak = 0.0f32;
    for _ in 0..20 {
        let samples = sampler.sample()?;
        frames += samples.len();
        peak = peak.max(
            samples
                .iter()
                .fold(0.0f32, |level, value| level.max(value.abs())),
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    println!("sampled frames: {frames}; peak: {peak:.3}");
    Ok(())
}
