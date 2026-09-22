//! Pure selection of matrix slots that need restoration after a keymap write.

pub(crate) fn slots_to_restore(
    observed: Option<&[[u8; 4]]>,
    attempted: &[[u8; 4]],
    original: &[[u8; 4]],
) -> Result<Vec<usize>, String> {
    if attempted.len() != 128
        || original.len() != 128
        || observed.is_some_and(|matrix| matrix.len() != 128)
    {
        return Err("Keymap matrix must contain 128 slots".into());
    }
    if attempted[126..] != original[126..]
        || observed.is_some_and(|matrix| matrix[126..] != original[126..])
    {
        return Err("Reserved keymap slots differ and cannot be restored".into());
    }
    let source = observed.unwrap_or(attempted);
    Ok((0..126)
        .filter(|&slot| source[slot] != original[slot])
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn matrix() -> Vec<[u8; 4]> {
        vec![[0; 4]; 128]
    }

    #[test]
    fn unreadable_unchanged_selects_none() {
        let original = matrix();
        assert!(
            slots_to_restore(None, &original, &original)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn unreadable_one_change_selects_one_slot() {
        let original = matrix();
        let mut attempted = original.clone();
        attempted[9] = [0, 0, 4, 0];
        assert_eq!(
            slots_to_restore(None, &attempted, &original).unwrap(),
            vec![9]
        );
    }

    #[test]
    fn known_observation_selects_actual_differences_only() {
        let original = matrix();
        let mut attempted = original.clone();
        attempted[9] = [0, 0, 4, 0];
        let mut observed = original.clone();
        observed[42] = [0, 0, 5, 0];
        assert_eq!(
            slots_to_restore(Some(&observed), &attempted, &original).unwrap(),
            vec![42]
        );
    }

    #[test]
    fn invalid_shape_and_reserved_difference_rejected() {
        let original = matrix();
        assert!(slots_to_restore(None, &original[..127], &original).is_err());
        assert!(slots_to_restore(Some(&original[..127]), &original, &original).is_err());
        let mut attempted = original.clone();
        attempted[126] = [1; 4];
        assert!(slots_to_restore(None, &attempted, &original).is_err());
        let mut observed = original.clone();
        observed[127] = [1; 4];
        assert!(slots_to_restore(Some(&observed), &original, &original).is_err());
    }
}
