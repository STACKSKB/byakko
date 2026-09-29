use super::apply_error::{ApplyResult, not_attempted, settings_apply_error};
use super::transaction::{VerifiedStep, apply_roundtrip, pacing, save_json_backup};
use super::*;

pub fn read_settings() -> Result<byakko_protocol::nia87::settings::Settings> {
    read_settings_with(Selection::Unique)
}

pub(super) fn read_settings_with(
    selection: Selection<'_>,
) -> Result<byakko_protocol::nia87::settings::Settings> {
    let session = Session::open_for(selection)?;
    read_settings_on_device(session.device())
}

pub(super) fn read_settings_on_device(
    device: &HidDevice,
) -> Result<byakko_protocol::nia87::settings::Settings> {
    let replies = [0x91, 0x97, 0x92, 0x86]
        .into_iter()
        .map(|opcode| read_payload(device, opcode, 0, 0))
        .collect::<Result<Vec<_>>>()?;
    Ok(byakko_protocol::nia87::settings::Settings::decode(
        &replies[0],
        &replies[1],
        &replies[2],
        &replies[3],
    )?)
}

pub fn apply_setting(
    expected: &byakko_protocol::nia87::settings::Settings,
    setting: byakko_protocol::nia87::settings::Setting,
    backup_dir: &std::path::Path,
) -> ApplyResult<byakko_protocol::nia87::settings::Settings> {
    apply_setting_with(Selection::Unique, expected, setting, backup_dir)
}

pub(super) fn apply_setting_with(
    selection: Selection<'_>,
    expected: &byakko_protocol::nia87::settings::Settings,
    setting: byakko_protocol::nia87::settings::Setting,
    backup_dir: &std::path::Path,
) -> ApplyResult<byakko_protocol::nia87::settings::Settings> {
    use byakko_protocol::nia87::settings::SettingPlan;
    let _lock = transaction_lock().map_err(not_attempted)?;
    let SettingPlan {
        target,
        report,
        restore_report,
    } = expected.plan_change(setting).map_err(not_attempted)?;
    if &target == expected {
        return Ok(target);
    }
    let (_, device) = selection.open().map_err(not_attempted)?;
    let backup = save_json_backup(
        backup_dir,
        "settings-before",
        &serde_json::json!({"format_version":1,"before":expected,"target":target}),
    )
    .map_err(not_attempted)?;
    let send = |data: &[u8; 64]| -> Result<()> {
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(data);
        device.send_setter(&host)?;
        std::thread::sleep(pacing::SETTING_SETTER);
        Ok(())
    };
    apply_roundtrip(
        &backup,
        VerifiedStep {
            write: || send(&report),
            matches: |actual: &byakko_protocol::nia87::settings::Settings| *actual == target,
            mismatch: "Setting readback mismatch",
        },
        VerifiedStep {
            write: || send(&restore_report),
            matches: |actual: &byakko_protocol::nia87::settings::Settings| actual == expected,
            mismatch: "Settings restoration mismatch",
        },
        || read_settings_on_device(&device),
        settings_apply_error,
    )
}
