use super::apply_error::picture_submit_error;
use super::*;

/// Read the current custom lighting picture as 128 matrix-indexed RGB values.
pub fn read_picture() -> Result<Vec<[u8; 3]>> {
    read_picture_with(Selection::Unique)
}

pub(super) fn read_picture_with(selection: Selection<'_>) -> Result<Vec<[u8; 3]>> {
    let session = Session::open_for(selection)?;
    read_picture_on_device(session.device())
}

pub(super) fn read_picture_with_context(
    selection: Selection<'_>,
) -> Result<(Vec<[u8; 3]>, [u8; 2])> {
    let session = Session::open_for(selection)?;
    read_picture_with_context_on_device(session.device())
}

fn picture_context(device: &HidDevice) -> Result<[u8; 2]> {
    let lighting = read_lighting_on_device(device)?;
    Ok(lighting.picture_context())
}

fn read_picture_with_context_on_device(device: &HidDevice) -> Result<(Vec<[u8; 3]>, [u8; 2])> {
    let context = picture_context(device)?;
    Ok((read_picture_on_device(device)?, context))
}

pub(super) fn read_picture_on_device(device: &HidDevice) -> Result<Vec<[u8; 3]>> {
    read_picture_pages(|opcode, index, page| read_payload(device, opcode, index, page))
}

fn read_picture_pages(
    mut exchange: impl FnMut(u8, u8, u8) -> Result<[u8; 64]>,
) -> Result<Vec<[u8; 3]>> {
    let pages = (0..6)
        .map(|page| exchange(0x8c, 0, page))
        .collect::<Result<Vec<_>>>()?;
    Ok(crate::nia87::lighting::user_picture_from_pages(&pages)?)
}
/// Submit a complete custom picture from a cached before-image.
/// A successful return means all seven USB reports were accepted, not that an
/// immediate getter verified firmware persistence.
pub fn apply_picture(
    expected: &[[u8; 3]],
    desired: &[[u8; 3]],
    backup_dir: &std::path::Path,
) -> Result<Vec<[u8; 3]>> {
    apply_picture_with(Selection::Unique, expected, desired, None, backup_dir)
}

pub(super) fn apply_picture_with(
    selection: Selection<'_>,
    expected: &[[u8; 3]],
    desired: &[[u8; 3]],
    expected_context: Option<[u8; 2]>,
    backup_dir: &std::path::Path,
) -> Result<Vec<[u8; 3]>> {
    if expected.len() != 128 || desired.len() != 128 || expected[126..] != desired[126..] {
        return Err("Invalid picture size or reserved-slot modification".into());
    }
    let physical_slots = crate::nia87::board::physical_slot_mask();
    if (0..126).any(|slot| expected[slot] != desired[slot] && !physical_slots[slot]) {
        return Err("Picture edit changes an unmapped matrix slot".into());
    }
    if !(0..126).any(|slot| expected[slot] != desired[slot]) {
        return Ok(expected.to_vec());
    }
    let reports = crate::nia87::lighting::user_picture_write_reports(desired)?;
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
        &serde_json::json!({
            "format_version": 2,
            "colors": expected,
            "desired": desired,
            "context_revision": expected_context,
        }),
    )?;
    backup.sync_all()?;
    let session = Session::open_for(selection)?;
    let device = session.device();
    submit_picture_reports(
        &reports,
        &path,
        |host| device.send_setter(host),
        || {
            // The captured official path schedules two 10 ms waits before
            // each page. This is pacing, not a readback or retry interval.
            std::thread::sleep(std::time::Duration::from_millis(20));
        },
    )?;
    Ok(desired.to_vec())
}

fn submit_picture_reports(
    reports: &[[u8; 64]; 7],
    backup: &std::path::Path,
    mut send: impl FnMut(&[u8; 65]) -> Result<()>,
    mut schedule: impl FnMut(),
) -> Result<()> {
    for report in reports {
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(report);
        schedule();
        if let Err(error) = send(&host) {
            return Err(picture_submit_error(error.as_ref(), backup).into());
        }
    }
    Ok(())
}

