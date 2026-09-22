//! Whole-configuration capture, apply, and recovery on one locked HID session.
use super::{
    FeatureSetter, HidDevice, Result, Session, lighting_restore_report, open_unique,
    read_lighting_on_device, read_macro_on_device, read_payload, read_picture_on_device,
    read_settings_on_device, snapshot_on_device, write_binding, write_lighting_report,
    write_macro_bytes,
};
/// Capture all supported local configuration data without sending setters.
/// One handle and lock cover both complete sweeps. Other Byakko processes cannot
/// intervene; an external configurator must still be closed. Progress counts
/// completed macro slots across the two sweeps, out of 100.
pub fn capture_configuration(
    progress: impl FnMut(usize, usize),
) -> Result<crate::configuration::Configuration> {
    let session = Session::open()?;
    capture_configuration_on_device(session.device(), progress)
}

fn capture_configuration_on_device(
    device: &HidDevice,
    mut progress: impl FnMut(usize, usize),
) -> Result<crate::configuration::Configuration> {
    let mut capture = |pass: usize| -> Result<crate::configuration::Configuration> {
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
            progress(pass * 50 + usize::from(slot) + 1, 100);
        }
        // Check identity and maps again after the longer macro sweep.
        if snapshot_on_device(device)? != keymaps {
            return Err("Keyboard identity or keymaps changed during configuration capture".into());
        }
        Ok(crate::configuration::Configuration {
            keymaps,
            macros,
            lighting,
            picture,
            settings,
        })
    };
    let first = capture(0)?;
    let second = capture(1)?;
    if first != second {
        return Err("Configuration changed between complete captures; no archive saved".into());
    }
    crate::configuration::validate(&first)?;
    Ok(first)
}

