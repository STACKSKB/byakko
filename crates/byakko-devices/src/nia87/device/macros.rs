use super::apply_error::{detailed, macro_apply_error};
use super::transport::FeatureSetter;
use super::{HidDevice, Result, Selection, Session, read_payload, transaction_lock};
use serde_json;
pub fn read_macro(slot: u8) -> Result<Vec<u8>> {
    read_macro_with(Selection::Unique, slot)
}

pub(super) fn read_macro_with(selection: Selection<'_>, slot: u8) -> Result<Vec<u8>> {
    let session = Session::open_for(selection)?;
    read_macro_on_device(session.device(), slot)
}

pub(super) fn read_macros_with(selection: Selection<'_>, slots: &[u8]) -> Result<Vec<Vec<u8>>> {
    let session = Session::open_for(selection)?;
    slots
        .iter()
        .map(|slot| read_macro_on_device(session.device(), *slot))
        .collect()
}

pub(super) fn read_macro_on_device(device: &HidDevice, slot: u8) -> Result<Vec<u8>> {
    crate::rongyuan::yc500::macro_io::read_complete(slot, |opcode, index, page| {
        read_payload(device, opcode, index, page)
    })
}

pub(super) fn write_macro_bytes(device: &HidDevice, slot: u8, bytes: &[u8]) -> Result<()> {
    for report in crate::nia87::macros::write_reports(slot, bytes)? {
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(&report);
        device.send_setter(&host)?;
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    Ok(())
}

/// The existing macro transaction with explicit recovery status.
pub fn apply_macro_detailed(
    slot: u8,
    expected: &[u8],
    new_macro: &crate::nia87::macros::Macro,
    backup_dir: &std::path::Path,
) -> std::result::Result<Vec<u8>, byakko_core::session::ApplyFailure> {
    detailed(apply_macro(slot, expected, new_macro, backup_dir))
}

pub fn apply_macro(
    slot: u8,
    expected: &[u8],
    new_macro: &crate::nia87::macros::Macro,
    backup_dir: &std::path::Path,
) -> Result<Vec<u8>> {
    apply_macro_with(Selection::Unique, slot, expected, new_macro, backup_dir)
}

pub(super) fn apply_macro_with(
    selection: Selection<'_>,
    slot: u8,
    expected: &[u8],
    new_macro: &crate::nia87::macros::Macro,
    backup_dir: &std::path::Path,
) -> Result<Vec<u8>> {
    let _lock = transaction_lock()?;
    let target = crate::nia87::macros::encode(new_macro)?;
    crate::nia87::macros::decode(expected)?; // Refuse to overwrite an unrecognized store we cannot restore.
    if target == expected {
        return Ok(target);
    }
    std::fs::create_dir_all(backup_dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = backup_dir.join(format!("macro-{slot}-before-{stamp}.json"));
    let mut backup = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    serde_json::to_writer_pretty(
        &mut backup,
        &serde_json::json!({"slot":slot,"bytes":expected}),
    )?;
    backup.sync_all()?;
    let (_, device) = selection.open()?;
    let result = (|| -> Result<Vec<u8>> {
        write_macro_bytes(&device, slot, &target)?;
        let mut actual = read_macro_on_device(&device, slot)?;
        if actual != target {
            // The first copy after the setter can straddle a flash transition.
            // Retry only a mismatch, once, before treating it as a failed save.
            std::thread::sleep(std::time::Duration::from_millis(200));
            actual = read_macro_on_device(&device, slot)?;
        }
        if actual != target {
            return Err("Macro readback mismatch".into());
        }
        Ok(actual)
    })();
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let rollback = (|| -> Result<()> {
                write_macro_bytes(&device, slot, expected)?;
                if read_macro_on_device(&device, slot)? != expected {
                    return Err("macro restoration mismatch".into());
                }
                Ok(())
            })();
            Err(macro_apply_error(error.as_ref(), rollback, &path).into())
        }
    }
}
