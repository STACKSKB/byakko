//! Bounded synthetic host frames with camera evidence and effect restoration.
mod support;
use byakko_devices::nia87::host_lighting;
use byakko_devices::nia87::{device, lighting::LightingSetting};

fn main() -> device::Result<()> {
    let python = std::env::args().nth(1).ok_or("Supply Python executable")?;
    let target = support::target()?;
    let access = device::Access::bound(target.clone());
    let original = access.read_lighting()?;
    let restore = original
        .recognized_setting()
        .ok_or("Unknown saved lighting")?;
    let maps = access.snapshot()?;
    let settings = access.read_settings()?;
    let backups = std::path::Path::new("Research/captures/backups");
    let mut current = original.clone();
    let result = (|| -> device::Result<()> {
        for (effect_id, payload, label) in [
            (21, host_lighting::screen_report([255, 0, 0]), "screen red"),
            (21, host_lighting::screen_report([0, 0, 255]), "screen blue"),
            (20, host_lighting::music_report([0; 32]), "music zero"),
            (20, host_lighting::music_report([15; 32]), "music fifteen"),
        ] {
            let music = effect_id == 20;
            let desired = LightingSetting {
                effect_id,
                value: music.then_some(4),
                speed: None,
                option: music.then_some(0),
                rgb: music.then_some([255, 0, 0]),
                dazzle: false,
            };
            current = access.apply_lighting(&current, &desired, backups)?;
            let (_, dev) = device::open_expected(&target)?;
            let mut report = [0u8; 65];
            report[1..].copy_from_slice(&payload);
            println!("Capturing {label}");
            let mut camera = std::process::Command::new(&python)
                .arg("Research/capture_webcam.py")
                .spawn()?;
            let started = std::time::Instant::now();
            loop {
                if let Some(status) = camera.try_wait()? {
                    if !status.success() {
                        return Err("Camera failed".into());
                    }
                    break;
                }
                if started.elapsed() > std::time::Duration::from_secs(15) {
                    // Stop sending frames; leave the camera process to finish independently.
                    return Err("Camera timed out; host streaming stopped".into());
                }
                dev.send_feature_report(&report)?;
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
        }
        Ok(())
    })();
    let actual = access.read_lighting()?;
    let restored = access.apply_lighting(&actual, &restore, backups)?;
    if restored.raw()[1..8] != original.raw()[1..8]
        || access.snapshot()? != maps
        || access.read_settings()? != settings
    {
        return Err("Restoration/state comparison failed".into());
    }
    println!("Original lighting restored; settings and maps unchanged");
    result
}
