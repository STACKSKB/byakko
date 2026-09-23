//! Pure preflight for restoring a complete Nia87 configuration archive.
//! A plan contains intent only; constructing one never contacts a device.

use crate::nia87::{
    actions, board,
    configuration::{self, Configuration},
    macros, profiles,
    settings::{self, Setting},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeSummary {
    pub key_bindings: usize,
    pub macro_slots: Vec<u8>,
    pub picture_keys: usize,
    pub lighting: bool,
    pub settings: Vec<Setting>,
}

pub type Result<T> = std::result::Result<T, String>;

/// Reject changes that cannot be represented by the existing typed setters.
/// The `current` value must be a fresh capture from the device being restored.
pub fn plan(current: &Configuration, target: &Configuration) -> Result<ChangeSummary> {
    configuration::validate(current).map_err(|e| format!("current configuration: {e}"))?;
    configuration::validate(target).map_err(|e| format!("target configuration: {e}"))?;
    profiles::validate_for_device(&target.keymaps, &current.keymaps)
        .map_err(|e| format!("keymap: {e}"))?;

    let mut key_bindings = 0;
    for (name, before, after) in [
        ("base", &current.keymaps.base, &target.keymaps.base),
        (
            "function",
            &current.keymaps.function,
            &target.keymaps.function,
        ),
    ] {
        for (index, (old, new)) in before.iter().zip(after).enumerate().take(126) {
            if new[0] == 9 {
                actions::macro_binding(new[2], new[1])
                    .map_err(|e| format!("{name} binding {index}: {e}"))?;
                if new[3] != 0 {
                    return Err(format!(
                        "{name} binding {index}: macro binding has nonzero reserved byte"
                    ));
                }
            }
            if old != new {
                key_bindings += 1;
            }
        }
    }

    let mut macro_slots = Vec::new();
    for (index, (old, new)) in current.macros.iter().zip(&target.macros).enumerate() {
        if old == new {
            continue;
        }
        for (label, bytes) in [("current", old), ("target", new)] {
            let decoded =
                macros::decode(bytes).map_err(|e| format!("macro slot {index} {label}: {e}"))?;
            if macros::encode(&decoded).map_err(|e| format!("macro slot {index} {label}: {e}"))?
                != *bytes
            {
                return Err(format!(
                    "macro slot {index} {label}: raw bytes cannot round trip through the macro codec"
                ));
            }
        }
        macro_slots.push(index as u8);
    }

    if current.picture[126..] != target.picture[126..] {
        return Err("reserved user-picture slots 126 and 127 cannot be written".into());
    }
    let physical_slots = board::physical_slot_mask();
    if (0..126).any(|slot| current.picture[slot] != target.picture[slot] && !physical_slots[slot]) {
        return Err("unmapped user-picture slot cannot be written".into());
    }
    let picture_keys = current
        .picture
        .iter()
        .zip(&target.picture)
        .take(126)
        .filter(|(a, b)| a != b)
        .count();
    let lighting = current.lighting.raw() != target.lighting.raw();
    if lighting {
        for (label, value) in [("current", &current.lighting), ("target", &target.lighting)] {
            if value.recognized_setting().is_none() {
                return Err(format!(
                    "{label} lighting mode cannot be represented by the supported lighting setter"
                ));
            }
        }
        for index in 8..64 {
            if current.lighting.raw()[index] != target.lighting.raw()[index] {
                return Err(format!("lighting opaque byte {index} differs"));
            }
        }
    }

    let mut changed_settings = Vec::new();
    for (opcode, writable, desired, restore) in [
        (
            settings::DEBOUNCE_READ,
            &[2][..],
            Setting::Debounce(target.settings.debounce()),
            Setting::Debounce(current.settings.debounce()),
        ),
        (
            settings::AUTO_OS_READ,
            &[1][..],
            Setting::AutoOs(target.settings.auto_os()),
            Setting::AutoOs(current.settings.auto_os()),
        ),
        (
            settings::SLEEP_READ,
            &[1, 2, 3, 4, 5, 6, 7, 8][..],
            Setting::Sleep(target.settings.sleep_seconds()),
            Setting::Sleep(current.settings.sleep_seconds()),
        ),
        (
            settings::OPTIONS_READ,
            &[2, 4][..],
            Setting::Backlight(target.settings.backlight_enabled()),
            Setting::Backlight(current.settings.backlight_enabled()),
        ),
    ] {
        let before = current.settings.raw_reply(opcode).unwrap();
        let after = target.settings.raw_reply(opcode).unwrap();
        for index in 0..settings::REPORT_LEN {
            if before[index] != after[index] && !writable.contains(&index) {
                return Err(format!(
                    "settings reply 0x{opcode:02x} opaque byte {index} differs"
                ));
            }
        }
        if before == after {
            continue;
        }
        if opcode == settings::AUTO_OS_READ && after[1] > 1 {
            return Err("auto-OS target must be canonical 0 or 1".into());
        }
        let forward = current
            .settings
            .plan_change(desired)
            .map_err(|e| format!("settings reply 0x{opcode:02x}: {e}"))?;
        let reverse = target
            .settings
            .plan_change(restore)
            .map_err(|e| format!("settings reply 0x{opcode:02x} rollback: {e}"))?;
        if opcode == settings::OPTIONS_READ {
            if forward.target.raw_reply(opcode) != Some(after) {
                return Err("options target cannot be produced by the backlight setter".into());
            }
            if reverse.target.raw_reply(opcode) != Some(before) {
                return Err(
                    "options current state cannot be restored by the backlight setter".into(),
                );
            }
        }
        changed_settings.push(desired);
    }

    Ok(ChangeSummary {
        key_bindings,
        macro_slots,
        picture_keys,
        lighting,
        settings: changed_settings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nia87::{
        device::Snapshot,
        lighting::{Lighting, LightingSetting},
        settings::Settings,
    };

    fn fixture() -> Configuration {
        let mut replies = [[0u8; 64]; 4];
        for (reply, opcode) in replies.iter_mut().zip([0x91, 0x97, 0x92, 0x86]) {
            reply[0] = opcode;
        }
        replies[0][2] = 1;
        replies[2][1..9].copy_from_slice(&[120, 0, 120, 0, 88, 2, 88, 2]);
        replies[3][2] = 0x10;
        let lighting_setting = LightingSetting {
            effect_id: 4,
            value: Some(2),
            speed: Some(2),
            option: Some(0),
            rgb: Some([1, 2, 3]),
            dazzle: false,
        };
        let mut raw = crate::nia87::lighting::write_report(&lighting_setting).unwrap();
        raw[0] = crate::nia87::lighting::LED_READ_COMMAND;
        Configuration {
            keymaps: Snapshot {
                format_version: 1,
                firmware: 0x100,
                profile: 0,
                base: vec![[0, 0, 4, 0]; 128],
                function: vec![[0, 0, 4, 0]; 128],
            },
            macros: vec![vec![0; 256]; 50],
            lighting: Lighting::decode(&raw).unwrap(),
            picture: vec![[0; 3]; 128],
            settings: Settings::decode(&replies[0], &replies[1], &replies[2], &replies[3]).unwrap(),
        }
    }

    fn set_reply(config: &mut Configuration, opcode: u8, index: usize, value: u8) {
        let mut replies =
            [0x91, 0x97, 0x92, 0x86].map(|op| config.settings.raw_reply(op).unwrap().to_vec());
        let which = [0x91, 0x97, 0x92, 0x86]
            .iter()
            .position(|op| *op == opcode)
            .unwrap();
        replies[which][index] = value;
        config.settings =
            Settings::decode(&replies[0], &replies[1], &replies[2], &replies[3]).unwrap();
    }

    #[test]
    fn plans_supported_changes() {
        let before = fixture();
        let mut after = before.clone();
        after.keymaps.base[0] = actions::macro_binding(49, 2).unwrap();
        after.macros[49] = macros::encode(&macros::Macro {
            repeat_count: 2,
            events: vec![],
        })
        .unwrap();
        after.picture[0] = [1, 2, 3];
        set_reply(&mut after, 0x91, 2, 4);
        set_reply(&mut after, 0x86, 2, 0);
        let summary = plan(&before, &after).unwrap();
        assert_eq!(summary.key_bindings, 1);
        assert_eq!(summary.macro_slots, vec![49]);
        assert_eq!(summary.picture_keys, 1);
        assert_eq!(
            summary.settings,
            vec![Setting::Debounce(4), Setting::Backlight(true)]
        );
    }

    #[test]
    fn rejects_opaque_and_unrestorable_changes() {
        let before = fixture();
        let mut after = before.clone();
        after.keymaps.base[126][0] = 1;
        assert!(plan(&before, &after).unwrap_err().contains("Reserved"));
        after = before.clone();
        after.keymaps.base[0] = [9, 3, 50, 0];
        assert!(plan(&before, &after).unwrap_err().contains("macro slot"));
        after = before.clone();
        after.macros[0][255] = 1;
        assert!(
            plan(&before, &after)
                .unwrap_err()
                .contains("macro slot 0 target")
        );
        after = before.clone();
        set_reply(&mut after, 0x91, 50, 1);
        assert!(
            plan(&before, &after)
                .unwrap_err()
                .contains("opaque byte 50")
        );
        after = before.clone();
        set_reply(&mut after, 0x86, 4, 1);
        assert!(
            plan(&before, &after)
                .unwrap_err()
                .contains("cannot be produced")
        );
    }

    #[test]
    fn reserved_picture_and_keymap_slots_are_not_writable() {
        let before = fixture();
        for slot in [126, 127] {
            let mut after = before.clone();
            after.picture[slot] = [1, 2, 3];
            assert!(
                plan(&before, &after)
                    .unwrap_err()
                    .contains("reserved user-picture")
            );
            let mut after = before.clone();
            after.keymaps.function[slot] = [1, 2, 3, 4];
            assert!(
                plan(&before, &after)
                    .unwrap_err()
                    .contains("Reserved keymap")
            );
        }
    }

    #[test]
    fn unmapped_picture_slots_are_archival_but_not_writable() {
        let before = fixture();
        let mask = board::physical_slot_mask();
        assert_eq!(mask.iter().filter(|&&mapped| mapped).count(), 87);
        let slot = (0..126).find(|&slot| !mask[slot]).unwrap();
        let mut after = before.clone();
        after.picture[slot] = [1, 2, 3];
        assert!(
            plan(&before, &after)
                .unwrap_err()
                .contains("unmapped user-picture")
        );
        assert_eq!(plan(&before, &before).unwrap().picture_keys, 0);
    }

    #[test]
    fn rejects_each_invalid_macro_binding_field() {
        let before = fixture();
        for binding in [[9, 0, 50, 0], [9, 3, 0, 0], [9, 0, 0, 1]] {
            let mut after = before.clone();
            after.keymaps.base[4] = binding;
            assert!(plan(&before, &after).is_err(), "accepted {binding:?}");
        }
    }

    #[test]
    fn unchanged_unknown_macro_is_archival_but_changed_unknown_macro_is_rejected() {
        let mut before = fixture();
        before.macros[7][2] = 250;
        let mut after = before.clone();
        after.picture[3] = [4, 5, 6];
        assert_eq!(plan(&before, &after).unwrap().picture_keys, 1);
        after.macros[7][3] = 1;
        assert!(
            plan(&before, &after)
                .unwrap_err()
                .contains("macro slot 7 current")
        );
        let mut before = fixture();
        let mut after = before.clone();
        before.macros[7] = macros::encode(&macros::Macro {
            repeat_count: 1,
            events: vec![],
        })
        .unwrap();
        after.macros[7][2] = 250;
        assert!(
            plan(&before, &after)
                .unwrap_err()
                .contains("macro slot 7 target")
        );
    }

    #[test]
    fn rejects_unsafe_setting_values_and_opaque_bytes() {
        let before = fixture();
        let mut after = before.clone();
        set_reply(&mut after, 0x97, 1, 2);
        assert!(plan(&before, &after).unwrap_err().contains("canonical"));
        let mut after = before.clone();
        set_reply(&mut after, 0x92, 1, 59);
        assert!(plan(&before, &after).unwrap_err().contains("whole minutes"));
        let mut after = before.clone();
        set_reply(&mut after, 0x92, 40, 1);
        assert!(
            plan(&before, &after)
                .unwrap_err()
                .contains("opaque byte 40")
        );
        let mut after = before.clone();
        set_reply(&mut after, 0x86, 2, 0);
        set_reply(&mut after, 0x86, 4, 1);
        assert!(
            plan(&before, &after)
                .unwrap_err()
                .contains("cannot be produced")
        );
        let mut before = fixture();
        set_reply(&mut before, 0x86, 4, 1);
        let mut after = before.clone();
        set_reply(&mut after, 0x86, 2, 0);
        set_reply(&mut after, 0x86, 4, 0);
        assert!(
            plan(&before, &after)
                .unwrap_err()
                .contains("cannot be restored")
        );
    }

    #[test]
    fn unchanged_unknown_setting_is_archival_but_changed_value_needs_safe_rollback() {
        let mut before = fixture();
        set_reply(&mut before, settings::AUTO_OS_READ, 1, 2);
        let mut after = before.clone();
        after.picture[4] = [1, 2, 3];
        assert_eq!(plan(&before, &after).unwrap().picture_keys, 1);
        set_reply(&mut after, settings::AUTO_OS_READ, 1, 0);
        assert!(plan(&before, &after).unwrap_err().contains("canonical"));

        let mut before = fixture();
        set_reply(&mut before, settings::DEBOUNCE_READ, 2, 0);
        let mut after = before.clone();
        set_reply(&mut after, settings::DEBOUNCE_READ, 2, 5);
        assert!(
            plan(&before, &after)
                .unwrap_err()
                .contains("settings reply 0x91")
        );
    }

    #[test]
    fn rejects_opaque_lighting_bytes() {
        let before = fixture();
        let mut raw = before.lighting.raw().to_vec();
        raw[40] = 1;
        let mut after = before.clone();
        after.lighting = Lighting::decode(&raw).unwrap();
        assert!(
            plan(&before, &after)
                .unwrap_err()
                .contains("opaque byte 40")
        );
    }
}
