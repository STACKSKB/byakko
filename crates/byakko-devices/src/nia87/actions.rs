//! Nia87 action values used by the keyboard remapper.
//!
//! These values are deliberately kept as small data constructors.  The
//! configurator's action records are four bytes wide; macro contents are a
//! separate variable-length stream and are not represented here.

/// A named action that can be offered by a keyboard-oriented remapper.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionPreset {
    pub label: &'static str,
    pub bytes: [u8; 4],
}

/// USB HID modifier usages accepted by the Nia87 combo encoder.
pub const MODIFIER_LEFT_CTRL: u8 = 224;
pub const MODIFIER_LEFT_SHIFT: u8 = 225;
pub const MODIFIER_LEFT_ALT: u8 = 226;
pub const MODIFIER_LEFT_GUI: u8 = 227;

/// Nia87 macro play modes.
pub const MACRO_MODE_REPEAT_TIMES: u8 = 0;
pub const MACRO_MODE_ON_OFF: u8 = 1;
pub const MACRO_MODE_TOUCH_REPEAT: u8 = 2;

/// Encode an ordinary USB HID keyboard usage.
pub const fn key_binding(usage: u8) -> [u8; 4] {
    [0, 0, usage, 0]
}

/// Encode a single-modifier combo.
pub fn combo_binding(modifier: u8, usage: u8) -> Result<[u8; 4], String> {
    if !(MODIFIER_LEFT_CTRL..=MODIFIER_LEFT_GUI).contains(&modifier) {
        return Err(format!(
            "combo modifier must be a USB HID modifier usage 224..=227 (got {modifier})"
        ));
    }
    Ok([0, modifier, usage, 0])
}

/// Encode a shortcut with two distinct left-side modifiers.
///
/// The vendor `hC` encoder writes `[0, RC[skey], key, key2]`. Its shortcut UI
/// places the second modifier usage in `key` and the ordinary key in `key2`.
pub fn two_modifier_binding(first: u8, second: u8, usage: u8) -> Result<[u8; 4], String> {
    let valid = |value| (MODIFIER_LEFT_CTRL..=MODIFIER_LEFT_GUI).contains(&value);
    if !valid(first) || !valid(second) || first == second {
        return Err("shortcut needs two distinct Ctrl, Shift, Alt, or Win modifiers".into());
    }
    if usage == 0 || usage >= MODIFIER_LEFT_CTRL {
        return Err("shortcut target must be an ordinary keyboard usage".into());
    }
    Ok([0, first, second, usage])
}

/// Encode the four-byte action record that invokes a stored macro.
///
/// `MACROMAX` is 50 in the vendor bundle.  Its UI allocator has an inclusive
/// `<= 50` boundary and consequently exposes an extra index, but the
/// conventional 50-slot protocol range is zero through 49.  Keeping the
/// codec to that range avoids treating the observed UI off-by-one as a device
/// capability.
pub fn macro_binding(slot: u8, mode: u8) -> Result<[u8; 4], String> {
    if slot >= 50 {
        return Err(format!(
            "macro slot must be in the 50-slot range 0..=49 (got {slot})"
        ));
    }
    if mode > MACRO_MODE_TOUCH_REPEAT {
        return Err(format!(
            "macro mode must be 0 (repeat_times), 1 (on_off), or 2 (touch_repeat) (got {mode})"
        ));
    }
    Ok([9, mode, slot, 0])
}

