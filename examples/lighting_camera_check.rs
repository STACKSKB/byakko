//! Reversible steady-color camera check, using an explicitly supplied Python.
mod support;
use byakko_devices::nia87::{device, lighting::LightingSetting};

fn main() -> device::Result<()> {
    let python = std::env::args()
        .nth(1)
        .ok_or("Supply Python executable for the camera capture script")?;
    let access = support::access()?;
    let original = access.read_lighting()?;
    let original_setting = original
        .recognized_setting()
        .ok_or("Unknown original lighting")?;
    let maps = access.snapshot()?;
    let settings = access.read_settings()?;
    let backups = std::path::Path::new("Research/captures/backups");
    let mut current = original.clone();
    let result = (|| -> device::Result<()> {
        for rgb in [[255, 0, 0], [0, 255, 0]] {
            let desired = LightingSetting {
                effect_id: 1,
                value: Some(4),
                speed: None,
                option: None,
                rgb: Some(rgb),
                dazzle: false,
            };
            current = access.apply_lighting(&current, &desired, backups)?;
            std::thread::sleep(std::time::Duration::from_secs(2));
            println!("Capturing steady color {rgb:?}");
            let status = std::process::Command::new(&python)
                .arg("Research/capture_webcam.py")
                .status()?;
            if !status.success() {
                return Err("Camera capture failed".into());
            }
        }
        Ok(())
    })();
    let restored = access.apply_lighting(&current, &original_setting, backups)?;
    if restored.raw()[1..8] != original.raw()[1..8]
        || access.snapshot()? != maps
        || access.read_settings()? != settings
    {
        return Err("Restoration/state comparison failed".into());
    }
    println!("Original effect restored; keymaps and settings unchanged");
    result
}
