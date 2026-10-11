use super::apply_error::{ApplyResult, macro_apply_error, not_attempted};
use super::transaction::{
    VerifiedStep, after_prior_setter, apply_roundtrip, pacing, save_json_backup,
};
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

#[derive(Debug)]
struct MacroPageError {
    slot: u8,
    page: usize,
    source: Box<dyn std::error::Error + Send + Sync>,
}

impl std::fmt::Display for MacroPageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Macro {}, write page {}: {}",
            self.slot, self.page, self.source
        )
    }
}

impl std::error::Error for MacroPageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

fn submit_macro_bytes(
    slot: u8,
    bytes: &[u8],
    mut send: impl FnMut(&[u8; 65]) -> Result<()>,
    mut wait: impl FnMut(std::time::Duration),
) -> Result<()> {
    let mut prior_setter_sent = false;
    for (page, report) in byakko_protocol::nia87::macros::write_reports(slot, bytes)?
        .into_iter()
        .enumerate()
    {
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(&report);
        send(&host).map_err(|error| MacroPageError {
            slot,
            page,
            source: after_prior_setter(error, prior_setter_sent),
        })?;
        prior_setter_sent = true;
        wait(pacing::MACRO_PAGE);
    }
    wait(pacing::MACRO_SETTLE);
    Ok(())
}

pub(super) fn write_macro_bytes(device: &HidDevice, slot: u8, bytes: &[u8]) -> Result<()> {
    submit_macro_bytes(
        slot,
        bytes,
        |report| device.send_setter(report),
        std::thread::sleep,
    )
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

#[cfg(test)]
mod submit_tests {
    use super::super::transaction::no_logical_setter_sent;
    use super::super::transport::SetterNotAttempted;
    use super::*;
    use byakko_protocol::nia87::receiver::RelayError;

    #[test]
    fn receiver_offline_on_first_macro_page_differs_from_later_page() {
        let bytes = [0; 256];
        for failed_page in [0, 2] {
            let mut sends = 0;
            let error = submit_macro_bytes(
                3,
                &bytes,
                |_| {
                    let page = sends;
                    sends += 1;
                    if page == failed_page {
                        Err(Box::new(SetterNotAttempted::new(
                            RelayError::KeyboardOffline.into(),
                        )))
                    } else {
                        Ok(())
                    }
                },
                |_| {},
            )
            .unwrap_err();
            assert_eq!(sends, failed_page + 1);
            assert_eq!(no_logical_setter_sent(error.as_ref()), failed_page == 0);
            assert!(
                error
                    .to_string()
                    .contains(&format!("Macro 3, write page {failed_page}"))
            );
        }
    }
}
