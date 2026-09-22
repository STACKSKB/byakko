//! Bounded replay of the current Nia87 sleep setter; restores all four timers.
use byakko::{
    device::{self, Result},
    settings::Settings,
};

fn send(values: [u16; 4]) -> Result<()> {
    let (_, device) = device::open_unique()?;
    let mut host = [0; 65];
    host[1] = 0x12;
    host[8] = 0xed;
    for (i, value) in values.into_iter().enumerate() {
        host[9 + i * 2..11 + i * 2].copy_from_slice(&value.to_le_bytes());
    }
    device.send_feature_report(&host)?;
    std::thread::sleep(std::time::Duration::from_millis(500));
    Ok(())
}

fn save(path: &str, settings: &Settings) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    serde_json::to_writer_pretty(&mut file, settings)?;
    file.sync_all()?;
    Ok(())
}

fn main() -> Result<()> {
    let maps = device::snapshot()?;
    if maps.firmware != 0x100 || maps.profile != 0 {
        return Err("Unvalidated firmware/profile".into());
    }
    let original = device::read_settings()?;
    if original.sleep_seconds() != [120, 120, 600, 600] {
        return Err("Unexpected timer fixture; no writes sent".into());
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let prefix = format!("Research/captures/sleep-replay-{stamp}");
    save(&format!("{prefix}-before.json"), &original)?;
    let result = (|| -> Result<bool> {
        send([180, 120, 600, 600])?;
        let actual = device::read_settings()?;
        save(&format!("{prefix}-after.json"), &actual)?;
        Ok(actual.sleep_seconds() == [180, 120, 600, 600]
            && [0x91, 0x97, 0x86]
                .iter()
                .all(|&op| actual.raw_reply(op) == original.raw_reply(op)))
    })();
    send(original.sleep_seconds())?;
    let restored = device::read_settings()?;
    save(&format!("{prefix}-restored.json"), &restored)?;
    if restored != original || device::snapshot()? != maps {
        return Err("Full restoration comparison failed".into());
    }
    println!(
        "Sleep setter matched intended state: {}; all settings and both keymaps restored",
        result?
    );
    Ok(())
}
