use super::apply_error::settings_apply_error;
use super::*;

pub fn read_settings() -> Result<crate::nia87::settings::Settings> {
    read_settings_with(Selection::Unique)
}

pub(super) fn read_settings_with(
    selection: Selection<'_>,
) -> Result<crate::nia87::settings::Settings> {
    let session = Session::open_for(selection)?;
    read_settings_on_device(session.device())
}

pub(super) fn read_settings_on_device(
    device: &HidDevice,
) -> Result<crate::nia87::settings::Settings> {
    let replies = [0x91, 0x97, 0x92, 0x86]
        .into_iter()
        .map(|opcode| read_payload(device, opcode, 0, 0))
        .collect::<Result<Vec<_>>>()?;
    Ok(crate::nia87::settings::Settings::decode(
        &replies[0],
        &replies[1],
        &replies[2],
        &replies[3],
    )?)
}

pub fn apply_setting(
    expected: &crate::nia87::settings::Settings,
    setting: crate::nia87::settings::Setting,
    backup_dir: &std::path::Path,
) -> Result<crate::nia87::settings::Settings> {
    apply_setting_with(Selection::Unique, expected, setting, backup_dir)
}

pub(super) fn apply_setting_with(
    selection: Selection<'_>,
    expected: &crate::nia87::settings::Settings,
    setting: crate::nia87::settings::Setting,
    backup_dir: &std::path::Path,
) -> Result<crate::nia87::settings::Settings> {
    use crate::nia87::settings::{SettingPlan, Settings};
    let _lock = transaction_lock()?;
    let SettingPlan {
        target,
        report,
        restore_report,
    } = expected.plan_change(setting)?;
    if &target == expected {
        return Ok(target);
    }
    let (_, device) = selection.open()?;
    std::fs::create_dir_all(backup_dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = backup_dir.join(format!("settings-before-{stamp}.json"));
    let mut backup = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    serde_json::to_writer_pretty(
        &mut backup,
        &serde_json::json!({"format_version":1,"before":expected,"target":target}),
    )?;
    backup.sync_all()?;
    let send = |data: &[u8; 64]| -> Result<()> {
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(data);
        device.send_setter(&host)?;
        std::thread::sleep(std::time::Duration::from_millis(500));
        Ok(())
    };
    let result = (|| -> Result<Settings> {
        send(&report)?;
        let actual = read_settings_on_device(&device)?;
        if actual != target {
            return Err("Setting readback mismatch".into());
        }
        Ok(actual)
    })();
    match result {
        Ok(actual) => Ok(actual),
        Err(error) => {
            let restore = (|| -> Result<()> {
                send(&restore_report)?;
                if &read_settings_on_device(&device)? != expected {
                    return Err("Settings restoration mismatch".into());
                }
                Ok(())
            })();
            Err(settings_apply_error(&error, restore, &path).into())
        }
    }
}

/// Guarded one-setting transaction with an explicit recovery outcome.
pub fn apply_setting_detailed(
    expected: &crate::nia87::settings::Settings,
    setting: crate::nia87::settings::Setting,
    backup_dir: &std::path::Path,
) -> std::result::Result<crate::nia87::settings::Settings, byakko_core::session::ApplyFailure> {
    detailed(apply_setting(expected, setting, backup_dir))
}
