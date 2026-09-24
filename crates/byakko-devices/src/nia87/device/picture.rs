use super::apply_error::picture_apply_error;
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
/// Replace custom picture colors, preserving every unedited matrix slot.
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
        let actual = read_picture_on_device(device)?;
        if actual != desired {
            return Err(mismatch_diagnostic(
                &path,
                expected,
                desired,
                &actual,
                expected_context,
                &changes,
            )
            .into());
        }
        Ok(actual)
    })();
    match result {
        Ok(actual) => Ok(actual),
        Err(error) => {
            let restore = (|| -> Result<()> {
                write(expected)?;
                if read_picture_on_device(device)? != expected {
                    return Err("Picture restoration mismatch".into());
                }
                Ok(())
            })();
            Err(picture_apply_error(&error, restore, &path).into())
        }
    }
}

fn mismatch_diagnostic(
    backup: &std::path::Path,
    expected: &[[u8; 3]],
    desired: &[[u8; 3]],
    actual: &[[u8; 3]],
    context: Option<[u8; 2]>,
    changed_slots: &[usize],
) -> String {
    let details = mismatch_details(desired, actual, changed_slots);
    let evidence = backup.with_extension("failure.json");
    let save =
        write_mismatch_evidence(&evidence, expected, desired, actual, context, changed_slots);
    let evidence_status = match save {
        Ok(()) => format!("evidence {}", evidence.display()),
        Err(error) => format!("evidence could not be saved: {error}"),
    };
    format!(
        "Picture readback mismatch: {} edited and {} untouched slots differ; {}; {evidence_status}",
        details.intended, details.collateral, details.examples
    )
}

#[derive(Debug, Eq, PartialEq)]
struct MismatchDetails {
    intended: usize,
    collateral: usize,
    examples: String,
}

fn mismatch_details(
    desired: &[[u8; 3]],
    actual: &[[u8; 3]],
    changed_slots: &[usize],
) -> MismatchDetails {
    let mismatches: Vec<_> = desired
        .iter()
        .zip(actual)
        .enumerate()
        .filter_map(|(slot, (wanted, got))| (wanted != got).then_some((slot, wanted, got)))
        .collect();
    let (intended, collateral) =
        mismatches
            .iter()
            .fold((0, 0), |(intended, collateral), (slot, _, _)| {
                if changed_slots.contains(slot) {
                    (intended + 1, collateral)
                } else {
                    (intended, collateral + 1)
                }
            });
    let examples = mismatches
        .iter()
        .take(4)
        .map(|(slot, wanted, got)| {
            let kind = if changed_slots.contains(slot) {
                "edited"
            } else {
                "untouched"
            };
            format!("slot {slot} ({kind}): wanted {wanted:?}, got {got:?}")
        })
        .collect::<Vec<_>>()
        .join("; ");
    MismatchDetails {
        intended,
        collateral,
        examples,
    }
}

fn write_mismatch_evidence(
    path: &std::path::Path,
    expected: &[[u8; 3]],
    desired: &[[u8; 3]],
    actual: &[[u8; 3]],
    context: Option<[u8; 2]>,
    changed_slots: &[usize],
) -> Result<()> {
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    serde_json::to_writer_pretty(
        &mut output,
        &serde_json::json!({
            "format_version": 1,
            "expected": expected,
            "desired": desired,
            "actual": actual,
            "context_revision": context,
            "changed_slots": changed_slots,
        }),
    )?;
    output.sync_all()?;
    Ok(())
}

/// The guarded picture transaction with an explicit recovery result.
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
    fn mismatch_details_separate_edited_and_untouched_slots() {
        let mut desired = vec![[0; 3]; 128];
        desired[9] = [255, 0, 0];
        let same = mismatch_details(&desired, &desired, &[9]);
        assert_eq!(same.intended, 0);
        assert_eq!(same.collateral, 0);

        let mut actual = desired.clone();
        actual[9] = [0, 0, 0];
        let missed_write = mismatch_details(&desired, &actual, &[9]);
        assert_eq!(missed_write.intended, 1);
        assert_eq!(missed_write.collateral, 0);
        assert!(missed_write.examples.contains("slot 9 (edited)"));

        actual[10] = [0, 255, 0];
        let collateral = mismatch_details(&desired, &actual, &[9]);
        assert_eq!(collateral.intended, 1);
        assert_eq!(collateral.collateral, 1);
        assert!(collateral.examples.contains("slot 10 (untouched)"));
    }

    #[test]
    fn mismatch_message_survives_evidence_write_failure() {
        let missing = std::env::temp_dir().join(format!(
            "byakko-missing-picture-evidence-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let expected = vec![[0; 3]; 128];
        let mut desired = expected.clone();
        desired[1] = [4, 5, 6];
        let message = mismatch_diagnostic(
            &missing.join("before.json"),
            &expected,
            &desired,
            &expected,
            Some([13, 0]),
            &[1],
        );
        assert!(message.contains("Picture readback mismatch"));
        assert!(message.contains("evidence could not be saved"));
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
}
