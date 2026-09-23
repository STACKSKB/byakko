//! Capture and preflight review for complete native Nia87 configuration archives.
use super::{
    configuration::{self, Configuration},
    device::Access,
};
use byakko_core::archive::{ArchiveCapabilities, NativeArchive, Review, SectionChange};
use byakko_core::session::{ApplyFailure, Recovery};
use std::path::Path;

const BACKEND_ID: &str = "nia87";

pub fn capabilities() -> ArchiveCapabilities {
    ArchiveCapabilities {
        backend_id: BACKEND_ID.into(),
        format_id: configuration::ARCHIVE_FORMAT_ID.into(),
        max_bytes: configuration::MAX_ARCHIVE_BYTES,
    }
}

fn encode(config: &Configuration) -> Result<NativeArchive, String> {
    let bytes = configuration::encode(config).map_err(|error| error.to_string())?;
    Ok(NativeArchive {
        backend_id: BACKEND_ID.into(),
        format_id: configuration::ARCHIVE_FORMAT_ID.into(),
        bytes,
    })
}

fn decode(archive: &NativeArchive) -> Result<Configuration, String> {
    if archive.backend_id != BACKEND_ID || archive.format_id != configuration::ARCHIVE_FORMAT_ID {
        return Err("Archive belongs to another backend or format".into());
    }
    if archive.bytes.len() > configuration::MAX_ARCHIVE_BYTES as usize {
        return Err("Configuration archive is too large".into());
    }
    configuration::decode(&archive.bytes).map_err(|error| error.to_string())
}

pub fn capture() -> Result<NativeArchive, String> {
    capture_with(&Access::unique())
}

pub(super) fn capture_with(access: &Access) -> Result<NativeArchive, String> {
    let config = access
        .capture_configuration(|_, _| {})
        .map_err(|error| error.to_string())?;
    encode(&config)
}

pub fn review(target: &NativeArchive) -> Result<Review, String> {
    review_with(&Access::unique(), target)
}

pub(super) fn review_with(access: &Access, target: &NativeArchive) -> Result<Review, String> {
    let target_config = decode(target)?;
    let before_config = access
        .capture_configuration(|_, _| {})
        .map_err(|error| error.to_string())?;
    review_captured(&before_config, target, &target_config)
}

pub fn apply(
    expected: &NativeArchive,
    target: &NativeArchive,
    backup_dir: &Path,
) -> Result<NativeArchive, ApplyFailure> {
    apply_with(&Access::unique(), expected, target, backup_dir)
}

pub(super) fn apply_with(
    access: &Access,
    expected: &NativeArchive,
    target: &NativeArchive,
    backup_dir: &Path,
) -> Result<NativeArchive, ApplyFailure> {
    let expected_config = decode(expected).map_err(not_attempted)?;
    let target_config = decode(target).map_err(not_attempted)?;
    let actual = access.apply_configuration_detailed(
        &expected_config,
        &target_config,
        backup_dir,
        |_| {},
    )?;
    if actual != target_config {
        return Err(ApplyFailure {
            message: "Complete configuration apply returned a mismatched readback".into(),
            recovery: Recovery::Unverified,
        });
    }
    // Keep the exact imported JSON bytes; device success is established against
    // the decoded configuration, while these are the bytes the core reviewed.
    Ok(target.clone())
}

fn not_attempted(message: String) -> ApplyFailure {
    ApplyFailure {
        message,
        recovery: Recovery::NotAttempted,
    }
}

fn review_captured(
    before_config: &Configuration,
    target: &NativeArchive,
    target_config: &Configuration,
) -> Result<Review, String> {
    let forward = super::configuration_plan::plan(before_config, target_config)?;
    // Require the complete restore direction to be representable before the
    // user can accept this review in a later apply phase.
    super::configuration_plan::plan(target_config, before_config)?;
    let before = encode(before_config)?;
    let mut changes = Vec::new();
    add_count(
        &mut changes,
        "keymaps",
        "Key bindings",
        forward.key_bindings,
    );
    add_count(
        &mut changes,
        "macros",
        "Macro slots",
        forward.macro_slots.len(),
    );
    add_count(
        &mut changes,
        "picture",
        "Per-key colors",
        forward.picture_keys,
    );
    if forward.lighting {
        changes.push(SectionChange {
            id: "lighting".into(),
            label: "Global lighting".into(),
            count: Some(1),
        });
    }
    add_count(&mut changes, "settings", "Settings", forward.settings.len());
    Ok(Review {
        before,
        target: target.clone(),
        changes,
    })
}

