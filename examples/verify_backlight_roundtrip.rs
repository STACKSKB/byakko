//! Guarded off/on round trip for an already enabled Nia87 USB backlight.
//! Run only with the user's keyboard connected; all outputs are new files.
use std::path::Path;

use byakko::{
    device::{self, Result},
    settings::{Setting, Settings},
};

fn main() -> Result<()> {
    let maps = device::snapshot()?;
    if maps.firmware != 0x100 || maps.profile != 0 {
        return Err("Unverified Nia87 firmware/profile; no write sent".into());
    }
    let original = device::read_settings()?;
    if !original.backlight_enabled() || original.option_flags() & 0x10 != 0 {
        return Err("Backlight must be enabled at the start; no write sent".into());
    }
    let lighting = device::read_lighting()?;
    let backups = Path::new("Research/captures/backups");

    // The apply routine saves a create-new backup and restores its expected
    // complete options reply on any write or readback failure.
    let roundtrip = (|| -> Result<()> {
        let off = device::apply_setting(&original, Setting::Backlight(false), backups)?;
        if off != original.with_backlight(false) {
            return Err("Backlight-off full settings comparison failed".into());
        }
        let on = device::apply_setting(&off, Setting::Backlight(true), backups)?;
        if on != original {
            return Err("Backlight-on full settings comparison failed".into());
        }
        Ok(())
    })();

    // Always attempt to restore after the first transaction, including a
    // failed readback. The transaction itself also attempts its own rollback.
    let restoration = (|| -> Result<Settings> {
        let current = device::read_settings()?;
        if current == original {
            return Ok(current);
        }
        let restored = device::apply_setting(&current, Setting::Backlight(true), backups)?;
        if restored != original {
            return Err("Original options were not fully restored".into());
        }
        Ok(restored)
    })();
    let restored = restoration.map_err(|error| {
        format!("Roundtrip: {roundtrip:?}; restoration could not be verified: {error}")
    })?;
    if restored != original || device::snapshot()? != maps || device::read_lighting()? != lighting {
        return Err("Final settings, keymaps, or lighting comparison failed".into());
    }
    roundtrip?;
    println!(
        "Backlight off/on roundtrip verified; original settings, keymaps, and lighting restored"
    );
    Ok(())
}
