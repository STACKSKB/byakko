//! Nia87 physical slots derived from our observed, vendor-default-checked base map.
//! This table is invariant under subsequent key remapping.
pub fn slot_for_usage(usage: u8) -> Option<usize> {
    match usage {
        0x29 => Some(0),
        0x35 => Some(1),
        0x2b => Some(2),
        0x39 => Some(3),
        0xe1 => Some(4),
        0xe0 => Some(5),
        0x1e => Some(7),
        0x14 => Some(8),
        0x04 => Some(9),
        0x64 => Some(10),
        0x3a => Some(12),
        0x1f => Some(13),
        0x1a => Some(14),
        0x16 => Some(15),
        0x1d => Some(16),
        0xe3 => Some(17),
        0x3b => Some(18),
        0x20 => Some(19),
        0x08 => Some(20),
        0x07 => Some(21),
        0x1b => Some(22),
        0xe2 => Some(23),
        0x3c => Some(24),
        0x21 => Some(25),
        0x15 => Some(26),
        0x09 => Some(27),
        0x06 => Some(28),
        0x3d => Some(30),
        0x22 => Some(31),
        0x17 => Some(32),
        0x0a => Some(33),
        0x19 => Some(34),
        0x3e => Some(36),
        0x23 => Some(37),
        0x1c => Some(38),
        0x0b => Some(39),
        0x05 => Some(40),
        0x2c => Some(41),
        0x3f => Some(42),
        0x24 => Some(43),
        0x18 => Some(44),
        0x0d => Some(45),
        0x11 => Some(46),
        0x40 => Some(48),
        0x25 => Some(49),
        0x0c => Some(50),
        0x0e => Some(51),
        0x10 => Some(52),
        0xe6 => Some(53),
        0x41 => Some(54),
        0x26 => Some(55),
        0x12 => Some(56),
        0x0f => Some(57),
        0x36 => Some(58),
        0x42 => Some(60),
        0x27 => Some(61),
        0x13 => Some(62),
        0x33 => Some(63),
        0x37 => Some(64),
        0x65 => Some(65),
        0x43 => Some(66),
        0x2d => Some(67),
        0x2f => Some(68),
        0x34 => Some(69),
        0x38 => Some(70),
        0xe4 => Some(71),
        0x44 => Some(72),
        0x2e => Some(73),
        0x30 => Some(74),
        0x32 => Some(75),
        0xe5 => Some(76),
        0x50 => Some(77),
        0x45 => Some(78),
        0x2a => Some(79),
        0x31 => Some(80),
        0x28 => Some(81),
        0x52 => Some(82),
        0x51 => Some(83),
        0x46 => Some(84),
        0x49 => Some(85),
        0x4c => Some(86),
        0x4a => Some(87),
        0x4d => Some(88),
        0x4f => Some(89),
        0x47 => Some(90),
        0x48 => Some(91),
        0x4b => Some(92),
        0x4e => Some(93),
        0xff => Some(59), // Physical Fn, not a HID usage.
        _ => None,
    }
}

/// Writable picture slots for the observed physical Nia87 layout. Other
/// matrix entries are retained in snapshots but are not editable colors.
pub fn physical_slot_mask() -> [bool; 128] {
    let mut mask = [false; 128];
    for key in super::layout::nia87_keys() {
        if let Some(slot) = slot_for_usage(key.usage) {
            mask[slot] = true;
        }
    }
    mask
}

/// Ordinary keymap slots in the observed default matrix. The ANSI drawing
/// omits two ISO positions, while its physical Fn key has a special binding
/// that is preserved but not offered as an ordinary remap target.
pub fn writable_keymap_slot_mask() -> [bool; 128] {
    let mut mask = physical_slot_mask();
    mask[59] = false; // Fn
    mask[10] = true; // Non-US backslash
    mask[75] = true; // Non-US hash
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keymap_mask_preserves_iso_positions_but_protects_fn_and_empty_slots() {
        let mask = writable_keymap_slot_mask();
        assert_eq!(mask.iter().filter(|&&writable| writable).count(), 88);
        assert!(mask[10] && mask[75] && mask[42]);
        assert!(!mask[6] && !mask[59] && !mask[94] && !mask[126]);
    }
}
