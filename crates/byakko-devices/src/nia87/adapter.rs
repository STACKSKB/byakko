//! Nia87-specific translation between generic actions and complete raw snapshots.
use crate::nia87::{
    actions, board,
    device::{self, Snapshot},
    layout, macro_adapter,
};
use byakko_core::{
    Action, ActionChoice, Change, Descriptor, Layer, PhysicalKey, ShortcutCapabilities, State,
    UsageChoice, macros, validate_changes, validate_state,
};
use std::{collections::BTreeMap, path::Path};

pub const BACKEND_ID: &str = "nia87";
const LAYERS: [&str; 2] = ["base", "fn"];

#[derive(Default)]
pub struct Nia87Adapter;

/// One immutable HID target belongs to one desktop executor lifetime.
pub struct BoundNia87Adapter {
    access: device::Access,
}

impl BoundNia87Adapter {
    pub fn new(target: device::Target) -> Self {
        Self {
            access: device::Access::bound(target),
        }
    }
}

pub fn key_id(slot: usize) -> String {
    format!("slot-{slot:03}")
}

fn preset_id(bytes: [u8; 4]) -> String {
    format!(
        "{BACKEND_ID}:{:02X}{:02X}{:02X}{:02X}",
        bytes[0], bytes[1], bytes[2], bytes[3]
    )
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
    if let Some(preset) = actions::presets().iter().find(|p| p.bytes == raw) {
        return Action::Named {
            id: preset_id(preset.bytes),
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
            .iter()
            .find(|preset| preset_id(preset.bytes) == *id || preset.label == id)
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
    choices.extend(actions::presets().iter().map(|preset| ActionChoice {
        label: preset.label.into(),
        action: action_from_raw(preset.bytes),
    }));
    let mut usages: Vec<u8> = layout::nia87_keys()
        .into_iter()
        .filter(|key| key.usage != layout::FN_PLACEHOLDER_USAGE)
        .map(|key| key.usage)
        .collect();
    usages.extend(0x68..=0x73); // F13 through F24.
    // Standard keyboard-page outputs absent from the physical TKL board.
    // These use the ordinary four-byte key action, not a new firmware opcode.
    usages.extend(0x53..=0x63); // Numeric keypad through decimal.
    usages.extend([0x32, 0x64, 0x67, 0xe7]); // ISO keys, keypad equals, right Win.
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
    let shortcut_keys = choices
        .iter()
        .filter_map(|choice| match choice.action {
            Action::Key(usage) if usage < 224 => Some(UsageChoice {
                label: choice.label.clone(),
                usage,
            }),
            _ => None,
        })
        .collect();
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
        shortcuts: Some(ShortcutCapabilities {
            modifiers: [("Ctrl", 224), ("Shift", 225), ("Alt", 226), ("Win", 227)]
                .into_iter()
                .map(|(label, usage)| UsageChoice {
                    label: label.into(),
                    usage,
                })
                .collect(),
            keys: shortcut_keys,
            min_modifiers: 1,
            max_modifiers: 2,
        }),
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
    // Match the complete wire state represented by the revision. This also
    // accepts older clients whose named IDs were English display labels.
    let original = revision_snapshot(expected)?;
    if to_snapshot(expected)? != original {
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

impl Nia87Adapter {
    pub fn descriptor(&self) -> Descriptor {
        descriptor()
    }
    pub fn validate(&self, expected: &State, changes: &[Change]) -> Result<(), String> {
        draft_snapshot(expected, changes).map(|_| ())
    }
    pub fn read(&self) -> Result<State, String> {
        device::Access::unique()
            .snapshot()
            .map_err(|e| e.to_string())
            .and_then(|snapshot| from_snapshot(&snapshot))
    }
    pub fn apply(
        &self,
        expected: &State,
        changes: &[Change],
        backup_dir: &Path,
    ) -> Result<State, String> {
        self.apply_detailed(expected, changes, backup_dir)
            .map_err(|error| error.message)
    }
}

impl Nia87Adapter {
    pub fn apply_detailed(
        &self,
        expected: &State,
        changes: &[Change],
        backup_dir: &Path,
    ) -> Result<State, byakko_core::session::ApplyFailure> {
        apply_detailed_with(&device::Access::unique(), expected, changes, backup_dir)
    }
}

fn apply_detailed_with(
    access: &device::Access,
    expected: &State,
    changes: &[Change],
    backup_dir: &Path,
) -> Result<State, byakko_core::session::ApplyFailure> {
    use byakko_core::session::{ApplyFailure, Recovery};
    let prepare = || -> Result<_, String> {
        Ok((
            revision_snapshot(expected)?,
            draft_snapshot(expected, changes)?,
        ))
    };
    let (original, draft) = prepare().map_err(|message| ApplyFailure {
        message,
        recovery: Recovery::NotAttempted,
    })?;
    let actual =
        access.apply_keymaps_detailed(&original, &draft.base, &draft.function, backup_dir)?;
    from_snapshot(&actual).map_err(|message| ApplyFailure {
        message,
        recovery: Recovery::Unverified,
    })
}

impl crate::Device for Nia87Adapter {
    fn read(&mut self) -> Result<State, String> {
        Nia87Adapter::read(self)
    }

    fn apply(
        &mut self,
        expected: &State,
        changes: &[Change],
        backup_dir: &Path,
    ) -> Result<State, byakko_core::session::ApplyFailure> {
        self.apply_detailed(expected, changes, backup_dir)
    }

    fn read_macro(&mut self, slot: &str) -> Result<macros::Snapshot, String> {
        macro_adapter::read(slot)
    }

    fn apply_macro(
        &mut self,
        expected: &macros::Snapshot,
        desired: &macros::Program,
        backup_dir: &Path,
    ) -> Result<macros::Snapshot, byakko_core::session::ApplyFailure> {
        macro_adapter::apply(expected, desired, backup_dir)
    }

    fn read_lighting(&mut self) -> Result<byakko_core::lighting::Snapshot, String> {
        crate::nia87::lighting_adapter::read()
    }

    fn apply_lighting(
        &mut self,
        expected: &byakko_core::lighting::Snapshot,
        desired: &byakko_core::lighting::Setting,
        backup_dir: &Path,
    ) -> Result<byakko_core::lighting::Snapshot, byakko_core::session::ApplyFailure> {
        crate::nia87::lighting_adapter::apply(expected, desired, backup_dir)
    }

    fn read_picture(&mut self) -> Result<byakko_core::picture::Snapshot, String> {
        crate::nia87::picture_adapter::read()
    }

    fn apply_picture(
        &mut self,
        expected: &byakko_core::picture::Snapshot,
        desired: &BTreeMap<String, [u8; 3]>,
        backup_dir: &Path,
    ) -> Result<byakko_core::picture::Snapshot, byakko_core::session::ApplyFailure> {
        crate::nia87::picture_adapter::apply(expected, desired, backup_dir)
    }

    fn read_settings(&mut self) -> Result<byakko_core::settings::Snapshot, String> {
        crate::nia87::settings_adapter::read()
    }

    fn apply_setting(
        &mut self,
        expected: &byakko_core::settings::Snapshot,
        edit: &byakko_core::settings::Edit,
        backup_dir: &Path,
    ) -> Result<byakko_core::settings::Snapshot, byakko_core::session::ApplyFailure> {
        crate::nia87::settings_adapter::apply(expected, edit, backup_dir)
    }

    fn archive_capabilities(&self) -> Option<byakko_core::archive::ArchiveCapabilities> {
        Some(crate::nia87::archive_adapter::capabilities())
    }

    fn capture_archive(&mut self) -> Result<byakko_core::archive::NativeArchive, String> {
        crate::nia87::archive_adapter::capture()
    }

    fn review_archive(
        &mut self,
        target: &byakko_core::archive::NativeArchive,
    ) -> Result<byakko_core::archive::Review, String> {
        crate::nia87::archive_adapter::review(target)
    }

    fn apply_archive(
        &mut self,
        expected: &byakko_core::archive::NativeArchive,
        target: &byakko_core::archive::NativeArchive,
        backup_dir: &Path,
    ) -> Result<byakko_core::archive::NativeArchive, byakko_core::session::ApplyFailure> {
        crate::nia87::archive_adapter::apply(expected, target, backup_dir)
    }
}

impl crate::Device for BoundNia87Adapter {
    fn read(&mut self) -> Result<State, String> {
        self.access
            .snapshot()
            .map_err(|error| error.to_string())
            .and_then(|snapshot| from_snapshot(&snapshot))
    }

    fn apply(
        &mut self,
        expected: &State,
        changes: &[Change],
        backup_dir: &Path,
    ) -> Result<State, byakko_core::session::ApplyFailure> {
        apply_detailed_with(&self.access, expected, changes, backup_dir)
    }

    fn read_macro(&mut self, slot: &str) -> Result<macros::Snapshot, String> {
        macro_adapter::read_with(&self.access, slot)
    }

    fn apply_macro(
        &mut self,
        expected: &macros::Snapshot,
        desired: &macros::Program,
        backup_dir: &Path,
    ) -> Result<macros::Snapshot, byakko_core::session::ApplyFailure> {
        macro_adapter::apply_with(&self.access, expected, desired, backup_dir)
    }

    fn read_lighting(&mut self) -> Result<byakko_core::lighting::Snapshot, String> {
        crate::nia87::lighting_adapter::read_with(&self.access)
    }

    fn apply_lighting(
        &mut self,
        expected: &byakko_core::lighting::Snapshot,
        desired: &byakko_core::lighting::Setting,
        backup_dir: &Path,
    ) -> Result<byakko_core::lighting::Snapshot, byakko_core::session::ApplyFailure> {
        crate::nia87::lighting_adapter::apply_with(&self.access, expected, desired, backup_dir)
    }

    fn start_host_lighting(
        &mut self,
        mode: byakko_core::lighting::HostMode,
        setting: Option<byakko_core::lighting::Setting>,
        expected: &byakko_core::lighting::Snapshot,
        backup_dir: &Path,
    ) -> Result<Box<dyn crate::HostActivity>, byakko_core::session::ApplyFailure> {
        crate::nia87::host_adapter::start(&self.access, mode, setting, expected, backup_dir)
    }

    fn read_picture(&mut self) -> Result<byakko_core::picture::Snapshot, String> {
        crate::nia87::picture_adapter::read_with(&self.access)
    }

    fn apply_picture(
        &mut self,
        expected: &byakko_core::picture::Snapshot,
        desired: &BTreeMap<String, [u8; 3]>,
        backup_dir: &Path,
    ) -> Result<byakko_core::picture::Snapshot, byakko_core::session::ApplyFailure> {
        crate::nia87::picture_adapter::apply_with(&self.access, expected, desired, backup_dir)
    }

    fn read_settings(&mut self) -> Result<byakko_core::settings::Snapshot, String> {
        crate::nia87::settings_adapter::read_with(&self.access)
    }

    fn apply_setting(
        &mut self,
        expected: &byakko_core::settings::Snapshot,
        edit: &byakko_core::settings::Edit,
        backup_dir: &Path,
    ) -> Result<byakko_core::settings::Snapshot, byakko_core::session::ApplyFailure> {
        crate::nia87::settings_adapter::apply_with(&self.access, expected, edit, backup_dir)
    }

    fn archive_capabilities(&self) -> Option<byakko_core::archive::ArchiveCapabilities> {
        Some(crate::nia87::archive_adapter::capabilities())
    }

    fn capture_archive(&mut self) -> Result<byakko_core::archive::NativeArchive, String> {
        crate::nia87::archive_adapter::capture_with(&self.access)
    }

    fn review_archive(
        &mut self,
        target: &byakko_core::archive::NativeArchive,
    ) -> Result<byakko_core::archive::Review, String> {
        crate::nia87::archive_adapter::review_with(&self.access, target)
    }

    fn apply_archive(
        &mut self,
        expected: &byakko_core::archive::NativeArchive,
        target: &byakko_core::archive::NativeArchive,
        backup_dir: &Path,
    ) -> Result<byakko_core::archive::NativeArchive, byakko_core::session::ApplyFailure> {
        crate::nia87::archive_adapter::apply_with(&self.access, expected, target, backup_dir)
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
                id: "nia87:0300CD00".into()
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
        let choices = descriptor().actions;
        for (label, expected) in [
            ("Browser Back", [3, 0, 0x24, 2]),
            ("(", [0, 0, 0xe5, 0x26]),
            (")", [0, 0, 0xe5, 0x27]),
            ("Win+E", [0, 0, 0xe3, 0x08]),
            ("Win+Tab", [0, 0, 0xe3, 0x2b]),
            ("Win+D", [0, 0, 0xe3, 0x07]),
            ("Lock Screen", [0, 0, 0xe3, 0x0f]),
        ] {
            let choice = choices
                .iter()
                .find(|choice| choice.label == label)
                .expect("selected Nia87 keyboard catalog action");
            assert_eq!(raw_from_action(&choice.action).unwrap(), expected);
        }
        for choice in choices {
            assert_eq!(
                action_from_raw(raw_from_action(&choice.action).unwrap()),
                choice.action,
                "{}",
                choice.label
            );
        }
    }
    #[test]
    fn preset_identity_is_independent_of_its_display_label() {
        let raw = [3, 0, 205, 0];
        let named = action_from_raw(raw);
        assert_eq!(
            named,
            Action::Named {
                id: "nia87:0300CD00".into()
            }
        );
        assert_eq!(raw_from_action(&named).unwrap(), raw);
        assert_eq!(
            raw_from_action(&Action::Named {
                id: "Play/Pause".into()
            })
            .unwrap(),
            raw
        );
        let choice = descriptor()
            .actions
            .into_iter()
            .find(|choice| choice.label == "Play/Pause")
            .unwrap();
        assert_eq!(choice.action, named);
    }
    #[test]
    fn legacy_named_expected_state_still_passes_exact_revision_preflight() {
        let mut state = from_snapshot(&snapshot()).unwrap();
        state.bindings.get_mut("base").unwrap().insert(
            key_id(9),
            Action::Named {
                id: "Play/Pause".into(),
            },
        );
        let change = Change {
            layer: "base".into(),
            key: key_id(9),
            action: Action::Disabled,
        };
        assert_eq!(draft_snapshot(&state, &[change]).unwrap().base[9], [0; 4]);

        state
            .bindings
            .get_mut("base")
            .unwrap()
            .insert(key_id(9), Action::Named { id: "Mute".into() });
        assert!(draft_snapshot(&state, &[]).is_err());
    }
    #[test]
    fn shortcut_choices_cover_editable_ordinary_keys_and_round_trip() {
        let descriptor = descriptor();
        let shortcuts = descriptor.shortcuts.as_ref().unwrap();
        assert_eq!((shortcuts.min_modifiers, shortcuts.max_modifiers), (1, 2));
        assert_eq!(
            shortcuts
                .modifiers
                .iter()
                .map(|choice| choice.usage)
                .collect::<Vec<_>>(),
            vec![224, 225, 226, 227]
        );
        let advertised_keys: Vec<_> = descriptor
            .actions
            .iter()
            .filter_map(|choice| match choice.action {
                Action::Key(usage) if usage < 224 => Some((choice.label.as_str(), usage)),
                _ => None,
            })
            .collect();
        assert_eq!(
            shortcuts
                .keys
                .iter()
                .map(|choice| (choice.label.as_str(), choice.usage))
                .collect::<Vec<_>>(),
            advertised_keys
        );
        assert!(byakko_core::session::Session::new(descriptor).is_ok());
        for modifiers in [vec![224], vec![224, 225]] {
            let action = Action::Shortcut { modifiers, key: 6 };
            assert_eq!(action_from_raw(raw_from_action(&action).unwrap()), action);
        }
    }
    #[test]
    fn standard_non_tkl_key_choices_use_ordinary_action_encoding() {
        let descriptor = descriptor();
        for (label, usage) in [
            ("Non-US #", 0x32),
            ("Numpad 1", 0x59),
            ("Numpad 0", 0x62),
            ("Numpad =", 0x67),
            ("RWin", 0xe7),
        ] {
            let choice = descriptor
                .actions
                .iter()
                .find(|choice| choice.label == label)
                .expect("assignable standard keyboard usage");
            assert_eq!(choice.action, Action::Key(usage));
            assert_eq!(
                raw_from_action(&choice.action).unwrap(),
                [0, 0, usage as u8, 0]
            );
            assert_eq!(action_from_raw([0, 0, usage as u8, 0]), choice.action);
        }
        assert_eq!(descriptor.keys.iter().filter(|key| key.visible).count(), 87);
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
