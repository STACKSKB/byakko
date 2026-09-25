use super::apply_error::{RestoreMismatch, keymap_apply_error};
use super::transaction::{apply_with_recovery, pacing, save_json_backup};
use super::*;

fn read_matrix(device: &HidDevice, opcode: u8, index: u8) -> Result<Vec<[u8; 4]>> {
    read_matrix_with(opcode, index, |opcode, index, page| {
        read_payload(device, opcode, index, page)
    })
}

fn read_matrix_with(
    opcode: u8,
    index: u8,
    mut exchange: impl FnMut(u8, u8, u8) -> Result<[u8; 64]>,
) -> Result<Vec<[u8; 4]>> {
    let mut bytes = Vec::with_capacity(512);
    for page in 0..8 {
        let data = exchange(opcode, index, page)?;
        bytes.extend_from_slice(&data);
    }
    Ok(bytes.as_chunks::<4>().0.to_vec())
}

pub fn snapshot() -> Result<Snapshot> {
    snapshot_with(Selection::Unique)
}

pub(super) fn snapshot_with(selection: Selection<'_>) -> Result<Snapshot> {
    let session = Session::open_for(selection)?;
    snapshot_on_device(session.device())
}

pub(super) fn snapshot_unlocked(selection: Selection<'_>) -> Result<Snapshot> {
    let (_, device) = selection.open()?;
    snapshot_on_device(&device)
}

pub(super) fn snapshot_on_device(device: &HidDevice) -> Result<Snapshot> {
    let v = read_payload(device, 0x80, 0, 0)?;
    let p = read_payload(device, 0x85, 0, 0)?;
    if v[0] != 0x80 || p[0] != 0x85 {
        return Err("Unrelated identity response; close other configurators".into());
    }
    let base = read_matrix(device, 0x89, p[1])?;
    let function = read_matrix(device, 0x90, 0)?;
    Ok(Snapshot {
        format_version: 1,
        firmware: u16::from_le_bytes([v[1], v[2]]),
        profile: p[1],
        base,
        function,
    })
}

pub(super) fn write_binding(
    device: &HidDevice,
    function: bool,
    index: u8,
    slot: usize,
    binding: [u8; 4],
) -> Result<()> {
    if slot >= 126 {
        return Err("The last two configuration slots are read-only padding".into());
    }
    let mut payload = [0u8; 65];
    payload[1..].copy_from_slice(&crate::nia87::protocol::single_key_report(
        function, index, slot, binding,
    )?);
    device.send_setter(&payload)?;
    // Firmware 0100 can cross-write base/Fn values when setters are only
    // 100 ms apart. A 1 s interval passed mixed-layer write/restore tests.
    std::thread::sleep(pacing::KEYMAP_SETTER);
    Ok(())
}

/// The same guarded transaction as `apply_keymaps`, with a typed recovery
/// outcome for callers that must distinguish verified restore from failure.
pub fn apply_keymaps_detailed(
    expected: &Snapshot,
    base: &[[u8; 4]],
    function: &[[u8; 4]],
    backup_dir: &std::path::Path,
) -> std::result::Result<Snapshot, byakko_core::session::ApplyFailure> {
    detailed(apply_keymaps(expected, base, function, backup_dir))
}

pub fn apply_keymaps(
    expected: &Snapshot,
    base: &[[u8; 4]],
    function: &[[u8; 4]],
    backup_dir: &std::path::Path,
) -> Result<Snapshot> {
    apply_keymaps_with(Selection::Unique, expected, base, function, backup_dir)
}

