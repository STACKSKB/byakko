//! Nia87-specific translation between generic actions and complete raw snapshots.
use super::{
    Action, ActionChoice, Change, Descriptor, KeymapBackend, Layer, PhysicalKey, State,
    validate_changes, validate_state,
};
use crate::{
    actions, board,
    device::{self, Snapshot},
    layout,
};
use std::{collections::BTreeMap, path::Path};

pub const BACKEND_ID: &str = "nia87";
const LAYERS: [&str; 2] = ["base", "fn"];

#[derive(Default)]
pub struct Nia87Adapter;

pub fn key_id(slot: usize) -> String {
    format!("slot-{slot:03}")
}

pub fn action_from_raw(raw: [u8; 4]) -> Action {
    if raw == [0; 4] {
        return Action::Disabled;
    }
    if raw[0] == 0 && raw[1] == 0 && raw[3] == 0 && raw[2] != 0 {
        return Action::Key(raw[2] as u16);
    }
    if raw[0] == 9 && raw[1] <= 2 && raw[2] < 50 && raw[3] == 0 {
        return Action::Macro {
            slot: raw[2] as u16,
            mode: raw[1],
        };
    }
    if let Some(preset) = actions::presets().into_iter().find(|p| p.bytes == raw) {
        return Action::Named {
            id: preset.label.to_owned(),
        };
    }
    if raw[0] == 0 && (224..=227).contains(&raw[1]) {
        if raw[3] == 0 && raw[2] != 0 && raw[2] < 224 {
            return Action::Shortcut {
                modifiers: vec![raw[1] as u16],
                key: raw[2] as u16,
            };
        }
        if (224..=227).contains(&raw[2]) && raw[1] != raw[2] && raw[3] != 0 && raw[3] < 224 {
            return Action::Shortcut {
                modifiers: vec![raw[1] as u16, raw[2] as u16],
                key: raw[3] as u16,
            };
        }
    }
    Action::Opaque {
        backend_id: BACKEND_ID.into(),
        data: raw.into(),
        label: format!("Raw {:02X?}", raw),
    }
}

pub fn raw_from_action(action: &Action) -> Result<[u8; 4], String> {
    match action {
        Action::Key(usage) if *usage > 0 && *usage <= u8::MAX as u16 => {
            Ok(actions::key_binding(*usage as u8))
        }
        Action::Key(_) => Err("Nia87 key usage must be 1..=255".into()),
        Action::Disabled => Ok([0; 4]),
        Action::Macro { slot, mode } => {
            let slot = u8::try_from(*slot).map_err(|_| "Macro slot exceeds Nia87 range")?;
            actions::macro_binding(slot, *mode)
        }
        Action::Shortcut { modifiers, key } => {
            let key = u8::try_from(*key).map_err(|_| "Shortcut key exceeds Nia87 range")?;
            if key == 0 || key >= 224 {
                return Err("Shortcut target must be an ordinary keyboard usage".into());
            }
            match modifiers.as_slice() {
                [modifier] => {
                    let modifier = u8::try_from(*modifier)
                        .map_err(|_| "Shortcut modifier exceeds Nia87 range")?;
                    actions::combo_binding(modifier, key)
                }
                [first, second] => {
                    let first = u8::try_from(*first)
                        .map_err(|_| "Shortcut modifier exceeds Nia87 range")?;
                    let second = u8::try_from(*second)
                        .map_err(|_| "Shortcut modifier exceeds Nia87 range")?;
                    actions::two_modifier_binding(first, second, key)
                }
                _ => Err("Nia87 shortcuts need one or two modifiers".into()),
            }
        }
        Action::Named { id } => actions::presets()
            .into_iter()
            .find(|preset| preset.label == id)
            .map(|preset| preset.bytes)
            .ok_or_else(|| format!("Unknown Nia87 action: {id}")),
        Action::Opaque {
            backend_id, data, ..
        } if backend_id == BACKEND_ID && data.len() == 4 => {
            Ok(data.as_slice().try_into().expect("length checked"))
        }
        Action::Opaque { .. } => {
            Err("Opaque action belongs to another backend or has invalid length".into())
        }
    }
}

