use super::*;

#[cfg(test)]
mod lighting_tests {
    #[test]
    fn reserved_slot_is_rejected_before_device_access() {
        let expected = super::Snapshot {
            format_version: 1,
            firmware: 0x100,
            profile: 0,
            base: vec![[0; 4]; 128],
            function: vec![[0; 4]; 128],
        };
        let mut base = expected.base.clone();
        let function = expected.function.clone();
        base[127] = [0, 0, 0x72, 0];
        let error = super::apply_keymaps(
            &expected,
            &base,
            &function,
            std::path::Path::new("unused-backup-path"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("reserved padding"));
    }

    #[test]
    fn unmapped_keymap_slot_is_rejected_before_device_access() {
        let expected = super::Snapshot {
            format_version: 1,
            firmware: 0x100,
            profile: 0,
            base: vec![[0; 4]; 128],
            function: vec![[0; 4]; 128],
        };
        let mut function = expected.function.clone();
        function[6] = [0, 0, 4, 0];
        let error = super::apply_keymaps(
            &expected,
            &expected.base,
            &function,
            std::path::Path::new("unused-backup-path"),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unmapped function keymap slot 6")
        );
    }

    #[test]
    fn reserved_picture_slots_are_rejected_before_device_access() {
        use byakko_core::session::Recovery;
        let expected = vec![[0; 3]; 128];
        let mut desired = expected.clone();
        desired[126] = [1, 2, 3];
        let failure = super::apply_picture_detailed(
            &expected,
            &desired,
            std::path::Path::new("unused-backup-path"),
        )
        .unwrap_err();
        assert_eq!(failure.recovery, Recovery::NotAttempted);
        assert!(failure.message.contains("reserved-slot"));
    }

    #[test]
    fn detailed_keymap_preflight_errors_have_no_recovery_attempt() {
        use byakko_core::session::Recovery;
        let expected = super::Snapshot {
            format_version: 1,
            firmware: 0x0100,
            profile: 0,
            base: vec![[0; 4]; 128],
            function: vec![[0; 4]; 128],
        };
        let short = vec![[0; 4]; 127];
        let failure = super::apply_keymaps_detailed(
            &expected,
            &short,
            &expected.function,
            std::path::Path::new("unused-backup-path"),
        )
        .unwrap_err();
        assert_eq!(failure.recovery, Recovery::NotAttempted);
        assert!(failure.message.contains("Invalid keymap shape"));
        let mut reserved = expected.base.clone();
        reserved[127] = [1; 4];
        let failure = super::apply_keymaps_detailed(
            &expected,
            &reserved,
            &expected.function,
            std::path::Path::new("unused-backup-path"),
        )
        .unwrap_err();
        assert_eq!(failure.recovery, Recovery::NotAttempted);
        assert!(failure.message.contains("reserved padding"));
    }

    #[test]
    fn keymap_rollback_error_preserves_diagnostic_and_typed_outcome() {
        use byakko_core::session::Recovery;
        let backup = std::path::Path::new("backup.json");
        let verified = super::keymap_apply_error(&"write failed", Ok(()), backup);
        assert_eq!(verified.0.recovery, Recovery::Verified);
        assert_eq!(
            verified.to_string(),
            "Apply failed: write failed. Restore result: original keymaps verified. Backup: backup.json"
        );
        let failed =
            super::keymap_apply_error(&"readback mismatch", Err("restore failed".into()), backup);
        assert_eq!(failed.0.recovery, Recovery::Failed);
        assert_eq!(
            failed.to_string(),
            "Apply failed: readback mismatch. Restore result: FAILED: restore failed. Backup: backup.json"
        );
    }

    use super::*;

    #[test]
    fn restore_report_recreates_known_fields_without_sending_opaque_tail() {
        let mut response = [0u8; 64];
        response[0] = crate::nia87::lighting::LED_READ_COMMAND;
        response[1..8].copy_from_slice(&[5, 2, 4, 7, 12, 34, 56]);
        response[9] = 0xa5;
        let original = crate::nia87::lighting::Lighting::decode(&response).unwrap();
        let report = lighting_restore_report(&original);
        assert_eq!(&report[..9], &[7, 5, 2, 4, 7, 12, 34, 56, 0x80]);
        assert!(report[9..].iter().all(|byte| *byte == 0));
        assert!(lighting_matches_report(&original, &report, &original));
        let mut changed = response;
        changed[9] = 0;
        let changed = crate::nia87::lighting::Lighting::decode(&changed).unwrap();
        assert!(!lighting_matches_report(&changed, &report, &original));
    }
}