pub(super) fn apply_keymaps_with(
    selection: Selection<'_>,
    expected: &Snapshot,
    base: &[[u8; 4]],
    function: &[[u8; 4]],
    backup_dir: &std::path::Path,
) -> Result<Snapshot> {
    if expected.format_version != 1
        || expected.base.len() != 128
        || expected.function.len() != 128
        || base.len() != 128
        || function.len() != 128
    {
        return Err("Invalid keymap shape or backup format".into());
    }
    if expected.base[126..] != base[126..] || expected.function[126..] != function[126..] {
        return Err("Cannot modify reserved padding slots".into());
    }
    crate::nia87::keymap_policy::validate_changes(
        &expected.base,
        &expected.function,
        base,
        function,
    )?;
    if expected.firmware != 0x0100 || expected.profile != 0 {
        return Err(
            "Firmware/profile differs from validated Nia87 0x0100/profile 0; no keymap writes sent"
                .into(),
        );
    }
    let _lock = transaction_lock()?;
    let changes: Vec<_> = (0..126)
        .flat_map(|slot| {
            [
                (false, slot, expected.base[slot], base[slot]),
                (true, slot, expected.function[slot], function[slot]),
            ]
        })
        .filter(|(_, _, old, new)| old != new)
        .collect();
    if changes.is_empty() {
        return Ok(expected.clone());
    }
    let backup = save_json_backup(backup_dir, "keymaps-before", expected)?;
    let (_, device) = selection.open()?;
    apply_with_recovery(
        &backup,
        || -> Result<Snapshot> {
            // The official helper's captured final HID report for a Fn binding is
            // the single-key 0x15 command with index 0.
            for &(is_fn, slot, _, new) in &changes {
                if is_fn {
                    continue;
                }
                write_binding(&device, false, expected.profile, slot, new)?;
            }
            for &(is_fn, slot, _, new) in &changes {
                if is_fn {
                    write_binding(&device, true, 0, slot, new)?;
                }
            }
            let actual = snapshot_on_device(&device)?;
            if actual.base != base
                || actual.function != function
                || actual.firmware != expected.firmware
                || actual.profile != expected.profile
            {
                let mismatch_path =
                    backup_dir.join(format!("keymaps-mismatch-{}.json", backup.stamp()));
                let mut mismatch = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(mismatch_path)?;
                serde_json::to_writer_pretty(
                    &mut mismatch,
                    &serde_json::json!({"actual":actual,"desired_base":base,"desired_function":function}),
                )?;
                mismatch.sync_all()?;
                return Err("Readback does not match the complete intended keymaps".into());
            }
            Ok(actual)
        },
        || -> Result<()> {
            // A failed setter may have changed either map. Read both maps
            // before recovery, restore Fn first, then reread both maps
            // because those Fn writes might also have affected base.
            for (is_fn, profile, attempted, original) in [
                (true, 0, function, expected.function.as_slice()),
                (false, expected.profile, base, expected.base.as_slice()),
            ] {
                let observed = snapshot_unlocked(selection).ok();
                let observed_map = observed.as_ref().map(|snapshot| {
                    if is_fn {
                        snapshot.function.as_slice()
                    } else {
                        snapshot.base.as_slice()
                    }
                });
                // Without a trustworthy read, restore only planned changes.
                // Complete readback below must still prove restoration.
                let slots = crate::nia87::recovery_keymaps::slots_to_restore(
                    observed_map,
                    attempted,
                    original,
                )?;
                for slot in slots {
                    write_binding(&device, is_fn, profile, slot, original[slot])?;
                }
            }
            if snapshot_on_device(&device)? != *expected {
                return Err(RestoreMismatch("restored data could not be verified").into());
            }
            Ok(())
        },
        keymap_apply_error,
    )
}

#[cfg(test)]
mod tests {
    use super::read_matrix_with;

    #[test]
    fn matrix_read_requests_each_page_once() {
        let mut calls = Vec::new();
        let matrix = read_matrix_with(0x89, 2, |opcode, index, page| {
            calls.push((opcode, index, page));
            Ok([page; 64])
        })
        .unwrap();
        assert_eq!(matrix.len(), 128);
        assert_eq!(
            calls,
            (0..8).map(|page| (0x89, 2, page)).collect::<Vec<_>>()
        );
        assert_eq!(matrix[0], [0; 4]);
        assert_eq!(matrix[127], [7; 4]);
    }
}
