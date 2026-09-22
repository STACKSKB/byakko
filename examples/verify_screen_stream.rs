//! Bounded real screen sampler, camera evidence, and restoration check.
use byakko::device;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

fn main() -> device::Result<()> {
    let python = std::env::args().nth(1).ok_or("Supply Python executable")?;
    let before = device::read_lighting()?;
    let settings = device::read_settings()?;
    let maps = device::snapshot()?;
    let stop = AtomicBool::new(false);
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_secs(10));
            stop.store(true, Ordering::Relaxed);
        });
        let camera = scope.spawn(|| -> device::Result<()> {
            std::thread::sleep(Duration::from_secs(3));
            // A concurrent Byakko transaction must be rejected while streaming.
            if device::read_lighting().is_ok() {
                return Err("Stream did not hold transaction lock".into());
            }
            let status = std::process::Command::new(python)
                .arg("Research/capture_webcam.py")
                .status()?;
            if !status.success() {
                return Err("Camera capture failed".into());
            }
            Ok(())
        });
        let result = byakko::screen_stream::run(
            &before,
            std::path::Path::new("Research/captures/backups"),
            &stop,
        );
        (result, camera.join())
    });
    let restored = result.0?;
    result.1.map_err(|_| "Camera thread panicked")??;
    if restored.raw()[1..8] != before.raw()[1..8]
        || device::read_settings()? != settings
        || device::snapshot()? != maps
    {
        return Err("Stream restoration comparison failed".into());
    }
    println!(
        "Native screen sampling completed; exclusive lock held; lighting, settings and maps restored"
    );
    Ok(())
}
