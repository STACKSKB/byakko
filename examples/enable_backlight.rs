//! Explicit user-requested backlight enable, preserving other option bits.
use byakko::{
    device::{self, Result},
    settings::Settings,
};

fn save(path: &str, value: &Settings) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.sync_all()?;
    Ok(())
}

fn send(raw: &[u8]) -> Result<()> {
    let (_, dev) = device::open_unique()?;
    let mut host = [0u8; 65];
    host[1..].copy_from_slice(raw);
    host[1] = 6;
    host[8] = !host[1..8].iter().fold(0u8, |sum, b| sum.wrapping_add(*b));
    dev.send_feature_report(&host)?;
    std::thread::sleep(std::time::Duration::from_secs(1));
    Ok(())
}

fn main() -> Result<()> {
    let maps = device::snapshot()?;
    if maps.firmware != 0x100 || maps.profile != 0 {
        return Err("Unverified firmware/profile".into());
    }
    let before = device::read_settings()?;
    let lighting = device::read_lighting()?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let prefix = format!("Research/captures/backlight-enable-{stamp}");
    save(&format!("{prefix}-before.json"), &before)?;
    let mut desired = before.raw_reply(0x86).unwrap().to_vec();
    desired[2] &= !0x10;
    // The official lighting controls treat power-save mode as lights off too.
    desired[4] = 0;
    let result = (|| -> Result<()> {
        send(&desired)?;
        let after = device::read_settings()?;
        save(&format!("{prefix}-after.json"), &after)?;
        let raw = after.raw_reply(0x86).unwrap();
        if raw
            .iter()
            .enumerate()
            .any(|(i, b)| i != 7 && *b != desired[i])
            || [0x91, 0x97, 0x92]
                .iter()
                .any(|&op| after.raw_reply(op) != before.raw_reply(op))
            || device::snapshot()? != maps
            || device::read_lighting()? != lighting
        {
            return Err("Backlight enable readback comparison failed".into());
        }
        println!(
            "Backlight enabled: flags=0x{:02x}, power-save={}; other settings, keymaps and lighting effect unchanged",
            after.option_flags(),
            after.power_save_value()
        );
        Ok(())
    })();
    if let Err(error) = result {
        send(before.raw_reply(0x86).unwrap())?;
        if device::read_settings()? != before {
            return Err(format!("{error}; restoration mismatch").into());
        }
        return Err(format!("{error}; original settings restored").into());
    }
    Ok(())
}