/// Picture submission with typed transport uncertainty on a failed send.
pub fn apply_picture_detailed(
    expected: &[[u8; 3]],
    desired: &[[u8; 3]],
    backup_dir: &std::path::Path,
) -> std::result::Result<Vec<[u8; 3]>, byakko_core::session::ApplyFailure> {
    detailed(apply_picture(expected, desired, backup_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::session::Recovery;

    #[test]
    fn picture_upload_sends_seven_pages_and_stops_on_transport_failure() {
        let mut colors = vec![[0; 3]; 128];
        colors[28] = [215, 0, 0];
        let reports = crate::nia87::lighting::user_picture_write_reports(&colors).unwrap();
        let backup = std::path::Path::new("picture-before.json");
        let mut sent = Vec::new();
        let schedules = std::cell::Cell::new(0);
        submit_picture_reports(
            &reports,
            backup,
            |host| {
                sent.push(*host);
                Ok(())
            },
            || schedules.set(schedules.get() + 1),
        )
        .unwrap();
        assert_eq!(sent.len(), 7);
        assert_eq!(schedules.get(), 7);
        for (page, host) in sent.iter().enumerate() {
            assert_eq!(&host[..6], &[0, 0x0c, 0, 0x80, 1, page as u8]);
        }
        let payload: Vec<_> = sent
            .iter()
            .flat_map(|host| host[9..].iter().copied())
            .collect();
        assert_eq!(&payload[84..87], &[215, 0, 0]);
        assert_eq!(&payload[384..], &[0; 8]);

        let mut sends = 0;
        schedules.set(0);
        let result = submit_picture_reports(
            &reports,
            backup,
            |_| {
                sends += 1;
                if sends == 3 {
                    Err("Disconnected".into())
                } else {
                    Ok(())
                }
            },
            || schedules.set(schedules.get() + 1),
        );
        let failure = detailed(result).unwrap_err();
        assert_eq!(failure.recovery, Recovery::Unverified);
        assert!(failure.message.contains("Disconnected"));
        assert!(failure.message.contains("no automatic restore"));
        assert_eq!(sends, 3);
        assert_eq!(schedules.get(), 3);
    }

    #[test]
    fn picture_snapshot_reads_each_page_once_without_identity_queries() {
        let mut calls = Vec::new();
        let colors = read_picture_pages(|opcode, index, page| {
            calls.push((opcode, index, page));
            Ok([page; 64])
        })
        .unwrap();
        assert_eq!(
            calls,
            (0..6).map(|page| (0x8c, 0, page)).collect::<Vec<_>>()
        );
        assert_eq!(colors.len(), 128);
        assert_eq!(colors[0], [0; 3]);
        assert_eq!(colors[127], [5; 3]);
    }

    #[test]
    fn picture_read_stops_at_transport_error() {
        let mut calls = 0;
        let result = read_picture_pages(|_, _, page| {
            calls += 1;
            if page == 2 {
                Err("Disconnected".into())
            } else {
                Ok([0; 64])
            }
        });
        assert_eq!(calls, 3);
        assert_eq!(result.unwrap_err().to_string(), "Disconnected");
    }

    #[test]
    fn picture_context_tracks_effect_and_option_but_not_brightness() {
        let mut reply = [0u8; 64];
        reply[0] = crate::nia87::lighting::LED_READ_COMMAND;
        reply[1] = 13;
        reply[4] = 0x10;
        let option_two = crate::nia87::lighting::Lighting::decode(&reply).unwrap();
        assert_eq!(option_two.picture_context(), [13, 1]);
        reply[3] = 2;
        assert_eq!(
            crate::nia87::lighting::Lighting::decode(&reply)
                .unwrap()
                .picture_context(),
            [13, 1]
        );
        reply[4] = 0x20;
        assert_eq!(
            crate::nia87::lighting::Lighting::decode(&reply)
                .unwrap()
                .picture_context(),
            [13, 2]
        );
        reply[1] = 1;
        assert_eq!(
            crate::nia87::lighting::Lighting::decode(&reply)
                .unwrap()
                .picture_context(),
            [1, 2]
        );
    }

    #[test]
    fn unmapped_picture_slot_is_rejected_before_device_access() {
        let physical = crate::nia87::board::physical_slot_mask();
        let unmapped = (0..126).find(|&slot| !physical[slot]).unwrap();
        let expected = vec![[0; 3]; 128];
        let mut desired = expected.clone();
        desired[unmapped] = [1, 2, 3];
        let failure = apply_picture_detailed(
            &expected,
            &desired,
            std::path::Path::new("unused-backup-path"),
        )
        .unwrap_err();
        assert_eq!(failure.recovery, Recovery::NotAttempted);
        assert!(failure.message.contains("unmapped matrix slot"));
    }

    #[test]
    fn wrong_size_and_reserved_slot_are_rejected_before_sending() {
        let path = std::path::Path::new("unused-backup-path");
        let expected = vec![[0; 3]; 128];
        let short = apply_picture_detailed(&expected[..127], &expected, path).unwrap_err();
        assert_eq!(short.recovery, Recovery::NotAttempted);

        let mut reserved = expected.clone();
        reserved[126] = [1, 2, 3];
        let failure = apply_picture_detailed(&expected, &reserved, path).unwrap_err();
        assert_eq!(failure.recovery, Recovery::NotAttempted);
        assert!(failure.message.contains("reserved-slot"));
    }
}
