//! Nia87 forward-write policy. Complete raw matrices remain archival data.

use super::board;

/// Validate intended edits before opening a device. Recovery deliberately does
/// not use this mask: it must be able to repair unexpected collateral changes.
pub fn validate_changes(
    old_base: &[[u8; 4]],
    old_function: &[[u8; 4]],
    new_base: &[[u8; 4]],
    new_function: &[[u8; 4]],
) -> Result<(), String> {
    if [old_base, old_function, new_base, new_function]
        .iter()
        .any(|matrix| matrix.len() != 128)
    {
        return Err("Both key matrices must contain 128 bindings".into());
    }
    let writable = board::writable_keymap_slot_mask();
    for (name, before, after, protected) in [
        ("base", old_base, new_base, &[][..]),
        (
            "function",
            old_function,
            new_function,
            &board::FN_SYSTEM_SLOTS[..],
        ),
    ] {
        for (slot, (old, new)) in before.iter().zip(after).enumerate() {
            if old != new && (!writable[slot] || protected.contains(&slot)) {
                return Err(format!(
                    "Cannot modify reserved or unmapped {name} keymap slot {slot}"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onboard_fn_positions_are_protected_without_restricting_base_or_ordinary_fn() {
        let before = vec![[0; 4]; 128];
        for slot in board::FN_SYSTEM_SLOTS {
            let mut after = before.clone();
            after[slot] = [0, 0, 4, 0];
            assert!(validate_changes(&before, &before, &before, &after).is_err());
            if slot != 59 {
                validate_changes(&before, &before, &after, &before).unwrap();
            }
        }
        let mut after = before.clone();
        after[9] = [9, 0, 1, 0];
        validate_changes(&before, &before, &before, &after).unwrap();
    }

    #[test]
    fn preserves_opaque_entries_but_rejects_edits_outside_known_keys() {
        let mut before = vec![[0; 4]; 128];
        before[6] = [7, 8, 9, 10];
        let mut after = before.clone();
        after[10] = [0, 0, 4, 0];
        after[75] = [0, 0, 5, 0];
        validate_changes(&before, &before, &after, &before).unwrap();
        for slot in [6, 59, 94, 126, 127] {
            let mut invalid = after.clone();
            invalid[slot][0] ^= 1;
            assert!(
                validate_changes(&before, &before, &invalid, &before)
                    .unwrap_err()
                    .contains(&format!("slot {slot}"))
            );
        }
        validate_changes(&before, &before, &before, &before).unwrap();
    }
}
