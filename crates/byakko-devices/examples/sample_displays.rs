//! Bounded OS capture smoke check; no HID access and no saved images.
use byakko_devices::screen_sample::{ScreenCapture, ScreenSampler, ScreenSampling, displays};

fn main() -> Result<(), String> {
    for display in displays()? {
        println!("Display: {}", display.label);
        for sampling in [
            ScreenSampling::Average,
            ScreenSampling::Point { x: 500, y: 500 },
        ] {
            let mut sampler = ScreenSampler::with_capture(ScreenCapture {
                display_id: Some(display.id.clone()),
                sampling,
            })?;
            println!("  {sampling:?}: {:?}", sampler.sample()?);
        }
    }
    let unavailable = ScreenSampler::with_capture(ScreenCapture {
        display_id: Some("byakko-nonexistent-display".into()),
        sampling: ScreenSampling::Average,
    });
    match unavailable {
        Err(reason) => println!("Missing display rejected: {reason}"),
        Ok(_) => return Err("Missing display unexpectedly accepted".into()),
    }
    Ok(())
}
