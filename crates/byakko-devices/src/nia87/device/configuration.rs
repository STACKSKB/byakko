//! Whole-configuration capture, apply, and recovery on one locked HID session.
use super::transaction::{pacing, save_encoded_backup};
use super::{
    FeatureSetter, HidDevice, Result, Selection, Session, lighting_restore_report,
    read_lighting_on_device, read_macro_on_device, read_picture_on_device, read_settings_on_device,
    snapshot_on_device, write_binding, write_lighting_report, write_macro_bytes,
};
use byakko_core::session::{ApplyFailure, Recovery};
use std::fmt;

#[derive(Debug)]
struct ConfigurationApplyError(ApplyFailure);

impl fmt::Display for ConfigurationApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.message)
    }
}
impl std::error::Error for ConfigurationApplyError {}

#[derive(Debug)]
enum RecoveryResult {
    Verified(String),
    Failed(String, Box<crate::nia87::configuration::Configuration>),
    Unverified(String),
}
/// Capture all supported local configuration data without sending setters.
/// One bound handle and lock cover the complete capture. Progress counts
/// completed macro slots out of 50.
pub fn capture_configuration(
    progress: impl FnMut(usize, usize),
) -> Result<crate::nia87::configuration::Configuration> {
    capture_selected(Selection::Unique, progress)
}

pub(super) fn capture_selected(
    selection: Selection<'_>,
    progress: impl FnMut(usize, usize),
) -> Result<crate::nia87::configuration::Configuration> {
    let session = Session::open_for(selection)?;
    capture_configuration_on_device(session.device(), progress)
}

fn capture_configuration_on_device(
    device: &HidDevice,
    mut progress: impl FnMut(usize, usize),
) -> Result<crate::nia87::configuration::Configuration> {
    let keymaps = snapshot_on_device(device)?;
    if keymaps.firmware != 0x0100 || keymaps.profile != 0 {
        return Err("Configuration capture requires firmware 0x0100, profile 0".into());
    }
    let lighting = read_lighting_on_device(device)?;
    let settings = read_settings_on_device(device)?;
    let picture = read_picture_on_device(device)?;
    let mut macros = Vec::with_capacity(50);
    for slot in 0..50 {
        macros.push(read_macro_on_device(device, slot)?);
        progress(usize::from(slot) + 1, 50);
    }
    let capture = crate::nia87::configuration::Configuration {
        keymaps,
        macros,
        lighting,
        picture,
        settings,
    };
    crate::nia87::configuration::validate(&capture)?;
    Ok(capture)
}

/// Apply a previously reviewed archive from its cached before-image.
/// The OS lock and HID handle remain owned through validation, backup, writes,
/// complete verification and any recovery attempt. Host capture is never started.
pub fn apply_configuration(
    expected: &crate::nia87::configuration::Configuration,
    target: &crate::nia87::configuration::Configuration,
    backup_dir: &std::path::Path,
    progress: impl FnMut(&str),
) -> Result<crate::nia87::configuration::Configuration> {
    apply_configuration_selected(Selection::Unique, expected, target, backup_dir, progress)
}

fn apply_configuration_selected(
    selection: Selection<'_>,
    expected: &crate::nia87::configuration::Configuration,
    target: &crate::nia87::configuration::Configuration,
    backup_dir: &std::path::Path,
    mut progress: impl FnMut(&str),
) -> Result<crate::nia87::configuration::Configuration> {
    let plan = crate::nia87::configuration_plan::plan(expected, target)?;
    // Recovery must be representable before the first setter is sent.
    let reverse = crate::nia87::configuration_plan::plan(target, expected)?;
    if expected == target {
        return Ok(expected.clone());
    }
    let session = Session::open_for(selection)?;
    let device = session.device();
    let encoded = crate::nia87::configuration::encode(expected)?;
    let backup = save_encoded_backup(backup_dir, "configuration-before", &encoded)?;
    let stamp = backup.stamp();
    let path = backup.path();
    let mut setter_started = false;
    let mut mismatched_readback = None;
    let result = (|| -> Result<crate::nia87::configuration::Configuration> {
        progress("Writing reviewed configuration changes");
        write_configuration_changes(device, expected, target, &plan, &mut setter_started)?;
        progress("Verifying complete configuration");
        let actual = capture_configuration_on_device(device, |_, _| {})?;
        if &actual != target {
            mismatched_readback = Some(actual);
            return Err("Complete configuration readback mismatch".into());
        }
        Ok(actual)
    })();
    match result {
        Ok(actual) => Ok(actual),
        Err(error) => {
            if !setter_started {
                return Err(error);
            }
            progress("Restoring original configuration after failure");
            let restore = recover_configuration(selection, device, target, expected, &reverse);
            let (recovery, detail) = match restore {
                RecoveryResult::Verified(message) => (Recovery::Verified, message),
                RecoveryResult::Failed(message, actual) => (
                    Recovery::Failed,
                    format!(
                        "{message}; {}",
                        retain_readback(
                            &backup_dir
                                .join(format!("configuration-recovery-mismatch-{stamp}.json")),
                            &actual,
                        )
                    ),
                ),
                RecoveryResult::Unverified(message) => (Recovery::Unverified, message),
            };
            // Diagnostic persistence follows recovery and never changes its outcome.
            let evidence = mismatched_readback.map(|actual| {
                retain_readback(
                    &backup_dir.join(format!("configuration-apply-mismatch-{stamp}.json")),
                    &actual,
                )
            });
            Err(ConfigurationApplyError(ApplyFailure {
                message: format!(
                    "Configuration apply failed: {error}; recovery: {detail}; backup {}{}",
                    path.display(),
                    evidence.map_or_else(String::new, |message| format!("; {message}"))
                ),
                recovery,
            })
            .into())
        }
    }
}

