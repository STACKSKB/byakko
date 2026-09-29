use super::apply_error::{ApplyResult, macro_apply_error, not_attempted};
use super::transaction::{VerifiedStep, apply_roundtrip, pacing, save_json_backup};
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
            .map_err(|error| format!("Macro {slot}, read page {page}: {error}").into())
    })
}

pub(super) fn write_macro_bytes(device: &HidDevice, slot: u8, bytes: &[u8]) -> Result<()> {
    for (page, report) in byakko_protocol::nia87::macros::write_reports(slot, bytes)?
        .into_iter()
        .enumerate()
    {
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(&report);
        device
            .send_setter(&host)
            .map_err(|error| format!("Macro {slot}, write page {page}: {error}"))?;
        std::thread::sleep(pacing::MACRO_PAGE);
    }
    std::thread::sleep(pacing::MACRO_SETTLE);
    Ok(())
}

pub fn apply_macro(
    slot: u8,
    expected: &[u8],
    new_macro: &byakko_protocol::nia87::macros::Macro,
    backup_dir: &std::path::Path,
) -> ApplyResult<Vec<u8>> {
    apply_macro_with(Selection::Unique, slot, expected, new_macro, backup_dir)
}

pub(super) fn apply_macro_with(
    selection: Selection<'_>,
    slot: u8,
    expected: &[u8],
    new_macro: &byakko_protocol::nia87::macros::Macro,
    backup_dir: &std::path::Path,
) -> ApplyResult<Vec<u8>> {
    let before = byakko_protocol::nia87::macros::ValidatedBeforeImage::validate(expected)
        .map_err(not_attempted)?;
    apply_macro_validated_with(selection, slot, &before, new_macro, backup_dir)
}

pub(super) fn apply_macro_validated_with(
    selection: Selection<'_>,
    slot: u8,
    before: &byakko_protocol::nia87::macros::ValidatedBeforeImage,
    new_macro: &byakko_protocol::nia87::macros::Macro,
    backup_dir: &std::path::Path,
) -> ApplyResult<Vec<u8>> {
    let _lock = transaction_lock().map_err(not_attempted)?;
    let target = byakko_protocol::nia87::macros::encode(new_macro).map_err(not_attempted)?;
    let expected = before.as_bytes();
    if target == expected {
        return Ok(target);
    }
    let backup = save_json_backup(
        backup_dir,
        &format!("macro-{slot}-before"),
        &serde_json::json!({"slot":slot,"bytes":expected}),
    )
    .map_err(not_attempted)?;
    let (_, device) = selection.open().map_err(not_attempted)?;
    apply_roundtrip(
        &backup,
        VerifiedStep {
            write: || write_macro_bytes(&device, slot, &target),
            matches: |actual: &Vec<u8>| *actual == target,
            mismatch: "Macro readback mismatch",
        },
        VerifiedStep {
            write: || write_macro_bytes(&device, slot, expected),
            matches: |actual: &Vec<u8>| actual == expected,
            mismatch: "macro restoration mismatch",
        },
        || read_macro_on_device(&device, slot),
        macro_apply_error,
    )
}