/// Apply a previously reviewed archive against an exact expected before-image.
/// The OS lock and HID handle remain owned through validation, backup, writes,
/// complete verification and any recovery attempt. Host capture is never started.
pub fn apply_configuration(
    expected: &crate::configuration::Configuration,
    target: &crate::configuration::Configuration,
    backup_dir: &std::path::Path,
    mut progress: impl FnMut(&str),
) -> Result<crate::configuration::Configuration> {
    let plan = crate::configuration_plan::plan(expected, target)?;
    // Recovery must be representable before the first setter is sent.
    let reverse = crate::configuration_plan::plan(target, expected)?;
    let session = Session::open()?;
    let device = session.device();
    progress("Checking complete current configuration");
    if &capture_configuration_on_device(device, |_, _| {})? != expected {
        return Err("Configuration changed since review; no writes sent".into());
    }
    if expected == target {
        return Ok(expected.clone());
    }
    std::fs::create_dir_all(backup_dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = backup_dir.join(format!("configuration-before-{stamp}.json"));
    crate::configuration::save_new(&path, expected)?;
    let result = (|| -> Result<crate::configuration::Configuration> {
        progress("Writing reviewed configuration changes");
        write_configuration_changes(device, expected, target, &plan)?;
        progress("Verifying complete configuration");
        let actual = capture_configuration_on_device(device, |_, _| {})?;
        if &actual != target {
            return Err("Complete configuration readback mismatch".into());
        }
        Ok(actual)
    })();
    match result {
        Ok(actual) => Ok(actual),
        Err(error) => {
            progress("Restoring original configuration after failure");
            let restore = recover_configuration(device, target, expected, &reverse);
            Err(format!(
                "Configuration apply failed: {error}; restore: {}; backup {}",
                match restore {
                    Ok(()) => "verified".into(),
                    Err(e) => e.to_string(),
                },
                path.display()
            )
            .into())
        }
    }
}

fn recover_configuration(
    device: &HidDevice,
    attempted: &crate::configuration::Configuration,
    original: &crate::configuration::Configuration,
    reverse: &crate::configuration_plan::ChangeSummary,
) -> Result<()> {
    let version = read_payload(device, 0x80, 0, 0)?;
    let profile = read_payload(device, 0x85, 0, 0)?;
    if version[0..3] != [0x80, 0, 1] || profile[0..2] != [0x85, 0] {
        return Err("Recovery identity check failed; durable archive retained".into());
    }
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
        let slots =
            match crate::recovery_keymaps::slots_to_restore(observed_map, attempted_map, wanted) {
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
                let report =
                    crate::lighting::per_key_color_report(0, slot as u8, original.picture[slot])?;
                let mut host = [0u8; 65];
                host[1..].copy_from_slice(&report);
                device.send_setter(&host)?;
                std::thread::sleep(std::time::Duration::from_millis(100));
                Ok(())
            })();
            attempt(format!("picture {slot}"), result);
        }
    }
    for &setting in &reverse.settings {
        let result = (|| -> Result<()> {
            let report = if matches!(setting, crate::settings::Setting::Backlight(_)) {
                crate::settings::backlight_write_report(
                    original
                        .settings
                        .raw_reply(0x86)
                        .expect("validated options"),
                )?
            } else {
                crate::settings::write_report(setting)?
            };
            let mut host = [0u8; 65];
            host[1..].copy_from_slice(&report);
            device.send_setter(&host)?;
            std::thread::sleep(std::time::Duration::from_millis(500));
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
    let verified = crate::recovery_verification::verify(original, first, || {
        // An aborted Windows feature request can leave this handle unusable.
        // Reopen only for readback under the same lock, never to retry setters.
        let (_, fresh) = open_unique().map_err(|e| e.to_string())?;
        capture_configuration_on_device(&fresh, |_, _| {}).map_err(|e| e.to_string())
    });
    match verified {
        Ok(()) => Ok(()),
        Err(verification) => Err(format!(
            "Recovery unverified: {verification}; section failures: {}",
            failures.join("; ")
        )
        .into()),
    }
}

fn write_configuration_changes(
    device: &HidDevice,
    before: &crate::configuration::Configuration,
    target: &crate::configuration::Configuration,
    plan: &crate::configuration_plan::ChangeSummary,
) -> Result<()> {
    // Revalidate identity on this same handle before apply or recovery.
    let version = read_payload(device, 0x80, 0, 0)?;
    let profile = read_payload(device, 0x85, 0, 0)?;
    if version[0..3] != [0x80, 0, 1] || profile[0..2] != [0x85, 0] {
        return Err("Configuration write identity check failed".into());
    }
    // Macro contents precede the key bindings that refer to them.
    for &slot in &plan.macro_slots {
        write_macro_bytes(device, slot, &target.macros[usize::from(slot)])?;
        if read_macro_on_device(device, slot)? != target.macros[usize::from(slot)] {
            return Err(format!("Macro {slot} readback mismatch").into());
        }
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
                write_binding(device, function, 0, slot, new[slot])?;
            }
        }
    }
    if snapshot_on_device(device)? != target.keymaps {
        return Err("Keymap section readback mismatch".into());
    }
    if plan.picture_keys > 0 {
        for slot in 0..126 {
            if before.picture[slot] != target.picture[slot] {
                let report =
                    crate::lighting::per_key_color_report(0, slot as u8, target.picture[slot])?;
                let mut host = [0u8; 65];
                host[1..].copy_from_slice(&report);
                device.send_setter(&host)?;
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        if read_picture_on_device(device)? != target.picture {
            return Err("Picture section readback mismatch".into());
        }
    }
    for &setting in &plan.settings {
        let report = if matches!(setting, crate::settings::Setting::Backlight(_)) {
            crate::settings::backlight_write_report(
                target.settings.raw_reply(0x86).expect("validated options"),
            )?
        } else {
            crate::settings::write_report(setting)?
        };
        let mut host = [0u8; 65];
        host[1..].copy_from_slice(&report);
        device.send_setter(&host)?;
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    if !plan.settings.is_empty() && read_settings_on_device(device)? != target.settings {
        return Err("Settings section readback mismatch".into());
    }
    if plan.lighting {
        write_lighting_report(device, &lighting_restore_report(&target.lighting))?;
        if read_lighting_on_device(device)? != target.lighting {
            return Err("Lighting section readback mismatch".into());
        }
    }
    Ok(())
}
