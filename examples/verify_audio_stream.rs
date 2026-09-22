//! Real playback loopback, tone, camera evidence, and lighting restoration.
use byakko::{device, lighting::LightingSetting};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

fn main() -> device::Result<()> {
    let python = std::env::args().nth(1).ok_or("Supply Python executable")?;
    let before = device::read_lighting()?;
    let settings = device::read_settings()?;
    let maps = device::snapshot()?;
    let desired = LightingSetting {
        effect_id: 22,
        value: Some(4),
        speed: None,
        option: Some(0),
        rgb: Some([0, 255, 0]),
        dazzle: false,
    };
    let stop = AtomicBool::new(false);
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_secs(12));
            stop.store(true, Ordering::Relaxed);
        });
        let tone = scope.spawn(|| -> device::Result<()> {
            std::thread::sleep(Duration::from_secs(2));
            let status = std::process::Command::new(&python)
                .arg("Research/play_test_tone.py")
                .status()?;
            if !status.success() {
                return Err("Test tone failed".into());
            }
            Ok(())
        });
        let camera = scope.spawn(|| -> device::Result<()> {
            std::thread::sleep(Duration::from_secs(4));
            if device::read_lighting().is_ok() {
                return Err("Stream did not hold transaction lock".into());
            }
            let status = std::process::Command::new(&python)
                .arg("Research/capture_webcam.py")
                .status()?;
            if !status.success() {
                return Err("Camera capture failed".into());
            }
            Ok(())
        });
        let result = byakko::audio_stream::run(
            &before,
            &desired,
            std::path::Path::new("Research/captures/backups"),
            &stop,
        );
        (result, tone.join(), camera.join())
    });
    let restored = result.0?;
    result.1.map_err(|_| "Tone thread panicked")??;
    result.2.map_err(|_| "Camera thread panicked")??;
    if restored.raw()[1..8] != before.raw()[1..8]
        || device::read_settings()? != settings
        || device::snapshot()? != maps
    {
        return Err("Audio stream restoration comparison failed".into());
    }
    println!(
        "Native audio stream completed; exclusive lock held; lighting, settings and maps restored"
    );
    Ok(())
}
