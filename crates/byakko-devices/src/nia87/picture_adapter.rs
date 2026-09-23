//! Per-key RGB projection for the Nia87's matrix-indexed native picture.
use super::{adapter, board, device, layout};
use byakko_core::{
    picture,
    session::{ApplyFailure, Recovery},
};
use std::{collections::BTreeMap, path::Path};

const BACKEND_ID: &str = "nia87";

fn key_by_slot() -> BTreeMap<usize, String> {
    layout::nia87_keys()
        .into_iter()
        .filter_map(|key| {
            let slot = board::slot_for_usage(key.usage)?;
            (slot < 126).then(|| (slot, adapter::key_id(slot)))
        })
        .collect()
}

pub fn capabilities() -> picture::Capabilities {
    picture::Capabilities {
        backend_id: BACKEND_ID.into(),
        keys: key_by_slot().into_values().collect(),
    }
}

fn encode_revision(colors: &[[u8; 3]]) -> Result<Vec<u8>, String> {
    if colors.len() != 128 {
        return Err("Invalid Nia87 picture size".into());
    }
    Ok(colors.iter().flatten().copied().collect())
}

fn decode_revision(bytes: &[u8]) -> Result<Vec<[u8; 3]>, String> {
    if bytes.len() != 384 {
        return Err("Invalid Nia87 picture revision length".into());
    }
    Ok(bytes.as_chunks::<3>().0.to_vec())
}

fn project(colors: &[[u8; 3]]) -> Result<picture::Snapshot, String> {
    let revision = encode_revision(colors)?;
    let map = key_by_slot();
    let editable = map
        .iter()
        .map(|(slot, key)| (key.clone(), colors[*slot]))
        .collect();
    Ok(picture::Snapshot {
        backend_id: BACKEND_ID.into(),
        revision,
        content: picture::Content::Editable(editable),
    })
}

fn checked_native(snapshot: &picture::Snapshot) -> Result<Vec<[u8; 3]>, String> {
    if snapshot.backend_id != BACKEND_ID {
        return Err("Picture snapshot belongs to another backend".into());
    }
    let colors = decode_revision(&snapshot.revision)?;
    if project(&colors)? != *snapshot {
        return Err(
            "Nia87 picture snapshot differs from its revision; reload before editing".into(),
        );
    }
    Ok(colors)
}

pub fn read() -> Result<picture::Snapshot, String> {
    read_with(&device::Access::unique())
}

pub(super) fn read_with(access: &device::Access) -> Result<picture::Snapshot, String> {
    let colors = access.read_picture().map_err(|error| error.to_string())?;
    project(&colors)
}

pub fn apply(
    expected: &picture::Snapshot,
    desired: &BTreeMap<String, [u8; 3]>,
    backup: &Path,
) -> Result<picture::Snapshot, ApplyFailure> {
    apply_with(&device::Access::unique(), expected, desired, backup)
}

pub(super) fn apply_with(
    access: &device::Access,
    expected: &picture::Snapshot,
    desired: &BTreeMap<String, [u8; 3]>,
    backup: &Path,
) -> Result<picture::Snapshot, ApplyFailure> {
    let original = checked_native(expected).map_err(not_attempted)?;
    let map = key_by_slot();
    if desired.len() != map.len()
        || desired
            .keys()
            .any(|key| !map.values().any(|known| known == key))
    {
        return Err(not_attempted(
            "Picture edit has missing or unknown key IDs".into(),
        ));
    }
    let mut target = original.clone();
    for (slot, key) in map {
        let Some(color) = desired.get(&key) else {
            return Err(not_attempted("Picture edit is missing a key".into()));
        };
        target[slot] = *color;
    }
    let actual = access.apply_picture_detailed(&original, &target, backup)?;
    project(&actual).map_err(|message| ApplyFailure {
        message,
        recovery: Recovery::Unverified,
    })
}

fn not_attempted(message: String) -> ApplyFailure {
    ApplyFailure {
        message,
        recovery: Recovery::NotAttempted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_physical_key_has_a_unique_picture_slot() {
        let physical = layout::nia87_keys();
        let slots: Vec<_> = physical
            .iter()
            .map(|key| board::slot_for_usage(key.usage).expect("physical key has picture slot"))
            .collect();
        assert_eq!(physical.len(), 87);
        assert!(slots.iter().all(|slot| *slot < 126));
        assert_eq!(
            slots.iter().copied().collect::<BTreeSet<_>>().len(),
            slots.len()
        );
        assert_eq!(capabilities().keys.len(), physical.len());
    }

    #[test]
    fn native_mapping_covers_fn_and_last_writable_key_and_preserves_padding() {
        let colors: Vec<_> = (0..128)
            .map(|slot| [slot as u8, (slot + 1) as u8, 255])
            .collect();
        let snapshot = project(&colors).unwrap();
        let picture::Content::Editable(content) = &snapshot.content else {
            unreachable!()
        };
        assert_eq!(content[&adapter::key_id(59)], colors[59]); // physical Fn key
        assert_eq!(content[&adapter::key_id(93)], colors[93]); // final mapped physical key
        assert_eq!(checked_native(&snapshot).unwrap(), colors);
        assert_eq!(
            &decode_revision(&snapshot.revision).unwrap()[126..],
            &colors[126..]
        );
        assert!(capabilities().keys.contains(&adapter::key_id(59)));
    }

    #[test]
    fn forged_content_or_revision_is_rejected_before_native_apply() {
        let colors = vec![[0; 3]; 128];
        let mut snapshot = project(&colors).unwrap();
        if let picture::Content::Editable(ref mut content) = snapshot.content {
            content.insert(adapter::key_id(0), [1, 2, 3]);
        }
        assert!(checked_native(&snapshot).is_err());
        snapshot = project(&colors).unwrap();
        snapshot.revision.pop();
        assert!(checked_native(&snapshot).is_err());
        assert!(project(&[[0; 3]; 127]).is_err());
    }
}
