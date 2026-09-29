//! Per-key RGB projection for the Nia87's matrix-indexed native picture.
use super::{adapter, board, layout};
use byakko_core::model::picture;
use std::collections::BTreeMap;

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
        lighting_effect: Some("13".into()),
    }
}

pub fn encode_revision(colors: &[[u8; 3]]) -> Result<Vec<u8>, String> {
    if colors.len() != 128 {
        return Err("Invalid Nia87 picture size".into());
    }
    Ok(colors.iter().flatten().copied().collect())
}

pub fn decode_revision(bytes: &[u8]) -> Result<Vec<[u8; 3]>, String> {
    if bytes.len() != 384 {
        return Err("Invalid Nia87 picture revision length".into());
    }
    Ok(bytes.as_chunks::<3>().0.to_vec())
}

pub fn project(colors: &[[u8; 3]], context: [u8; 2]) -> Result<picture::Snapshot, String> {
    let revision = encode_revision(colors)?;
    let map = key_by_slot();
    let editable = map
        .iter()
        .map(|(slot, key)| (key.clone(), colors[*slot]))
        .collect();
    Ok(picture::Snapshot {
        evidence: picture::Evidence::Readback,
        backend_id: BACKEND_ID.into(),
        revision,
        context_revision: context.into(),
        content: picture::Content::Editable(editable),
    })
}

pub fn checked_native(snapshot: &picture::Snapshot) -> Result<(Vec<[u8; 3]>, [u8; 2]), String> {
    if snapshot.backend_id != BACKEND_ID {
        return Err("Picture snapshot belongs to another backend".into());
    }
    let colors = decode_revision(&snapshot.revision)?;
    let context: [u8; 2] =
        snapshot.context_revision.as_slice().try_into().map_err(
            |_| "Nia87 picture snapshot lacks its lighting selector; reload before editing",
        )?;
    let mut projected = project(&colors, context)?;
    projected.evidence = snapshot.evidence;
    if projected != *snapshot {
        return Err(
            "Nia87 picture snapshot differs from its revision; reload before editing".into(),
        );
    }
    Ok((colors, context))
}

/// Validate a complete editable map and retain all hidden raw color slots.
pub type NativePictureEdit = (Vec<[u8; 3]>, Vec<[u8; 3]>, [u8; 2]);

pub fn desired_native(
    expected: &picture::Snapshot,
    desired: &BTreeMap<String, [u8; 3]>,
) -> Result<NativePictureEdit, String> {
    let (original, context) = checked_native(expected)?;
    let map = key_by_slot();
    if desired.len() != map.len()
        || desired
            .keys()
            .any(|key| !map.values().any(|known| known == key))
    {
        return Err("Picture edit has missing or unknown key IDs".into());
    }
    let mut target = original.clone();
    for (slot, key) in map {
        target[slot] = *desired.get(&key).ok_or("Picture edit is missing a key")?;
    }
    Ok((original, target, context))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn advertised_picture_effect_exists_in_lighting_catalog() {
        let effect = capabilities()
            .lighting_effect
            .expect("Nia87 picture effect");
        assert_eq!(effect, "13");
        assert!(
            crate::nia87::lighting_adapter::capabilities()
                .effects
                .iter()
                .any(|choice| choice.id == effect && choice.label == "Per-key colors")
        );
    }

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
        let snapshot = project(&colors, [13, 0]).unwrap();
        let picture::Content::Editable(content) = &snapshot.content else {
            unreachable!()
        };
        assert_eq!(content[&adapter::key_id(59)], colors[59]); // physical Fn key
        assert_eq!(content[&adapter::key_id(93)], colors[93]); // final mapped physical key
        assert_eq!(
            checked_native(&snapshot).unwrap(),
            (colors.clone(), [13, 0])
        );
        assert_eq!(
            &decode_revision(&snapshot.revision).unwrap()[126..],
            &colors[126..]
        );
        assert!(capabilities().keys.contains(&adapter::key_id(59)));
    }

    #[test]
    fn forged_content_or_revision_is_rejected_before_native_apply() {
        let colors = vec![[0; 3]; 128];
        let mut snapshot = project(&colors, [13, 0]).unwrap();
        if let picture::Content::Editable(ref mut content) = snapshot.content {
            content.insert(adapter::key_id(0), [1, 2, 3]);
        }
        assert!(checked_native(&snapshot).is_err());
        snapshot = project(&colors, [13, 0]).unwrap();
        snapshot.revision.pop();
        assert!(checked_native(&snapshot).is_err());
        assert!(project(&[[0; 3]; 127], [13, 0]).is_err());
        snapshot = project(&colors, [13, 0]).unwrap();
        snapshot.context_revision.clear();
        assert!(checked_native(&snapshot).is_err());
    }
}
