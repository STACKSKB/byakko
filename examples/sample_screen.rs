//! Read-only timing probe for the native screen sampler; no image is saved.
use std::time::Instant;

fn main() -> Result<(), String> {
    let mut sampler = byakko::screen_sample::ScreenSampler::new()?;
    for _ in 0..10 {
        let start = Instant::now();
        let rgb = sampler.sample()?;
        println!(
            "RGB {rgb:?} in {:.2} ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
    Ok(())
}
