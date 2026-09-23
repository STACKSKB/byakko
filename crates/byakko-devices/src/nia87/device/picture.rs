use super::apply_error::picture_apply_error;
use super::*;

/// Read the current custom lighting picture as 128 matrix-indexed RGB values.
pub fn read_picture() -> Result<Vec<[u8; 3]>> {
    read_picture_with(Selection::Unique)
}

pub fn read_picture_for(target: &Target) -> Result<Vec<[u8; 3]>> {
    read_picture_with(Selection::Expected(target))
}

pub(super) fn read_picture_with(selection: Selection<'_>) -> Result<Vec<[u8; 3]>> {
    let session = Session::open_for(selection)?;
    read_picture_on_device(session.device())
}

fn read_picture_unlocked(selection: Selection<'_>) -> Result<Vec<[u8; 3]>> {
    let (_, device) = selection.open()?;
    read_picture_on_device(&device)
}

pub(super) fn read_picture_on_device(device: &HidDevice) -> Result<Vec<[u8; 3]>> {
    stable_picture_reads(|| {
        let mut pages = Vec::new();
        for page in 0..6 {
            let barrier = read_payload(device, 0x80, 0, 0)?;
            if barrier[0] != 0x80 {
                return Err("Picture identity barrier failed".into());
            }
            let bytes = read_payload(device, 0x8c, 0, page)?;
            if bytes == barrier {
                return Err("Picture read returned stale identity".into());
            }
            pages.push(bytes);
        }
        Ok(crate::nia87::lighting::user_picture_from_pages(&pages)?)
    })
}

pub(super) fn stable_picture_reads(
    mut read: impl FnMut() -> Result<Vec<[u8; 3]>>,
) -> Result<Vec<[u8; 3]>> {
    let mut previous = None;
    for _ in 0..3 {
        let colors = read()?;
        if colors.len() != 128 {
            return Err("Incomplete picture snapshot".into());
        }
        if previous.as_ref() == Some(&colors) {
            return Ok(colors);
        }
        previous = Some(colors);
    }
    Err("Picture did not stabilize across three complete reads".into())
}

/// Replace custom picture colors, preserving every unedited matrix slot.
pub fn apply_picture(
    expected: &[[u8; 3]],
    desired: &[[u8; 3]],
    backup_dir: &std::path::Path,
) -> Result<Vec<[u8; 3]>> {
    apply_picture_with(Selection::Unique, expected, desired, backup_dir)
}

pub fn apply_picture_for(
    target: &Target,
    expected: &[[u8; 3]],
    desired: &[[u8; 3]],
    backup_dir: &std::path::Path,
) -> Result<Vec<[u8; 3]>> {
    apply_picture_with(Selection::Expected(target), expected, desired, backup_dir)
}

pub(super) fn apply_picture_with(
    selection: Selection<'_>,
    expected: &[[u8; 3]],
    desired: &[[u8; 3]],
    backup_dir: &std::path::Path,
) -> Result<Vec<[u8; 3]>> {
    let _lock = transaction_lock()?;
    if expected.len() != 128 || desired.len() != 128 || expected[126..] != desired[126..] {
        return Err("Invalid picture size or reserved-slot modification".into());
    }
    let identity = snapshot_unlocked(selection)?;
    if identity.firmware != 0x0100 || identity.profile != 0 {
        return Err("Unverified firmware/profile; no picture writes sent".into());
    }
    if read_picture_unlocked(selection)? != expected {
        return Err("Picture changed since load; no writes sent".into());
    }
    let changes: Vec<_> = (0..126).filter(|&i| expected[i] != desired[i]).collect();
    if changes.is_empty() {
        return Ok(expected.to_vec());
    }
    std::fs::create_dir_all(backup_dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = backup_dir.join(format!("picture-before-{stamp}.json"));
    let mut backup = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    serde_json::to_writer_pretty(
        &mut backup,
        &serde_json::json!({"format_version":1,"colors":expected}),
    )?;
    backup.sync_all()?;
    let (_, device) = selection.open()?;
    let write_identity = snapshot_on_device(&device)?;
    if write_identity != identity {
        return Err(
            "Keyboard identity or keymaps changed before picture write; no writes sent".into(),
        );
    }
    if read_picture_on_device(&device)? != expected {
        return Err("Picture changed before picture write; no writes sent".into());
    }
    let write = |colors: &[[u8; 3]]| -> Result<()> {
        for &slot in &changes {
            let mut host = [0u8; 65];
            host[1..].copy_from_slice(&crate::nia87::lighting::per_key_color_report(
                0,
                slot as u8,
                colors[slot],
            )?);
            device.send_setter(&host)?;
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Ok(())
    };
    let result = (|| -> Result<Vec<[u8; 3]>> {
        write(desired)?;
        let actual = read_picture_unlocked(selection)?;
        if actual != desired {
            return Err("Picture readback mismatch".into());
        }
        Ok(actual)
    })();
    match result {
        Ok(actual) => Ok(actual),
        Err(error) => {
            let restore = (|| -> Result<()> {
                write(expected)?;
                if read_picture_unlocked(selection)? != expected {
                    return Err("Picture restoration mismatch".into());
                }
                Ok(())
            })();
            Err(picture_apply_error(&error, restore, &path).into())
        }
    }
}

/// The guarded picture transaction with an explicit recovery result.
pub fn apply_picture_detailed(
    expected: &[[u8; 3]],
    desired: &[[u8; 3]],
    backup_dir: &std::path::Path,
) -> std::result::Result<Vec<[u8; 3]>, byakko_core::session::ApplyFailure> {
    detailed(apply_picture(expected, desired, backup_dir))
}

pub fn apply_picture_detailed_for(
    target: &Target,
    expected: &[[u8; 3]],
    desired: &[[u8; 3]],
    backup_dir: &std::path::Path,
) -> std::result::Result<Vec<[u8; 3]>, byakko_core::session::ApplyFailure> {
    detailed(apply_picture_for(target, expected, desired, backup_dir))
}