/// Conservative keyboard-oriented choices from the vendor action tables.
/// Mouse DPI, profile switching, launch slots, and other shared mouse/app
/// actions are omitted because the Nia87 descriptor does not prove support.
pub fn presets() -> Vec<ActionPreset> {
    const PRESETS: &[ActionPreset] = &[
        // Media and system functions (the AF/RF function tables).
        ActionPreset {
            label: "Previous Track",
            bytes: [3, 0, 182, 0],
        },
        ActionPreset {
            label: "Next Track",
            bytes: [3, 0, 181, 0],
        },
        ActionPreset {
            label: "Stop",
            bytes: [3, 0, 183, 0],
        },
        ActionPreset {
            label: "Play/Pause",
            bytes: [3, 0, 205, 0],
        },
        ActionPreset {
            label: "Mute",
            bytes: [3, 0, 226, 0],
        },
        ActionPreset {
            label: "Volume Down",
            bytes: [3, 0, 234, 0],
        },
        ActionPreset {
            label: "Volume Up",
            bytes: [3, 0, 233, 0],
        },
        ActionPreset {
            label: "Fn",
            bytes: [10, 1, 0, 0],
        },
        // Visible Nia87 actions; wire facts and limits are recorded in
        // Research/action-coverage.md. Host behavior depends on the OS.
        ActionPreset {
            label: "Media Player",
            bytes: [3, 0, 0x83, 1],
        },
        ActionPreset {
            label: "Calculator",
            bytes: [3, 0, 0x92, 1],
        },
        ActionPreset {
            label: "Email",
            bytes: [3, 0, 0x8a, 1],
        },
        ActionPreset {
            label: "File Browser",
            bytes: [3, 0, 0x94, 1],
        },
        ActionPreset {
            label: "Search",
            bytes: [3, 0, 0x21, 2],
        },
        ActionPreset {
            label: "Browser Home",
            bytes: [3, 0, 0x23, 2],
        },
        ActionPreset {
            label: "Browser Back",
            bytes: [3, 0, 0x24, 2],
        },
        ActionPreset {
            label: "(",
            bytes: [0, 0, 0xe5, 0x26],
        },
        ActionPreset {
            label: ")",
            bytes: [0, 0, 0xe5, 0x27],
        },
        ActionPreset {
            label: "Win+E",
            bytes: [0, 0, 0xe3, 0x08],
        },
        ActionPreset {
            label: "Win+Tab",
            bytes: [0, 0, 0xe3, 0x2b],
        },
        ActionPreset {
            label: "Win+D",
            bytes: [0, 0, 0xe3, 0x07],
        },
        ActionPreset {
            label: "Lock Screen",
            bytes: [0, 0, 0xe3, 0x0f],
        },
        ActionPreset {
            label: "Display Brightness Up",
            bytes: [3, 0, 0x6f, 0],
        },
        ActionPreset {
            label: "Display Brightness Down",
            bytes: [3, 0, 0x70, 0],
        },
        ActionPreset {
            label: "Browser Refresh",
            bytes: [3, 0, 0x27, 2],
        },
        ActionPreset {
            label: "Zoom Out",
            bytes: [0, 0, 0xe3, 0x2d],
        },
        ActionPreset {
            label: "Zoom In",
            bytes: [0, 0, 0xe3, 0x2e],
        },
        ActionPreset {
            label: "Microphone Toggle",
            bytes: [6, 0x80, 0, 0],
        },
        // Pointer actions (the AC mouse-action table).
        ActionPreset {
            label: "Mouse Left",
            bytes: [1, 0, 240, 0],
        },
        ActionPreset {
            label: "Mouse Right",
            bytes: [1, 0, 241, 0],
        },
        ActionPreset {
            label: "Mouse Middle",
            bytes: [1, 0, 242, 0],
        },
        ActionPreset {
            label: "Mouse Back",
            bytes: [1, 0, 243, 0],
        },
        ActionPreset {
            label: "Mouse Forward",
            bytes: [1, 0, 244, 0],
        },
        ActionPreset {
            label: "Wheel Forward",
            bytes: [1, 0, 247, 0],
        },
        ActionPreset {
            label: "Wheel Back",
            bytes: [1, 0, 248, 0],
        },
        ActionPreset {
            label: "Wheel Left",
            bytes: [1, 0, 245, 0],
        },
        ActionPreset {
            label: "Wheel Right",
            bytes: [1, 0, 246, 0],
        },
    ];
    PRESETS.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_key_round_trips_as_a_four_byte_binding() {
        let binding = key_binding(0x04);
        assert_eq!(binding, [0, 0, 0x04, 0]);
        assert_eq!(binding[2], 0x04);
    }

    #[test]
    fn combo_accepts_only_the_four_single_modifiers() {
        for modifier in MODIFIER_LEFT_CTRL..=MODIFIER_LEFT_GUI {
            assert_eq!(
                combo_binding(modifier, 0x04).unwrap(),
                [0, modifier, 0x04, 0]
            );
        }
        assert!(combo_binding(0, 0x04).is_err());
        assert!(combo_binding(223, 0x04).is_err());
        assert!(combo_binding(228, 0x04).is_err());
    }

    #[test]
    fn two_modifier_shortcut_uses_vendor_key2_position() {
        assert_eq!(two_modifier_binding(224, 225, 4).unwrap(), [0, 224, 225, 4]);
        assert!(two_modifier_binding(224, 224, 4).is_err());
    }

    #[test]
    fn macro_binding_round_trips_valid_slots_and_modes() {
        assert_eq!(
            macro_binding(0, MACRO_MODE_REPEAT_TIMES).unwrap(),
            [9, 0, 0, 0]
        );
        assert_eq!(
            macro_binding(49, MACRO_MODE_TOUCH_REPEAT).unwrap(),
            [9, 2, 49, 0]
        );
        assert!(macro_binding(50, MACRO_MODE_REPEAT_TIMES).is_err());
        assert!(macro_binding(49, 3).is_err());
    }

    #[test]
    fn presets_contain_verified_keyboard_and_pointer_actions() {
        let all = presets();
        assert!(
            all.iter()
                .any(|p| p.label == "Play/Pause" && p.bytes == [3, 0, 205, 0])
        );
        assert!(
            all.iter()
                .any(|p| p.label == "Mouse Left" && p.bytes == [1, 0, 240, 0])
        );
        assert!(!all.iter().any(|p| p.label.contains("Gamepad")));
        assert!(!all.iter().any(|p| p.label.contains("Snap")));
        assert!(!all.iter().any(|p| p.label.contains("DPI")
            || p.label.contains("Profile")
            || p.label.contains("Open App")));
    }
}
