//! Static per-key RGB maps for the physical Nia87. Positions are independent of
//! the selected keymap layer, bindings, firmware lighting effect and UI language.
use super::{adapter, layout::PhysicalKey, picture_adapter::picture_positions};
use byakko_core::editor::picture::Colors;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Look {
    Wasd,
    Esdf,
    SynthDusk,
    SignalBlocks,
    PixelVisitor,
    NightDesk,
    GeometricHeart,
    Rainbow,
}

pub const ALL: [Look; 8] = [
    Look::Wasd,
    Look::Esdf,
    Look::SynthDusk,
    Look::SignalBlocks,
    Look::PixelVisitor,
    Look::NightDesk,
    Look::GeometricHeart,
    Look::Rainbow,
];

impl Look {
    pub fn id(self) -> &'static str {
        match self {
            Self::Wasd => "wasd",
            Self::Esdf => "esdf",
            Self::SynthDusk => "dusk",
            Self::SignalBlocks => "blocks",
            Self::PixelVisitor => "invader",
            Self::NightDesk => "night",
            Self::GeometricHeart => "heart",
            Self::Rainbow => "rainbow",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Wasd => "WASD",
            Self::Esdf => "ESDF",
            Self::SynthDusk => "Synth Dusk",
            Self::SignalBlocks => "Signal Blocks",
            Self::PixelVisitor => "Pixel Visitor",
            Self::NightDesk => "Night Desk",
            Self::GeometricHeart => "Geometric Heart",
            Self::Rainbow => "Rainbow",
        }
    }

    pub fn colors(self, brush: [u8; 3]) -> Colors {
        picture_positions()
            .map(|(slot, key)| (adapter::key_id(slot), self.sample(key, brush)))
            .collect()
    }

    fn sample(self, key: PhysicalKey, brush: [u8; 3]) -> [u8; 3] {
        // The ANSI TKL spans 18.5u by 6.5u. Wide keys sample at their own centre.
        let x = f64::from(key.x + key.width / 2.0) / 18.5;
        let y = f64::from(key.y + 0.5) / 6.5;
        let position = key.usage;
        let symbol = match position {
            0x04..=0x1d => b'A' + (position - 0x04),
            0x1e..=0x26 => b'1' + (position - 0x1e),
            0x27 => b'0',
            0x33 => b';',
            0x36 => b',',
            0x37 => b'.',
            _ => 0,
        };
        let arrow = matches!(position, 0x4f..=0x52);
        let nav = matches!(position, 0x46..=0x4e);
        let modifier_or_editing_key =
            matches!(position, 0x28 | 0x2a..=0x2c | 0x39 | 0x65 | 0xe0..=0xe7 | 0xff);
        let rgb = match self {
            Self::Wasd if b"WASD".contains(&symbol) => 0xff3c00,
            Self::Wasd if b"RE".contains(&symbol) || matches!(position, 0xe1 | 0xe0 | 0x2c) => {
                0x47bfc4
            }
            Self::Wasd if b"12345".contains(&symbol) => 0xe3ad4e,
            Self::Esdf if b"ESDF".contains(&symbol) => 0x62d7a7,
            Self::Esdf if b"GVC".contains(&symbol) => 0xe3a45a,
            Self::Esdf if b"TJZ".contains(&symbol) || matches!(position, 0x2b | 0x2c) => 0x718ed7,
            Self::SynthDusk => return dusk(x, y),
            Self::SignalBlocks if arrow => 0xffc06c,
            Self::SignalBlocks if nav => 0xc46c57,
            Self::SignalBlocks if key.y == 0.0 => 0x7972c4,
            Self::SignalBlocks if modifier_or_editing_key => 0x264a54,
            Self::SignalBlocks => 0x56a49a,
            Self::PixelVisitor if matches!(position, 0x3c | 0x41) => 0xffc66c, // F3 and F8
            Self::PixelVisitor if b"4567890ERYUOPSDFGHJKL;XC,.".contains(&symbol) => 0x59d9ac,
            Self::NightDesk if b"FJ".contains(&symbol) => 0xa77941,
            Self::NightDesk if arrow || matches!(position, 0x28 | 0x2a) => 0x78552e,
            Self::NightDesk if key.y == 0.0 || nav => 0x120d09,
            Self::NightDesk => 0x402a19,
            Self::GeometricHeart
                if matches!(position, 0x3b | 0x3c | 0x3f | 0x40)
                    || b"23457890WERTYUIOFGHJB".contains(&symbol) =>
            {
                return brush;
            }
            Self::Rainbow => [
                0xff2030, 0xff8000, 0xffdc00, 0x20dc50, 0x00b4ff, 0x4040ff, 0xb420ff,
            ][(x * 7.0) as usize],
            Self::Wasd | Self::Esdf | Self::PixelVisitor | Self::GeometricHeart => 0,
        };
        let [_, r, g, b] = u32::to_be_bytes(rgb);
        [r, g, b]
    }
}

fn dusk(x: f64, y: f64) -> [u8; 3] {
    const STOPS: [[u8; 3]; 5] = [
        [39, 110, 126],
        [46, 57, 104],
        [96, 62, 121],
        [175, 83, 127],
        [210, 127, 137],
    ];
    let t = (0.73 * x + 0.27 * (1.0 - y)) * 4.0;
    let start = t.floor() as usize;
    let blend = t.fract();
    std::array::from_fn(|i| {
        let a = f64::from(STOPS[start][i]);
        let b = f64::from(STOPS[start + 1][i]);
        (a + (b - a) * blend).round() as u8
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nia87::{board, picture_adapter};
    use std::collections::BTreeSet;

    #[test]
    fn each_look_covers_exactly_the_advertised_physical_picture() {
        let advertised: BTreeSet<_> = picture_adapter::capabilities().keys.into_iter().collect();
        assert_eq!(advertised.len(), 87);
        for look in ALL {
            let colors = look.colors([1, 2, 3]);
            assert_eq!(colors.keys().cloned().collect::<BTreeSet<_>>(), advertised);
        }
    }

    #[test]
    fn movement_groups_are_distinct_and_every_unassigned_key_is_off() {
        for (look, movement, shade, lit) in [
            (Look::Wasd, b"WASD", [255, 60, 0], 14),
            (Look::Esdf, b"ESDF", [98, 215, 167], 12),
        ] {
            let colors = look.colors([0; 3]);
            for letter in movement {
                let slot = board::slot_for_usage(letter - b'A' + 4).unwrap();
                assert_eq!(colors[&adapter::key_id(slot)], shade);
            }
            assert_eq!(colors.values().filter(|color| **color == shade).count(), 4);
            assert_eq!(
                colors.values().filter(|color| **color != [0; 3]).count(),
                lit
            );
        }
    }

    #[test]
    fn heart_uses_the_chosen_brush_and_leaves_the_spacebar_dark() {
        let colors = Look::GeometricHeart.colors([12, 34, 56]);
        for (usage, expected) in [(5, [12, 34, 56]), (0x2c, [0, 0, 0])] {
            assert_eq!(
                colors[&adapter::key_id(board::slot_for_usage(usage).unwrap())],
                expected
            );
        }
    }
}