pub fn descriptor() -> Descriptor {
    let mut keys: Vec<PhysicalKey> = (0..128)
        .map(|slot| PhysicalKey {
            id: key_id(slot),
            label: format!("Reserved {slot}"),
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            visible: false,
            writable: slot < 126,
        })
        .collect();
    for key in layout::nia87_keys() {
        if let Some(slot) = board::slot_for_usage(key.usage) {
            keys[slot] = PhysicalKey {
                id: key_id(slot),
                label: key.label.into(),
                x: key.x,
                y: key.y,
                width: key.width,
                height: 1.0,
                visible: true,
                writable: key.usage != layout::FN_PLACEHOLDER_USAGE,
            };
        }
    }
    let mut choices = vec![ActionChoice {
        label: "Disabled".into(),
        action: Action::Disabled,
    }];
    choices.extend(actions::presets().into_iter().map(|preset| ActionChoice {
        label: preset.label.into(),
        action: action_from_raw(preset.bytes),
    }));
    let mut usages: Vec<u8> = layout::nia87_keys()
        .into_iter()
        .filter(|key| key.usage != layout::FN_PLACEHOLDER_USAGE)
        .map(|key| key.usage)
        .collect();
    usages.extend(0x68..=0x73); // F13 through F24.
    usages.sort_unstable();
    usages.dedup();
    choices.extend(usages.into_iter().map(|usage| ActionChoice {
        label: layout::usage_label(usage),
        action: Action::Key(usage as u16),
    }));
    for (label, modifier) in [("Ctrl", 224), ("Shift", 225), ("Alt", 226), ("Win", 227)] {
        choices.push(ActionChoice {
            label: format!("{label}+A"),
            action: Action::Shortcut {
                modifiers: vec![modifier],
                key: 4,
            },
        });
    }
    Descriptor {
        backend_id: BACKEND_ID.into(),
        device_name: "Nia87".into(),
        keys,
        layers: vec![
            Layer {
                id: LAYERS[0].into(),
                label: "Base".into(),
            },
            Layer {
                id: LAYERS[1].into(),
                label: "Fn".into(),
            },
        ],
        actions: choices,
    }
}

fn validate_snapshot(snapshot: &Snapshot) -> Result<(), String> {
    if snapshot.format_version != 1 || snapshot.base.len() != 128 || snapshot.function.len() != 128
    {
        return Err("Unsupported Nia87 snapshot format or keymap size".into());
    }
    if snapshot.firmware != 0x0100 || snapshot.profile != 0 {
        return Err("Nia87 firmware/profile differs from validated 0x0100/profile 0".into());
    }
    Ok(())
}

pub fn from_snapshot(snapshot: &Snapshot) -> Result<State, String> {
    validate_snapshot(snapshot)?;
    let mut bindings = BTreeMap::new();
    for (layer, records) in [(LAYERS[0], &snapshot.base), (LAYERS[1], &snapshot.function)] {
        bindings.insert(
            layer.into(),
            records
                .iter()
                .enumerate()
                .map(|(slot, &raw)| (key_id(slot), action_from_raw(raw)))
                .collect(),
        );
    }
    Ok(State {
        revision: serde_json::to_vec(snapshot).map_err(|e| e.to_string())?,
        bindings,
    })
}

