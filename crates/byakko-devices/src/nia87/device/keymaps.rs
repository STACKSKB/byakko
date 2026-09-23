use super::apply_error::keymap_apply_error;
use super::*;

fn read_matrix(device: &HidDevice, opcode: u8, index: u8) -> Result<Vec<[u8; 4]>> {
    let mut bytes = Vec::with_capacity(512);
    for page in 0..8 {
        // Interleave a verified scalar read to detect unchanged stale responses.
        let barrier = read_payload(device, 0x80, 0, 0)?;
        if barrier[0] != 0x80 {
            return Err(
                "Version barrier returned unrelated data; close other configurators".into(),
            );
        }
        let data = read_payload(device, opcode, index, page)?;
        if data == barrier {
            return Err(format!("Stale response to matrix page {page}").into());
        }
        bytes.extend_from_slice(&data);
    }
    Ok(bytes.as_chunks::<4>().0.to_vec())
}

pub fn snapshot() -> Result<Snapshot> {
    snapshot_with(Selection::Unique)
}

pub fn snapshot_for(target: &Target) -> Result<Snapshot> {
    snapshot_with(Selection::Expected(target))
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
    if base != read_matrix(device, 0x89, p[1])? || function != read_matrix(device, 0x90, 0)? {
        return Err("Keymap changed between repeated reads; backup not trusted".into());
    }
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
    std::thread::sleep(std::time::Duration::from_secs(1));
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

pub fn apply_keymaps_detailed_for(
    target: &Target,
    expected: &Snapshot,
    base: &[[u8; 4]],
    function: &[[u8; 4]],
    backup_dir: &std::path::Path,
) -> std::result::Result<Snapshot, byakko_core::session::ApplyFailure> {
    detailed(apply_keymaps_for(
        target, expected, base, function, backup_dir,
    ))
}

pub fn apply_keymaps(
    expected: &Snapshot,
    base: &[[u8; 4]],
    function: &[[u8; 4]],
    backup_dir: &std::path::Path,
) -> Result<Snapshot> {
    apply_keymaps_with(Selection::Unique, expected, base, function, backup_dir)
}

pub fn apply_keymaps_for(
    target: &Target,
    expected: &Snapshot,
    base: &[[u8; 4]],
    function: &[[u8; 4]],
    backup_dir: &std::path::Path,
) -> Result<Snapshot> {
    apply_keymaps_with(
        Selection::Expected(target),
        expected,
        base,
        function,
        backup_dir,
    )
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
    let _lock = transaction_lock()?;
    let current = snapshot_unlocked(selection)?;
    if &current != expected {
        return Err(
            "Keyboard changed since it was loaded. Reload before applying; no writes sent.".into(),
        );
    }
    if current.firmware != 0x0100 || current.profile != 0 {
        return Err(
            "Firmware/profile differs from validated Nia87 0x0100/profile 0; no keymap writes sent"
                .into(),
        );
    }
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
        return Ok(current);
    }
    std::fs::create_dir_all(backup_dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = backup_dir.join(format!("keymaps-before-{stamp}.json"));
    let mut backup = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    serde_json::to_writer_pretty(&mut backup, &current)?;
    backup.sync_all()?;
    let (_, device) = selection.open()?;
    // The write handle is opened after the backup. Recheck its identity too,
    // so a reconnect cannot put these reports onto an unvalidated device.
    let version = read_payload(&device, 0x80, 0, 0)?;
    let profile = read_payload(&device, 0x85, 0, 0)?;
    if version[0] != 0x80
        || u16::from_le_bytes([version[1], version[2]]) != 0x0100
        || profile[0] != 0x85
        || profile[1] != 0
    {
        return Err(
            "Write handle is not validated Nia87 firmware 0x0100/profile 0; no writes sent".into(),
        );
    }
    if snapshot_on_device(&device)? != current {
        return Err("Keyboard changed before keymap write; no writes sent".into());
    }
    let result = (|| -> Result<Snapshot> {
        // The official helper's captured final HID report for a Fn binding is
        // the single-key 0x15 command with index 0.
        for &(is_fn, slot, _, new) in &changes {
            if is_fn {
                continue;
            }
            write_binding(&device, false, current.profile, slot, new)?;
        }
        for &(is_fn, slot, _, new) in &changes {
            if is_fn {
                write_binding(&device, true, 0, slot, new)?;
            }
        }
        let actual = snapshot_unlocked(selection)?;
        if actual.base != base
            || actual.function != function
            || actual.firmware != current.firmware
            || actual.profile != current.profile
        {
            let mismatch_path = backup_dir.join(format!("keymaps-mismatch-{stamp}.json"));
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
    })();
    match result {
        Ok(actual) => Ok(actual),
        Err(error) => {
            let rollback = (|| -> Result<()> {
                // A failed setter may have changed either map. Read both maps
                // before recovery, restore Fn first, then reread both maps
                // because those Fn writes might also have affected base.
                for (is_fn, profile, attempted, original) in [
                    (true, 0, function, current.function.as_slice()),
                    (false, current.profile, base, current.base.as_slice()),
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
                if snapshot_unlocked(selection)? != current {
                    return Err("restored data could not be verified".into());
                }
                Ok(())
            })();
            Err(keymap_apply_error(error.as_ref(), rollback, &path).into())
        }
    }
}