fn add_count(changes: &mut Vec<SectionChange>, id: &str, label: &str, count: usize) {
    if count > 0 {
        changes.push(SectionChange {
            id: id.into(),
            label: label.into(),
            count: Some(count as u32),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nia87::{
        configuration::Configuration, device::Snapshot, lighting::Lighting, settings::Settings,
    };

    fn fixture() -> Configuration {
        let mut replies = [[0u8; 64]; 4];
        for (reply, opcode) in replies.iter_mut().zip([0x91, 0x97, 0x92, 0x86]) {
            reply[0] = opcode;
        }
        replies[0][2] = 1;
        replies[2][1..9].copy_from_slice(&[120, 0, 120, 0, 88, 2, 88, 2]);
        replies[3][2] = 0x10;
        let mut lighting = [0; 64];
        lighting[0] = crate::nia87::lighting::LED_READ_COMMAND;
        Configuration {
            keymaps: Snapshot {
                format_version: 1,
                firmware: 0x100,
                profile: 0,
                base: vec![[0, 0, 4, 0]; 128],
                function: vec![[0, 0, 4, 0]; 128],
            },
            macros: vec![vec![0; 256]; 50],
            lighting: Lighting::decode(&lighting).unwrap(),
            picture: vec![[0; 3]; 128],
            settings: Settings::decode(&replies[0], &replies[1], &replies[2], &replies[3]).unwrap(),
        }
    }

    #[test]
    fn archive_decode_rejects_wrong_identity_and_malformed_bytes() {
        let target = encode(&fixture()).unwrap();
        let mut wrong = target.clone();
        wrong.backend_id = "other".into();
        assert!(decode(&wrong).is_err());
        wrong = target.clone();
        wrong.format_id = "other".into();
        assert!(decode(&wrong).is_err());
        let mut malformed = target;
        malformed.bytes.truncate(30);
        assert!(decode(&malformed).is_err());
    }

    #[test]
    fn section_summary_uses_both_forward_and_reverse_plan() {
        let before = fixture();
        let mut target = before.clone();
        target.keymaps.base[0] = [0, 0, 5, 0];
        target.picture[0] = [1, 2, 3];
        target.macros[0][2] = 250; // Invalid changed macro must be rejected.
        let native = encode(&target).unwrap();
        // Archive structural validity does not imply that its changed macro is writable.
        assert!(review_captured(&before, &native, &target).is_err());

        target = before.clone();
        target.keymaps.base[0] = [0, 0, 5, 0];
        target.picture[0] = [1, 2, 3];
        let valid = encode(&target).unwrap();
        let review = review_captured(&before, &valid, &target).unwrap();
        assert_eq!(review.before, encode(&before).unwrap());
        assert_eq!(review.target, valid);
        assert_eq!(
            review.changes,
            vec![
                SectionChange {
                    id: "keymaps".into(),
                    label: "Key bindings".into(),
                    count: Some(1)
                },
                SectionChange {
                    id: "picture".into(),
                    label: "Per-key colors".into(),
                    count: Some(1)
                },
            ]
        );
        assert_eq!(decode(&valid).unwrap(), target);
        assert!(!native.bytes.is_empty());
    }

    #[test]
    fn review_rejects_restore_direction_that_cannot_recreate_original_settings() {
        let mut before = fixture();
        let mut replies = [0x91, 0x97, 0x92, 0x86]
            .map(|opcode| before.settings.raw_reply(opcode).unwrap().to_vec());
        replies[3][4] = 1;
        before.settings =
            Settings::decode(&replies[0], &replies[1], &replies[2], &replies[3]).unwrap();

        let mut target = before.clone();
        let mut target_replies = [0x91, 0x97, 0x92, 0x86]
            .map(|opcode| target.settings.raw_reply(opcode).unwrap().to_vec());
        target_replies[3][2] = 0;
        target_replies[3][4] = 0;
        target.settings = Settings::decode(
            &target_replies[0],
            &target_replies[1],
            &target_replies[2],
            &target_replies[3],
        )
        .unwrap();
        let archive = encode(&target).unwrap();
        assert!(
            review_captured(&before, &archive, &target)
                .unwrap_err()
                .contains("cannot be restored")
        );
    }

    #[test]
    fn apply_preflight_rejections_are_typed_not_attempted() {
        let before = encode(&fixture()).unwrap();
        let mut malformed = before.clone();
        malformed.bytes.truncate(24);
        let error = apply(&before, &malformed, Path::new("unused-backups")).unwrap_err();
        assert_eq!(error.recovery, Recovery::NotAttempted);

        let mut invalid_target = fixture();
        invalid_target.macros[0][2] = 250;
        let invalid_target = encode(&invalid_target).unwrap();
        let error = apply(&before, &invalid_target, Path::new("unused-backups")).unwrap_err();
        assert_eq!(error.recovery, Recovery::NotAttempted);
        assert!(error.message.contains("macro slot"));
    }
}