fn revision_snapshot(state: &State) -> Result<Snapshot, String> {
    let snapshot: Snapshot = serde_json::from_slice(&state.revision)
        .map_err(|_| "Invalid Nia87 revision token".to_owned())?;
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

/// Translate a complete state into a raw snapshot, retaining identity and every reserved slot.
pub fn to_snapshot(state: &State) -> Result<Snapshot, String> {
    let descriptor = descriptor();
    validate_state(&descriptor, state)?;
    let mut snapshot = revision_snapshot(state)?;
    for (layer, records) in [
        (LAYERS[0], &mut snapshot.base),
        (LAYERS[1], &mut snapshot.function),
    ] {
        for (slot, raw) in records.iter_mut().enumerate() {
            let action = &state.bindings[layer][&key_id(slot)];
            let encoded = raw_from_action(action)?;
            if !descriptor.keys[slot].writable && encoded != *raw {
                return Err(format!("Cannot modify reserved slot {slot}"));
            }
            *raw = encoded;
        }
    }
    Ok(snapshot)
}

pub fn draft_snapshot(expected: &State, changes: &[Change]) -> Result<Snapshot, String> {
    validate_changes(&descriptor(), changes)?;
    // Enforce that the purported expected bindings are the state represented by the revision.
    let original = revision_snapshot(expected)?;
    if from_snapshot(&original)?.bindings != expected.bindings {
        return Err("Nia87 state differs from its revision; reload before editing".into());
    }
    let mut draft = expected.clone();
    for change in changes {
        raw_from_action(&change.action)?;
        draft
            .bindings
            .get_mut(&change.layer)
            .expect("validated layer")
            .insert(change.key.clone(), change.action.clone());
    }
    to_snapshot(&draft)
}

impl KeymapBackend for Nia87Adapter {
    fn descriptor(&self) -> Descriptor {
        descriptor()
    }
    fn validate(&self, expected: &State, changes: &[Change]) -> Result<(), String> {
        draft_snapshot(expected, changes).map(|_| ())
    }
    fn read(&self) -> Result<State, String> {
        device::snapshot()
            .map_err(|e| e.to_string())
            .and_then(|snapshot| from_snapshot(&snapshot))
    }
    fn apply(
        &self,
        expected: &State,
        changes: &[Change],
        backup_dir: &Path,
    ) -> Result<State, String> {
        let original = revision_snapshot(expected)?;
        let draft = draft_snapshot(expected, changes)?;
        let actual = device::apply_keymaps(&original, &draft.base, &draft.function, backup_dir)
            .map_err(|e| e.to_string())?;
        from_snapshot(&actual)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        let mut base = vec![[0, 0, 4, 0]; 128];
        base[126] = [255, 1, 2, 3];
        base[9] = [3, 0, 205, 0];
        Snapshot {
            format_version: 1,
            firmware: 0x100,
            profile: 0,
            base,
            function: vec![[7, 8, 9, 10]; 128],
        }
    }
    #[test]
    fn round_trips_every_raw_slot() {
        let raw = snapshot();
        let state = from_snapshot(&raw).unwrap();
        assert_eq!(
            state.bindings["base"]["slot-009"],
            Action::Named {
                id: "Play/Pause".into()
            }
        );
        assert_eq!(to_snapshot(&state).unwrap(), raw);
        assert_eq!(descriptor().keys.iter().filter(|k| k.visible).count(), 87);
        assert!(!descriptor().keys[59].writable);
    }
    #[test]
    fn rejects_stale_or_hidden_edit() {
        let mut state = from_snapshot(&snapshot()).unwrap();
        state
            .bindings
            .get_mut("base")
            .unwrap()
            .insert(key_id(9), Action::Disabled);
        assert!(draft_snapshot(&state, &[]).is_err());
        let state = from_snapshot(&snapshot()).unwrap();
        assert!(
            draft_snapshot(
                &state,
                &[Change {
                    layer: "base".into(),
                    key: key_id(126),
                    action: Action::Disabled
                }]
            )
            .is_err()
        );
        assert!(
            draft_snapshot(
                &state,
                &[Change {
                    layer: "other".into(),
                    key: key_id(9),
                    action: Action::Disabled
                }]
            )
            .is_err()
        );
    }
    #[test]
    fn shortcuts_round_trip_and_validate() {
        let state = from_snapshot(&snapshot()).unwrap();
        for modifiers in [vec![224], vec![224, 225]] {
            let change = Change {
                layer: "base".into(),
                key: key_id(9),
                action: Action::Shortcut {
                    modifiers: modifiers.clone(),
                    key: 4,
                },
            };
            let draft = draft_snapshot(&state, &[change]).unwrap();
            assert_eq!(
                from_snapshot(&draft).unwrap().bindings["base"][&key_id(9)],
                Action::Shortcut { modifiers, key: 4 }
            );
        }
        assert!(
            draft_snapshot(
                &state,
                &[Change {
                    layer: "base".into(),
                    key: key_id(59),
                    action: Action::Disabled
                }]
            )
            .is_err()
        );
    }
    #[test]
    fn catalog_choices_match_decoded_bindings() {
        for choice in descriptor().actions {
            assert_eq!(
                action_from_raw(raw_from_action(&choice.action).unwrap()),
                choice.action,
                "{}",
                choice.label
            );
        }
    }
    #[test]
    fn rejects_unvalidated_identity() {
        let mut raw = snapshot();
        raw.firmware = 0x0101;
        assert!(from_snapshot(&raw).is_err());
        raw.firmware = 0x0100;
        raw.profile = 1;
        assert!(from_snapshot(&raw).is_err());
    }
}