/// Apply the native archive while preserving whether recovery verified,
/// definitely failed, or could not establish the original state.
pub fn apply_configuration_detailed(
    expected: &crate::nia87::configuration::Configuration,
    target: &crate::nia87::configuration::Configuration,
    backup_dir: &std::path::Path,
    progress: impl FnMut(&str),
) -> std::result::Result<crate::nia87::configuration::Configuration, ApplyFailure> {
    apply_detailed_selected(Selection::Unique, expected, target, backup_dir, progress)
}

pub(super) fn apply_detailed_selected(
    selection: Selection<'_>,
    expected: &crate::nia87::configuration::Configuration,
    target: &crate::nia87::configuration::Configuration,
    backup_dir: &std::path::Path,
    progress: impl FnMut(&str),
) -> std::result::Result<crate::nia87::configuration::Configuration, ApplyFailure> {
    apply_configuration_selected(selection, expected, target, backup_dir, progress).map_err(
        |error| {
            error.downcast_ref::<ConfigurationApplyError>().map_or_else(
                || ApplyFailure {
                    message: error.to_string(),
                    recovery: Recovery::NotAttempted,
                },
                |typed| typed.0.clone(),
            )
        },
    )
}

fn recover_configuration(
    selection: Selection<'_>,
    device: &HidDevice,
    attempted: &crate::nia87::configuration::Configuration,
    original: &crate::nia87::configuration::Configuration,
    reverse: &crate::nia87::configuration_plan::ChangeSummary,
) -> RecoveryResult {
    let mut failures = Vec::new();
    let mut attempt = |label: String, result: Result<()>| {
        if let Err(error) = result {
            failures.push(format!("{label}: {error}"));
        }
    };
    // Remove new bindings first, then restore their macro contents. Reread
    // between layers to detect a setter's unexpected effect on the other map.
    for function in [true, false] {
        let observed = match snapshot_on_device(device) {
            Ok(snapshot) => Some(snapshot),
            Err(error) => {
                attempt(
                    format!("keymap read {function}; recovery limited to planned slots"),
                    Err(error),
                );
                None
            }
        };
        let wanted = if function {
            &original.keymaps.function
        } else {
            &original.keymaps.base
        };
        let attempted_map = if function {
            &attempted.keymaps.function
        } else {
            &attempted.keymaps.base
        };
        let observed_map = observed.as_ref().map(|snapshot| {
            if function {
                snapshot.function.as_slice()
            } else {
                snapshot.base.as_slice()
            }
        });
        let slots = match crate::nia87::recovery_keymaps::slots_to_restore(
            observed_map,
            attempted_map,
            wanted,
        ) {
            Ok(slots) => slots,
            Err(error) => {
                attempt(
                    format!("keymap recovery plan {function}"),
                    Err(error.into()),
                );
                continue;
            }
        };
        for slot in slots {
            attempt(
                format!("key {function}/{slot}"),
                write_binding(device, function, 0, slot, wanted[slot]),
            );
        }
    }
    for &slot in &reverse.macro_slots {
        attempt(
            format!("macro {slot}"),
            write_macro_bytes(device, slot, &original.macros[usize::from(slot)]),
        );
    }
    for slot in 0..126 {
        if attempted.picture[slot] != original.picture[slot] {
            let result = (|| -> Result<()> {
                let report = crate::nia87::lighting::per_key_color_report(
                    0,
                    slot as u8,
                    original.picture[slot],
                )?;
                let mut host = [0u8; 65];
                host[1..].copy_from_slice(&report);
                device.send_setter(&host)?;
                std::thread::sleep(pacing::ARCHIVE_PICTURE_KEY);
                Ok(())
            })();
            attempt(format!("picture {slot}"), result);
        }
    }
    for &setting in &reverse.settings {
        let result = (|| -> Result<()> {
            let report = if matches!(setting, crate::nia87::settings::Setting::Backlight(_)) {
                crate::nia87::settings::backlight_write_report(
                    original
                        .settings
                        .raw_reply(0x86)
                        .expect("validated options"),
                )?
            } else {
                crate::nia87::settings::write_report(setting)?
            };
            let mut host = [0u8; 65];
            host[1..].copy_from_slice(&report);
            device.send_setter(&host)?;
            std::thread::sleep(pacing::SETTING_SETTER);
            Ok(())
        })();
        attempt(format!("setting {setting:?}"), result);
    }
    if reverse.lighting {
        attempt(
            "lighting".into(),
            write_lighting_report(device, &lighting_restore_report(&original.lighting)),
        );
    }
    // A transient write error may still have delivered its packet. Only full
    // readback establishes recovery; do not skip it after an individual error.
    let first = capture_configuration_on_device(device, |_, _| {}).map_err(|e| e.to_string());
    let verified = crate::nia87::recovery_verification::verify(original, first, || {
        // An aborted Windows feature request can leave this handle unusable.
        // Reopen only for readback under the same lock, never to retry setters.
        let (_, fresh) = selection.open().map_err(|e| e.to_string())?;
        capture_configuration_on_device(&fresh, |_, _| {}).map_err(|e| e.to_string())
    });
    let section_failures = failures.join("; ");
    match verified {
        Ok(()) => RecoveryResult::Verified(if section_failures.is_empty() {
            "original configuration verified".into()
        } else {
            format!("original configuration verified after section errors: {section_failures}")
        }),
        Err(crate::nia87::recovery_verification::VerificationFailure::Mismatch {
            actual, ..
        }) => RecoveryResult::Failed(
            format!(
                "original configuration readback mismatched; section failures: {section_failures}"
            ),
            actual,
        ),
        Err(
            error @ crate::nia87::recovery_verification::VerificationFailure::Unreadable { .. },
        ) => RecoveryResult::Unverified(format!("{error}; section failures: {section_failures}")),
    }
}

