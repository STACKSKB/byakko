//! Physical ANSI tenkeyless layout. Coordinates are in standard key-width units.
//! USB HID usages identify keys, not firmware configuration slots.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalKey {
    pub label: &'static str,
    pub usage: u8,
    pub x: f32,
    pub y: f32,
    pub width: f32,
}

/// The Fn key has no standard keyboard-page usage. This is a layout-only sentinel.
pub const FN_PLACEHOLDER_USAGE: u8 = 0xff;

pub fn nia87_keys() -> Vec<PhysicalKey> {
    let mut keys = Vec::with_capacity(87);
    let mut row = |y: f32, entries: &[(&'static str, u8, f32, f32)]| {
        keys.extend(entries.iter().map(|&(label, usage, x, width)| PhysicalKey {
            label,
            usage,
            x,
            y,
            width,
        }));
    };

    row(
        0.0,
        &[
            ("Esc", 0x29, 0.0, 1.0),
            ("F1", 0x3a, 2.0, 1.0),
            ("F2", 0x3b, 3.0, 1.0),
            ("F3", 0x3c, 4.0, 1.0),
            ("F4", 0x3d, 5.0, 1.0),
            ("F5", 0x3e, 6.5, 1.0),
            ("F6", 0x3f, 7.5, 1.0),
            ("F7", 0x40, 8.5, 1.0),
            ("F8", 0x41, 9.5, 1.0),
            ("F9", 0x42, 11.0, 1.0),
            ("F10", 0x43, 12.0, 1.0),
            ("F11", 0x44, 13.0, 1.0),
            ("F12", 0x45, 14.0, 1.0),
            ("PrtSc", 0x46, 15.5, 1.0),
            ("ScrLk", 0x47, 16.5, 1.0),
            ("Pause", 0x48, 17.5, 1.0),
        ],
    );
    row(
        1.5,
        &[
            ("`", 0x35, 0.0, 1.0),
            ("1", 0x1e, 1.0, 1.0),
            ("2", 0x1f, 2.0, 1.0),
            ("3", 0x20, 3.0, 1.0),
            ("4", 0x21, 4.0, 1.0),
            ("5", 0x22, 5.0, 1.0),
            ("6", 0x23, 6.0, 1.0),
            ("7", 0x24, 7.0, 1.0),
            ("8", 0x25, 8.0, 1.0),
            ("9", 0x26, 9.0, 1.0),
            ("0", 0x27, 10.0, 1.0),
            ("-", 0x2d, 11.0, 1.0),
            ("=", 0x2e, 12.0, 1.0),
            ("Backspace", 0x2a, 13.0, 2.0),
            ("Insert", 0x49, 15.5, 1.0),
            ("Home", 0x4a, 16.5, 1.0),
            ("PgUp", 0x4b, 17.5, 1.0),
        ],
    );
    row(
        2.5,
        &[
            ("Tab", 0x2b, 0.0, 1.5),
            ("Q", 0x14, 1.5, 1.0),
            ("W", 0x1a, 2.5, 1.0),
            ("E", 0x08, 3.5, 1.0),
            ("R", 0x15, 4.5, 1.0),
            ("T", 0x17, 5.5, 1.0),
            ("Y", 0x1c, 6.5, 1.0),
            ("U", 0x18, 7.5, 1.0),
            ("I", 0x0c, 8.5, 1.0),
            ("O", 0x12, 9.5, 1.0),
            ("P", 0x13, 10.5, 1.0),
            ("[", 0x2f, 11.5, 1.0),
            ("]", 0x30, 12.5, 1.0),
            ("\\", 0x31, 13.5, 1.5),
            ("Delete", 0x4c, 15.5, 1.0),
            ("End", 0x4d, 16.5, 1.0),
            ("PgDn", 0x4e, 17.5, 1.0),
        ],
    );
    row(
        3.5,
        &[
            ("Caps", 0x39, 0.0, 1.75),
            ("A", 0x04, 1.75, 1.0),
            ("S", 0x16, 2.75, 1.0),
            ("D", 0x07, 3.75, 1.0),
            ("F", 0x09, 4.75, 1.0),
            ("G", 0x0a, 5.75, 1.0),
            ("H", 0x0b, 6.75, 1.0),
            ("J", 0x0d, 7.75, 1.0),
            ("K", 0x0e, 8.75, 1.0),
            ("L", 0x0f, 9.75, 1.0),
            (";", 0x33, 10.75, 1.0),
            ("'", 0x34, 11.75, 1.0),
            ("Enter", 0x28, 12.75, 2.25),
        ],
    );
    row(
        4.5,
        &[
            ("LShift", 0xe1, 0.0, 2.25),
            ("Z", 0x1d, 2.25, 1.0),
            ("X", 0x1b, 3.25, 1.0),
            ("C", 0x06, 4.25, 1.0),
            ("V", 0x19, 5.25, 1.0),
            ("B", 0x05, 6.25, 1.0),
            ("N", 0x11, 7.25, 1.0),
            ("M", 0x10, 8.25, 1.0),
            (",", 0x36, 9.25, 1.0),
            (".", 0x37, 10.25, 1.0),
            ("/", 0x38, 11.25, 1.0),
            ("RShift", 0xe5, 12.25, 2.75),
            ("Up", 0x52, 16.5, 1.0),
        ],
    );
    row(
        5.5,
        &[
            ("LCtrl", 0xe0, 0.0, 1.25),
            ("LWin", 0xe3, 1.25, 1.25),
            ("LAlt", 0xe2, 2.5, 1.25),
            ("Space", 0x2c, 3.75, 6.25),
            ("RAlt", 0xe6, 10.0, 1.25),
            ("Fn", FN_PLACEHOLDER_USAGE, 11.25, 1.25),
            ("Menu", 0x65, 12.5, 1.25),
            ("RCtrl", 0xe4, 13.75, 1.25),
            ("Left", 0x50, 15.5, 1.0),
            ("Down", 0x51, 16.5, 1.0),
            ("Right", 0x4f, 17.5, 1.0),
        ],
    );
    keys
}

/// Human-readable labels for USB HID keyboard-page usages.
pub fn usage_label(usage: u8) -> String {
    match usage {
        0x04..=0x1d => ((b'A' + usage - 0x04) as char).to_string(),
        0x1e..=0x26 => (usage - 0x1d).to_string(),
        0x27 => "0".into(),
        0x28 => "Enter".into(),
        0x29 => "Esc".into(),
        0x2a => "Backspace".into(),
        0x2b => "Tab".into(),
        0x2c => "Space".into(),
        0x2d => "-".into(),
        0x2e => "=".into(),
        0x2f => "[".into(),
        0x30 => "]".into(),
        0x31 => "\\".into(),
        0x32 => "Non-US #".into(),
        0x33 => ";".into(),
        0x34 => "'".into(),
        0x35 => "`".into(),
        0x36 => ",".into(),
        0x37 => ".".into(),
        0x38 => "/".into(),
        0x39 => "Caps".into(),
        0x3a..=0x45 => format!("F{}", usage - 0x39),
        0x46 => "PrtSc".into(),
        0x47 => "ScrLk".into(),
        0x48 => "Pause".into(),
        0x49 => "Insert".into(),
        0x4a => "Home".into(),
        0x4b => "PgUp".into(),
        0x4c => "Delete".into(),
        0x4d => "End".into(),
        0x4e => "PgDn".into(),
        0x4f => "Right".into(),
        0x50 => "Left".into(),
        0x51 => "Down".into(),
        0x52 => "Up".into(),
        0x53 => "NumLock".into(),
        0x64 => "Non-US \\".into(),
        0x65 => "Menu".into(),
        0x68..=0x73 => format!("F{}", usage - 0x68 + 13),
        0xe0 => "LCtrl".into(),
        0xe1 => "LShift".into(),
        0xe2 => "LAlt".into(),
        0xe3 => "LWin".into(),
        0xe4 => "RCtrl".into(),
        0xe5 => "RShift".into(),
        0xe6 => "RAlt".into(),
        0xe7 => "RWin".into(),
        FN_PLACEHOLDER_USAGE => "Fn".into(),
        _ => format!("0x{usage:02X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn contains_87_physical_keys() {
        assert_eq!(nia87_keys().len(), 87);
    }

    #[test]
    fn known_usages_are_unique_and_labeled() {
        let keys = nia87_keys();
        let mut usages = HashSet::new();
        for key in &keys {
            if key.usage != FN_PLACEHOLDER_USAGE {
                assert!(usages.insert(key.usage), "duplicate usage: {}", key.label);
                assert_eq!(usage_label(key.usage), key.label);
            }
        }
        assert_eq!(usages.len(), 86);
        assert_eq!(
            keys.iter()
                .filter(|key| key.usage == FN_PLACEHOLDER_USAGE)
                .count(),
            1
        );
    }

    #[test]
    fn keys_do_not_overlap_within_a_row() {
        let keys = nia87_keys();
        for a in &keys {
            assert!(a.width > 0.0);
            for b in &keys {
                if std::ptr::eq(a, b) || a.y != b.y {
                    continue;
                }
                assert!(
                    a.x + a.width <= b.x || b.x + b.width <= a.x,
                    "{} overlaps {} at y={}",
                    a.label,
                    b.label,
                    a.y
                );
            }
        }
    }
}
