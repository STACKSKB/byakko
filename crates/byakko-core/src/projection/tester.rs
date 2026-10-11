// SPDX-License-Identifier: GPL-3.0-or-later
//! Representative physical layouts for local keyboard testing.
//! A printed legend never changes the USB HID keyboard-page usage.
use std::fmt;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Standard {
    #[default]
    Ansi,
    Iso,
    Jis,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Size {
    #[default]
    Full,
    Tkl,
    SeventyFive,
    SixtyFive,
    Sixty,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Legends {
    #[default]
    Qwerty,
    Dvorak,
    Colemak,
    Azerty,
    Qwertz,
    Japanese,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LayoutSelection {
    pub standard: Standard,
    pub size: Size,
    pub legends: Legends,
}

impl Standard {
    pub const ALL: [Self; 3] = [Self::Ansi, Self::Iso, Self::Jis];
}
impl Size {
    pub const ALL: [Self; 5] = [
        Self::Full,
        Self::Tkl,
        Self::SeventyFive,
        Self::SixtyFive,
        Self::Sixty,
    ];
}
impl Legends {
    pub const ALL: [Self; 6] = [
        Self::Qwerty,
        Self::Dvorak,
        Self::Colemak,
        Self::Azerty,
        Self::Qwertz,
        Self::Japanese,
    ];
}
impl fmt::Display for Standard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ansi => "ANSI",
            Self::Iso => "ISO",
            Self::Jis => "JIS",
        })
    }
}
impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Full => "Full-size",
            Self::Tkl => "TKL",
            Self::SeventyFive => "75%",
            Self::SixtyFive => "65%",
            Self::Sixty => "60%",
        })
    }
}
impl fmt::Display for Legends {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Qwerty => "QWERTY (US / UK)",
            Self::Dvorak => "Dvorak",
            Self::Colemak => "Colemak",
            Self::Azerty => "AZERTY (French)",
            Self::Qwertz => "QWERTZ (German)",
            Self::Japanese => "Japanese (JIS)",
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    /// None for an unobservable firmware Fn position.
    pub usage: Option<u16>,
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

type Spec = (Option<u16>, f32);
fn usage(id: u16, width: f32) -> Spec {
    (Some(id), width)
}
fn key(
    keys: &mut Vec<Key>,
    selection: LayoutSelection,
    id: Option<u16>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) {
    keys.push(Key {
        usage: id,
        label: id.map_or_else(|| "Fn".to_string(), |id| label(id, selection)),
        x,
        y,
        width,
        height,
    });
}
fn row(keys: &mut Vec<Key>, selection: LayoutSelection, y: f32, specs: &[Spec]) {
    let mut x = 0.0;
    for &(id, width) in specs {
        let height = if selection.standard != Standard::Ansi && id == Some(0x28) {
            2.0
        } else {
            1.0
        };
        key(keys, selection, id, x, y, width, height);
        x += width;
    }
}

pub fn layout_for(selection: LayoutSelection) -> Vec<Key> {
    let mut keys = Vec::with_capacity(110);
    let alpha_y = match selection.size {
        Size::Full | Size::Tkl => 1.5,
        Size::SeventyFive => 1.25,
        Size::SixtyFive | Size::Sixty => 0.0,
    };
    row(
        &mut keys,
        selection,
        alpha_y,
        &[
            usage(0x35, 1.0),
            usage(0x1e, 1.0),
            usage(0x1f, 1.0),
            usage(0x20, 1.0),
            usage(0x21, 1.0),
            usage(0x22, 1.0),
            usage(0x23, 1.0),
            usage(0x24, 1.0),
            usage(0x25, 1.0),
            usage(0x26, 1.0),
            usage(0x27, 1.0),
            usage(0x2d, 1.0),
            usage(0x2e, 1.0),
            usage(0x2a, 2.0),
        ],
    );
    if selection.standard == Standard::Jis {
        // The JIS yen key occupies half of the usual two-unit Backspace.
        if let Some(backspace) = keys.iter_mut().find(|key| key.usage == Some(0x2a)) {
            backspace.x = 14.0;
            backspace.width = 1.0;
        }
        key(&mut keys, selection, Some(0x89), 13.0, alpha_y, 1.0, 1.0);
    }
    let mut second = vec![
        usage(0x2b, 1.5),
        usage(0x14, 1.0),
        usage(0x1a, 1.0),
        usage(0x08, 1.0),
        usage(0x15, 1.0),
        usage(0x17, 1.0),
        usage(0x1c, 1.0),
        usage(0x18, 1.0),
        usage(0x0c, 1.0),
        usage(0x12, 1.0),
        usage(0x13, 1.0),
        usage(0x2f, 1.0),
        usage(0x30, 1.0),
    ];
    second.push(if selection.standard != Standard::Ansi {
        usage(0x28, 1.5)
    } else {
        usage(0x31, 1.5)
    });
    row(&mut keys, selection, alpha_y + 1.0, &second);
    let mut third = vec![
        usage(0x39, 1.75),
        usage(0x04, 1.0),
        usage(0x16, 1.0),
        usage(0x07, 1.0),
        usage(0x09, 1.0),
        usage(0x0a, 1.0),
        usage(0x0b, 1.0),
        usage(0x0d, 1.0),
        usage(0x0e, 1.0),
        usage(0x0f, 1.0),
        usage(0x33, 1.0),
        usage(0x34, 1.0),
    ];
    third.push(if selection.standard != Standard::Ansi {
        usage(0x31, 0.75)
    } else {
        usage(0x28, 2.25)
    });
    row(&mut keys, selection, alpha_y + 2.0, &third);
    let mut fourth = vec![usage(
        0xe1,
        if selection.standard == Standard::Iso {
            1.25
        } else {
            2.25
        },
    )];
    if selection.standard == Standard::Iso {
        fourth.push(usage(0x64, 1.0));
    }
    fourth.extend([
        usage(0x1d, 1.0),
        usage(0x1b, 1.0),
        usage(0x06, 1.0),
        usage(0x19, 1.0),
        usage(0x05, 1.0),
        usage(0x11, 1.0),
        usage(0x10, 1.0),
        usage(0x36, 1.0),
        usage(0x37, 1.0),
        usage(0x38, 1.0),
        usage(
            0xe5,
            if matches!(selection.size, Size::SeventyFive | Size::SixtyFive) {
                1.75
            } else {
                2.75
            },
        ),
    ]);
    if selection.standard == Standard::Jis {
        let shift = fourth.last_mut().expect("fourth row has right Shift");
        shift.1 = if matches!(selection.size, Size::SeventyFive | Size::SixtyFive) {
            0.75
        } else {
            1.75
        };
        fourth.insert(fourth.len() - 1, usage(0x87, 1.0));
    }
    row(&mut keys, selection, alpha_y + 3.0, &fourth);
    let bottom = if selection.standard == Standard::Jis {
        let mut row = vec![
            usage(0xe0, 1.25),
            usage(0xe3, 1.25),
            usage(0xe2, 1.25),
            usage(0x8b, 1.25),
            usage(0x2c, 2.5),
            usage(0x8a, 1.25),
            usage(0x88, 1.25),
        ];
        if matches!(selection.size, Size::SeventyFive | Size::SixtyFive) {
            row.extend([usage(0xe6, 1.0), (None, 1.0), usage(0xe4, 1.0)]);
        } else {
            row.extend([
                usage(0xe6, 1.25),
                if selection.size == Size::Sixty {
                    (None, 1.25)
                } else {
                    usage(0xe7, 1.25)
                },
                usage(0x65, 1.25),
                usage(0xe4, 1.25),
            ]);
        }
        row
    } else if matches!(selection.size, Size::SeventyFive | Size::SixtyFive) {
        vec![
            usage(0xe0, 1.25),
            usage(0xe3, 1.25),
            usage(0xe2, 1.25),
            usage(0x2c, 6.25),
            usage(0xe6, 1.0),
            (None, 1.0),
            usage(0xe4, 1.0),
        ]
    } else if selection.size == Size::Sixty {
        vec![
            usage(0xe0, 1.25),
            usage(0xe3, 1.25),
            usage(0xe2, 1.25),
            usage(0x2c, 6.25),
            usage(0xe6, 1.25),
            (None, 1.25),
            usage(0x65, 1.25),
            usage(0xe4, 1.25),
        ]
    } else {
        vec![
            usage(0xe0, 1.25),
            usage(0xe3, 1.25),
            usage(0xe2, 1.25),
            usage(0x2c, 6.25),
            usage(0xe6, 1.25),
            usage(0xe7, 1.25),
            usage(0x65, 1.25),
            usage(0xe4, 1.25),
        ]
    };
    row(&mut keys, selection, alpha_y + 4.0, &bottom);
    if matches!(selection.size, Size::Sixty | Size::SixtyFive) {
        if let Some(grave) = keys.iter_mut().find(|key| key.usage == Some(0x35)) {
            grave.usage = Some(0x29);
            grave.label = "Esc".into();
        }
    } else {
        key(&mut keys, selection, Some(0x29), 0.0, 0.0, 1.0, 1.0);
        for number in 0..12 {
            let x = if matches!(selection.size, Size::Full | Size::Tkl) {
                2.0 + number as f32 + (number / 4) as f32 * 0.5
            } else {
                1.25 + number as f32
            };
            key(&mut keys, selection, Some(0x3a + number), x, 0.0, 1.0, 1.0);
        }
    }
    if matches!(selection.size, Size::Full | Size::Tkl) {
        for (index, id) in [0x46, 0x47, 0x48].into_iter().enumerate() {
            key(
                &mut keys,
                selection,
                Some(id),
                15.5 + index as f32,
                0.0,
                1.0,
                1.0,
            );
        }
        for (index, id) in [0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e].into_iter().enumerate() {
            key(
                &mut keys,
                selection,
                Some(id),
                15.5 + (index % 3) as f32,
                alpha_y + (index / 3) as f32,
                1.0,
                1.0,
            );
        }
        key(
            &mut keys,
            selection,
            Some(0x52),
            16.5,
            alpha_y + 3.0,
            1.0,
            1.0,
        );
        for (index, id) in [0x50, 0x51, 0x4f].into_iter().enumerate() {
            key(
                &mut keys,
                selection,
                Some(id),
                15.5 + index as f32,
                alpha_y + 4.0,
                1.0,
                1.0,
            );
        }
    } else if matches!(selection.size, Size::SeventyFive | Size::SixtyFive) {
        for (index, id) in [0x4c, 0x4b, 0x4e, 0x4d].into_iter().enumerate() {
            key(
                &mut keys,
                selection,
                Some(id),
                15.25,
                alpha_y + index as f32,
                1.0,
                1.0,
            );
        }
        if selection.size == Size::SeventyFive {
            key(&mut keys, selection, Some(0x4a), 14.0, 0.0, 1.0, 1.0);
        }
        key(
            &mut keys,
            selection,
            Some(0x52),
            14.0,
            alpha_y + 3.0,
            1.0,
            1.0,
        );
        for (index, id) in [0x50, 0x51, 0x4f].into_iter().enumerate() {
            key(
                &mut keys,
                selection,
                Some(id),
                13.25 + index as f32,
                alpha_y + 4.0,
                1.0,
                1.0,
            );
        }
    }
    if selection.size == Size::Full {
        for (index, id) in [0x53, 0x54, 0x55, 0x56].into_iter().enumerate() {
            key(
                &mut keys,
                selection,
                Some(id),
                19.0 + index as f32,
                alpha_y,
                1.0,
                1.0,
            );
        }
        for (r, ids) in [[0x5f, 0x60, 0x61], [0x5c, 0x5d, 0x5e], [0x59, 0x5a, 0x5b]]
            .into_iter()
            .enumerate()
        {
            for (c, id) in ids.into_iter().enumerate() {
                key(
                    &mut keys,
                    selection,
                    Some(id),
                    19.0 + c as f32,
                    alpha_y + r as f32 + 1.0,
                    1.0,
                    1.0,
                );
            }
        }
        key(
            &mut keys,
            selection,
            Some(0x57),
            22.0,
            alpha_y + 1.0,
            1.0,
            2.0,
        );
        key(
            &mut keys,
            selection,
            Some(0x58),
            22.0,
            alpha_y + 3.0,
            1.0,
            2.0,
        );
        key(
            &mut keys,
            selection,
            Some(0x62),
            19.0,
            alpha_y + 4.0,
            2.0,
            1.0,
        );
        key(
            &mut keys,
            selection,
            Some(0x63),
            21.0,
            alpha_y + 4.0,
            1.0,
            1.0,
        );
    }
    keys
}

pub fn layout_bounds(keys: &[Key]) -> (f32, f32) {
    keys.iter()
        .fold((0.0_f32, 0.0_f32), |(width, height), key| {
            (width.max(key.x + key.width), height.max(key.y + key.height))
        })
}

fn label(id: u16, choice: LayoutSelection) -> String {
    // The alphabet uses fixed HID positions (A=0x04 through Z=0x1d).
    if (0x04..=0x1d).contains(&id) {
        let index = (id - 0x04) as usize;
        const QWERTY: [&str; 26] = [
            "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q",
            "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
        ];
        const DVORAK: [&str; 26] = [
            "A", "X", "J", "E", ".", "U", "I", "D", "C", "H", "T", "N", "M", "B", "R", "L", "'",
            "P", "O", "Y", "G", "K", ",", "Q", "F", ";",
        ];
        const COLEMAK: [&str; 26] = [
            "A", "B", "C", "S", "F", "T", "D", "H", "U", "N", "E", "I", "M", "K", "Y", ";", "Q",
            "P", "R", "G", "L", "V", "W", "X", "J", "Z",
        ];
        let printed = match choice.legends {
            Legends::Qwerty => QWERTY[index],
            Legends::Dvorak => DVORAK[index],
            Legends::Colemak => COLEMAK[index],
            Legends::Azerty => match id {
                0x04 => "Q",
                0x14 => "A",
                0x1a => "Z",
                0x1d => "W",
                0x10 => ",",
                _ => QWERTY[index],
            },
            Legends::Japanese => QWERTY[index],
            Legends::Qwertz => match id {
                0x1c => "Z",
                0x1d => "Y",
                _ => QWERTY[index],
            },
        };
        return printed.to_string();
    }
    if (0x1e..=0x27).contains(&id) {
        const DEFAULT: [&str; 10] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
        const AZERTY: [&str; 10] = ["&", "é", "\"", "'", "(", "−", "è", "_", "ç", "à"];
        return if choice.legends == Legends::Azerty {
            AZERTY
        } else {
            DEFAULT
        }[(id - 0x1e) as usize]
            .into();
    }
    if (0x3a..=0x45).contains(&id) {
        return format!("F{}", id - 0x3a + 1);
    }
    if (0x59..=0x61).contains(&id) {
        return format!("{}", (id - 0x59 + 1) % 10);
    }
    let common = match id {
        0x28 => "Enter",
        0x29 => "Esc",
        0x2a => {
            if choice.standard == Standard::Jis {
                "Bksp"
            } else {
                "Backspace"
            }
        }
        0x2b => "Tab",
        0x2c => "Space",
        0x2d => "−",
        0x2e => "=",
        0x2f => "[",
        0x30 => "]",
        0x31 => "\\",
        0x33 => ";",
        0x34 => "'",
        0x35 => "`",
        0x36 => ",",
        0x37 => ".",
        0x38 => "/",
        0x39 => "Caps",
        0x46 => "PrtSc",
        0x47 => "ScrLk",
        0x48 => "Pause",
        0x49 => "Ins",
        0x4a => "Home",
        0x4b => "PgUp",
        0x4c => "Del",
        0x4d => "End",
        0x4e => "PgDn",
        0x4f => "→",
        0x50 => "←",
        0x51 => "↓",
        0x52 => "↑",
        0x53 => "Num",
        0x54 => "/",
        0x55 => "×",
        0x56 => "−",
        0x57 => "+",
        0x58 => "Enter",
        0x62 => "0",
        0x63 => ".",
        0x64 => "\\",
        0x65 => "Menu",
        0x87 => "\\ ろ",
        0x88 => "かな",
        0x89 => "¥",
        0x8a => "変換",
        0x8b => "無変換",
        0xe0 | 0xe4 => "Ctrl",
        0xe1 | 0xe5 => "Shift",
        0xe2 | 0xe6 => "Alt",
        0xe3 | 0xe7 => "Win",
        _ => "?",
    };
    let override_label = match (choice.legends, id) {
        (Legends::Japanese, 0x35) => Some("半角"),
        (Legends::Japanese, 0x2e) => Some("^"),
        (Legends::Japanese, 0x2f) => Some("@"),
        (Legends::Japanese, 0x30) => Some("["),
        (Legends::Japanese, 0x34) => Some(":"),
        (Legends::Japanese, 0x31) => Some("]"),
        (Legends::Qwertz, 0x2d) => Some("ß"),
        (Legends::Qwertz, 0x2e) => Some("´"),
        (Legends::Qwertz, 0x2f) => Some("Ü"),
        (Legends::Qwertz, 0x30) => Some("+"),
        (Legends::Qwertz, 0x33) => Some("Ö"),
        (Legends::Qwertz, 0x34) => Some("Ä"),
        (Legends::Qwertz, 0x35) => Some("^"),
        (Legends::Qwertz, 0x31) => Some("#"),
        (Legends::Qwertz, 0x38) => Some("−"),
        (Legends::Qwertz, 0xe6) => Some("AltGr"),
        (Legends::Qwertz, 0x64) => Some("<"),
        (Legends::Azerty, 0x2d) => Some(")"),
        (Legends::Azerty, 0x2f) => Some("^"),
        (Legends::Azerty, 0x30) => Some("$"),
        (Legends::Azerty, 0x33) => Some("M"),
        (Legends::Azerty, 0x34) => Some("ù"),
        (Legends::Azerty, 0x35) => Some("²"),
        (Legends::Azerty, 0x31) => Some("*"),
        (Legends::Azerty, 0x36) => Some(";"),
        (Legends::Azerty, 0x37) => Some(":"),
        (Legends::Azerty, 0x38) => Some("!"),
        (Legends::Azerty, 0xe6) => Some("AltGr"),
        (Legends::Azerty, 0x64) => Some("<"),
        (_, 0xe6) if choice.standard == Standard::Iso => Some("AltGr"),
        (_, 0x31) if choice.standard == Standard::Iso && choice.legends == Legends::Qwerty => {
            Some("#")
        }
        _ => None,
    };
    override_label.unwrap_or(common).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layouts_keep_physical_usages_when_legends_change() {
        let mut choice = LayoutSelection::default();
        let us = layout_for(choice);
        choice.legends = Legends::Azerty;
        let french = layout_for(choice);
        assert_eq!(
            us.iter().map(|key| key.usage).collect::<Vec<_>>(),
            french.iter().map(|key| key.usage).collect::<Vec<_>>()
        );
        assert_eq!(
            us.iter().find(|key| key.usage == Some(0x14)).unwrap().label,
            "Q"
        );
        assert_eq!(
            french
                .iter()
                .find(|key| key.usage == Some(0x14))
                .unwrap()
                .label,
            "A"
        );
    }
    #[test]
    fn azerty_and_qwertz_labels_match_browser_catalog() {
        let azerty = LayoutSelection {
            legends: Legends::Azerty,
            ..Default::default()
        };
        for (usage, expected) in [
            (0x14, "A"),
            (0x1a, "Z"),
            (0x04, "Q"),
            (0x1d, "W"),
            (0x10, ","),
            (0x33, "M"),
            (0x36, ";"),
            (0x37, ":"),
            (0x38, "!"),
            (0x35, "²"),
            (0x2d, ")"),
            (0x2e, "="),
            (0x2f, "^"),
            (0x30, "$"),
            (0x31, "*"),
            (0x34, "ù"),
            (0x64, "<"),
            (0xe6, "AltGr"),
        ] {
            assert_eq!(label(usage, azerty), expected, "AZERTY HID {usage:02X}");
        }
        let qwertz = LayoutSelection {
            legends: Legends::Qwertz,
            ..Default::default()
        };
        for (usage, expected) in [
            (0x1c, "Z"),
            (0x1d, "Y"),
            (0x2d, "ß"),
            (0x2e, "´"),
            (0x2f, "Ü"),
            (0x30, "+"),
            (0x31, "#"),
            (0x33, "Ö"),
            (0x34, "Ä"),
            (0x35, "^"),
            (0x38, "−"),
            (0x64, "<"),
            (0xe6, "AltGr"),
        ] {
            assert_eq!(label(usage, qwertz), expected, "QWERTZ HID {usage:02X}");
        }
        for (usage, expected) in [
            (0x1e, "&"),
            (0x1f, "é"),
            (0x20, "\""),
            (0x21, "'"),
            (0x22, "("),
            (0x23, "−"),
            (0x24, "è"),
            (0x25, "_"),
            (0x26, "ç"),
            (0x27, "à"),
        ] {
            assert_eq!(
                label(usage, azerty),
                expected,
                "AZERTY digit HID {usage:02X}"
            );
        }
    }
    #[test]
    fn jis_usages_legends_and_geometry_cover_each_size() {
        for size in Size::ALL {
            for legends in Legends::ALL {
                let selection = LayoutSelection {
                    standard: Standard::Jis,
                    size,
                    legends,
                };
                let keys = layout_for(selection);
                let (width, height) = layout_bounds(&keys);
                let mut usages = std::collections::BTreeSet::new();
                for key in &keys {
                    assert!(key.x >= 0.0 && key.y >= 0.0);
                    assert!(key.width > 0.0 && key.height > 0.0);
                    assert!(key.x + key.width <= width && key.y + key.height <= height);
                    if let Some(usage) = key.usage {
                        assert!(
                            usages.insert(usage),
                            "duplicate JIS HID {usage:02X} at {size}"
                        );
                    }
                }
                for usage in [0x87, 0x88, 0x89, 0x8a, 0x8b] {
                    assert!(
                        usages.contains(&usage),
                        "missing JIS HID {usage:02X} at {size}"
                    );
                }
                for (index, left) in keys.iter().enumerate() {
                    for right in keys.iter().skip(index + 1) {
                        let overlap_x =
                            left.x < right.x + right.width && right.x < left.x + left.width;
                        let overlap_y =
                            left.y < right.y + right.height && right.y < left.y + left.height;
                        assert!(
                            !(overlap_x && overlap_y),
                            "JIS {size} overlap: {:?} and {:?}",
                            left.usage,
                            right.usage
                        );
                    }
                }
            }
        }
        let japanese = LayoutSelection {
            standard: Standard::Jis,
            legends: Legends::Japanese,
            ..Default::default()
        };
        for (usage, expected) in [
            (0x2a, "Bksp"),
            (0x35, "半角"),
            (0x2e, "^"),
            (0x2f, "@"),
            (0x30, "["),
            (0x34, ":"),
            (0x31, "]"),
            (0x87, "\\ ろ"),
            (0x88, "かな"),
            (0x89, "¥"),
            (0x8a, "変換"),
            (0x8b, "無変換"),
        ] {
            assert_eq!(label(usage, japanese), expected, "JIS HID {usage:02X}");
        }
    }
    #[test]
    fn iso_enter_has_two_row_height_without_backslash_overlap() {
        for size in Size::ALL {
            for legends in Legends::ALL {
                let keys = layout_for(LayoutSelection {
                    standard: Standard::Iso,
                    size,
                    legends,
                });
                let enter = keys.iter().find(|key| key.usage == Some(0x28)).unwrap();
                let backslash = keys.iter().find(|key| key.usage == Some(0x31)).unwrap();
                assert_eq!(enter.height, 2.0);
                assert!(backslash.x + backslash.width <= enter.x);
                assert_eq!(backslash.y, enter.y + 1.0);
            }
        }
    }
    #[test]
    fn standards_and_sizes_are_discrete_with_unobservable_fn() {
        let ansi = layout_for(LayoutSelection::default());
        let iso = layout_for(LayoutSelection {
            standard: Standard::Iso,
            ..Default::default()
        });
        assert!(ansi.iter().all(|key| key.usage != Some(0x64)));
        assert!(iso.iter().any(|key| key.usage == Some(0x64)));
        for size in Size::ALL {
            let keys = layout_for(LayoutSelection {
                size,
                ..Default::default()
            });
            assert!(keys.iter().all(|key| key.width > 0.0 && key.height > 0.0));
            assert_eq!(keys.iter().filter(|key| key.usage == Some(0x04)).count(), 1);
            assert_eq!(
                keys.iter().any(|key| key.usage == Some(0x53)),
                size == Size::Full
            );
            assert_eq!(
                keys.iter().any(|key| key.usage == Some(0x3a)),
                size != Size::Sixty && size != Size::SixtyFive
            );
            assert_eq!(
                keys.iter().any(|key| key.usage.is_none()),
                matches!(size, Size::SeventyFive | Size::SixtyFive | Size::Sixty)
            );
        }
    }
}