/// Best-effort evidence from an existing complete read; never perform device I/O.
fn retain_readback(
    path: &std::path::Path,
    actual: &crate::nia87::configuration::Configuration,
) -> String {
    match crate::nia87::configuration::save_new(path, actual) {
        Ok(()) => format!("mismatched readback saved to {}", path.display()),
        Err(error) => format!(
            "could not save mismatched readback to {}: {error}",
            path.display()
        ),
    }
}

fn write_configuration_changes(
    device: &HidDevice,
    before: &crate::nia87::configuration::Configuration,
    target: &crate::nia87::configuration::Configuration,
    plan: &crate::nia87::configuration_plan::ChangeSummary,
    setter_started: &mut bool,
) -> Result<()> {
    // Macro contents precede the key bindings that refer to them.
    for &slot in &plan.macro_slots {
        *setter_started = true;
        write_macro_bytes(device, slot, &target.macros[usize::from(slot)])?;
    }
    for function in [false, true] {
        let prior = &before.keymaps;
        let (old, new) = if function {
            (&prior.function, &target.keymaps.function)
        } else {
            (&prior.base, &target.keymaps.base)
        };
        for slot in 0..126 {
            if old[slot] != new[slot] {
                *setter_started = true;
                write_binding(device, function, 0, slot, new[slot])?;
            }
        }
    }
    if plan.picture_keys > 0 {
        for slot in 0..126 {
            if before.picture[slot] != target.picture[slot] {
                let report = crate::nia87::lighting::per_key_color_report(
                    0,
                    slot as u8,
                    target.picture[slot],
                )?;
                let mut host = [0u8; 65];
                host[1..].copy_from_slice(&report);
                *setter_started = true;
                device.send_setter(&host)?;
                std::thread::sleep(pacing::ARCHIVE_PICTURE_KEY);
            }
        }
    }
    for &setting in &plan.settings {
        let report = if matches!(setting, crate::nia87::settings::Setting::Backlight(_)) {
            crate::nia87::settings::backlight_write_report(
                target.settings.raw_reply(0x86).expect("validated options"),
            )?
        } else {
            crate::nia87::settings::write_report(setting)?
        };
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(&report);
        *setter_started = true;
        device.send_setter(&host)?;
        std::thread::sleep(pacing::SETTING_SETTER);
    }
    if plan.lighting {
        *setter_started = true;
        write_lighting_report(device, &lighting_restore_report(&target.lighting))?;
    }
    Ok(())
}

#[cfg(test)]
mod evidence_tests {
    use super::retain_readback;
    use crate::nia87::configuration;

    #[test]
    fn retains_complete_readback_without_overwriting_existing_evidence() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "byakko-readback-evidence-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("mismatch.json");
        let actual = configuration::tests::example();
        assert!(retain_readback(&path, &actual).starts_with("mismatched readback saved to "));
        assert_eq!(configuration::load(&path).unwrap(), actual);

        let mut another = actual.clone();
        another.macros[0][0] ^= 0xff;
        let error = retain_readback(&path, &another);
        assert!(error.starts_with("could not save mismatched readback to "));
        assert!(error.contains(&path.display().to_string()));
        assert_eq!(configuration::load(&path).unwrap(), actual);

        // Diagnostic persistence failure is reported as detail, not propagated
        // into the already determined recovery outcome.
        let missing_parent = directory.join("missing/mismatch.json");
        assert!(
            retain_readback(&missing_parent, &actual)
                .starts_with("could not save mismatched readback to ")
        );
    }
}
