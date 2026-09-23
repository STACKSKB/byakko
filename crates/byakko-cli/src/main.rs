//! Nia87 composition root for the native CLI.
use byakko_core::{
    State, archive::NativeArchive, lighting::Snapshot as LightingSnapshot,
    settings::Snapshot as SettingsSnapshot,
};
use byakko_devices::{
    Executor,
    nia87::{self, BoundNia87Adapter},
};
use std::io::Write;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const USAGE: &str = "Usage: byakko-cli <devices|describe|read|read-colors|read-lighting|read-settings|read-macro <slot-id>|capture-archive <new-file>|review-archive <file>|compare-archives <before-file> <after-file>|plan-keymap <state-file>|apply-keymap <state-file>|plan-settings <snapshot-file>|apply-settings <snapshot-file>|plan-lighting <snapshot-file>|apply-lighting <snapshot-file>>";
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments
        .first()
        .is_some_and(|command| command == "compare-archives")
    {
        if arguments.len() != 3 {
            return Err(USAGE.into());
        }
        let max_bytes = nia87::archive_adapter::capabilities().max_bytes;
        let before = load_archive(&arguments[1], max_bytes)?;
        let after = load_archive(&arguments[2], max_bytes)?;
        let changes = nia87::archive_adapter::compare(&before, &after)?;
        println!("{}", serde_json::to_string_pretty(&changes)?);
        return Ok(());
    }
    let macro_read = arguments
        .first()
        .is_some_and(|command| command == "read-macro");
    let archive_capture = arguments
        .first()
        .is_some_and(|command| command == "capture-archive");
    let archive_review = arguments
        .first()
        .is_some_and(|command| command == "review-archive");
    let keymap_file = arguments
        .first()
        .is_some_and(|command| command == "plan-keymap" || command == "apply-keymap");
    let settings_file = arguments
        .first()
        .is_some_and(|command| command == "plan-settings" || command == "apply-settings");
    let lighting_file = arguments
        .first()
        .is_some_and(|command| command == "plan-lighting" || command == "apply-lighting");
    if arguments.len() > 2
        || (arguments.len() == 2
            && !(macro_read
                || archive_capture
                || archive_review
                || keymap_file
                || settings_file
                || lighting_file))
        || ((macro_read
            || archive_capture
            || archive_review
            || keymap_file
            || settings_file
            || lighting_file)
            && arguments.len() != 2)
    {
        return Err(USAGE.into());
    }
    if archive_capture && std::path::Path::new(&arguments[1]).exists() {
        return Err("Archive output already exists; choose a new file".into());
    }
    match arguments.first().map(String::as_str) {
        None | Some("--help") => {
            println!(
                "{USAGE}\n\nRead commands export verified USB state as JSON; compare-archives is offline. To edit keys, one setting, or global lighting, save the matching read output, change its editable value, run the matching plan command, then explicitly run apply. Apply writes to the device with a durable backup and complete readback."
            );
        }
        Some("devices") => match nia87::device::availability() {
            nia87::device::Availability::Unavailable => println!("Nia87 not connected"),
            nia87::device::Availability::Available(candidate) => {
                println!("Nia87 configuration interface: {}", candidate.path)
            }
            nia87::device::Availability::Ambiguous(candidates) => {
                return Err(format!(
                    "{} matching Nia87 interfaces; select one before reading",
                    candidates.len()
                )
                .into());
            }
            nia87::device::Availability::EnumerationFailed(reason) => return Err(reason.into()),
        },
        Some("describe") => println!("{}", serde_json::to_string_pretty(&nia87::descriptor())?),
        Some(
            "read" | "read-colors" | "read-lighting" | "read-settings" | "read-macro"
            | "capture-archive" | "review-archive" | "plan-keymap" | "apply-keymap"
            | "plan-settings" | "apply-settings" | "plan-lighting" | "apply-lighting",
        ) => {
            let candidate = match nia87::device::availability() {
                nia87::device::Availability::Available(candidate) => candidate,
                nia87::device::Availability::Unavailable => {
                    return Err("Nia87 not connected".into());
                }
                nia87::device::Availability::Ambiguous(candidates) => {
                    return Err(format!(
                        "{} matching Nia87 interfaces; read refused",
                        candidates.len()
                    )
                    .into());
                }
                nia87::device::Availability::EnumerationFailed(reason) => return Err(reason.into()),
            };
            let target = nia87::device::Target::from_candidate(&candidate)?;
            let backups = byakko_devices::storage::user_data_dir()?.join("backups");
            let executor = Executor::spawn(BoundNia87Adapter::new(target), backups.clone())?;
            let mut session = nia87::application::session()?;
            let mut review_target = if archive_review {
                let max_bytes = session
                    .archive_capabilities()
                    .ok_or("Nia87 has no native archive capability")?
                    .max_bytes;
                Some(load_archive(&arguments[1], max_bytes)?)
            } else {
                None
            };
            let keymap_target = if keymap_file {
                Some(load_state(&arguments[1])?)
            } else {
                None
            };
            let settings_target = if settings_file {
                Some(load_settings(&arguments[1])?)
            } else {
                None
            };
            let lighting_target = if lighting_file {
                Some(load_lighting(&arguments[1])?)
            } else {
                None
            };
            let keymap = if macro_read {
                None
            } else {
                Some(byakko_cli::read_keymap(
                    &mut session,
                    &executor,
                    Duration::from_secs(30),
                )?)
            };
            let json = match arguments[0].as_str() {
                "read" => serde_json::to_string_pretty(keymap.as_ref().expect("read above"))?,
                "read-colors" => serde_json::to_string_pretty(&byakko_cli::read_colors(
                    &mut session,
                    &executor,
                    Duration::from_secs(30),
                )?)?,
                "read-lighting" => serde_json::to_string_pretty(&byakko_cli::read_lighting(
                    &mut session,
                    &executor,
                    Duration::from_secs(30),
                )?)?,
                "read-settings" => serde_json::to_string_pretty(&byakko_cli::read_settings(
                    &mut session,
                    &executor,
                    Duration::from_secs(30),
                )?)?,
                "read-macro" => serde_json::to_string_pretty(&byakko_cli::read_macro(
                    &mut session,
                    &executor,
                    &arguments[1],
                    Duration::from_secs(30),
                )?)?,
                "capture-archive" => {
                    let archive = byakko_cli::capture_archive(
                        &mut session,
                        &executor,
                        Duration::from_secs(180),
                    )?;
                    let bytes = serde_json::to_vec(&archive)?;
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&arguments[1])?;
                    file.write_all(&bytes)?;
                    file.write_all(b"\n")?;
                    file.sync_all()?;
                    eprintln!("Verified archive saved to {}", arguments[1]);
                    return Ok(());
                }
                "review-archive" => serde_json::to_string_pretty(
                    &byakko_cli::review_archive(
                        &mut session,
                        &executor,
                        review_target.take().expect("loaded above"),
                        Duration::from_secs(180),
                    )?
                    .changes,
                )?,
                "plan-keymap" | "apply-keymap" => {
                    let target = keymap_target.as_ref().expect("loaded above");
                    let canonical =
                        nia87::adapter::from_snapshot(&nia87::adapter::to_snapshot(target)?)?;
                    if canonical.bindings != target.bindings {
                        return Err("Keymap file contains noncanonical actions; export a fresh `read` state and edit its bindings".into());
                    }
                    let changes = byakko_cli::plan_keymap(&session, target)?;
                    let adapter = nia87::Nia87Adapter;
                    adapter.validate(keymap.as_ref().expect("read above"), &changes)?;
                    if arguments[0] == "plan-keymap" {
                        serde_json::to_string_pretty(&changes)?
                    } else {
                        if changes.is_empty() {
                            return Err("Keymap file contains no changes".into());
                        }
                        eprintln!(
                            "Applying {} key changes to {}",
                            changes.len(),
                            candidate.path
                        );
                        eprintln!("Before-image backup directory: {}", backups.display());
                        serde_json::to_string_pretty(&byakko_cli::apply_keymap(
                            &mut session,
                            &executor,
                            target,
                        )?)?
                    }
                }
                "plan-settings" | "apply-settings" => {
                    byakko_cli::read_settings(&mut session, &executor, Duration::from_secs(30))?;
                    let target = settings_target.as_ref().expect("loaded above");
                    let changes = byakko_cli::plan_settings(&session, target)?;
                    if arguments[0] == "plan-settings" {
                        serde_json::to_string_pretty(&changes)?
                    } else {
                        if changes.is_empty() {
                            return Err("Settings file contains no changes".into());
                        }
                        eprintln!(
                            "Applying {} setting change to {}",
                            changes.len(),
                            candidate.path
                        );
                        eprintln!("Before-image backup directory: {}", backups.display());
                        serde_json::to_string_pretty(&byakko_cli::apply_settings(
                            &mut session,
                            &executor,
                            target,
                        )?)?
                    }
                }
                "plan-lighting" | "apply-lighting" => {
                    let current = byakko_cli::read_lighting(
                        &mut session,
                        &executor,
                        Duration::from_secs(30),
                    )?;
                    let target = lighting_target.as_ref().expect("loaded above");
                    let desired = byakko_cli::plan_lighting(&session, target)?;
                    if let Some(ref setting) = desired {
                        nia87::lighting_adapter::draft(&current, setting)?;
                    }
                    if arguments[0] == "plan-lighting" {
                        serde_json::to_string_pretty(&desired)?
                    } else {
                        if desired.is_none() {
                            return Err("Lighting file contains no changes".into());
                        }
                        eprintln!("Applying global lighting to {}", candidate.path);
                        eprintln!("Before-image backup directory: {}", backups.display());
                        serde_json::to_string_pretty(&byakko_cli::apply_lighting(
                            &mut session,
                            &executor,
                            target,
                        )?)?
                    }
                }
                _ => unreachable!("read command matched above"),
            };
            println!("{json}");
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn load_state(path: &str) -> Result<State, Box<dyn std::error::Error>> {
    const MAX_STATE_JSON: u64 = 1024 * 1024;
    if std::fs::metadata(path)?.len() > MAX_STATE_JSON {
        return Err("Keymap state file exceeds the 1 MiB JSON limit".into());
    }
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

fn load_settings(path: &str) -> Result<SettingsSnapshot, Box<dyn std::error::Error>> {
    const MAX_SETTINGS_JSON: u64 = 1024 * 1024;
    if std::fs::metadata(path)?.len() > MAX_SETTINGS_JSON {
        return Err("Settings snapshot file exceeds the 1 MiB JSON limit".into());
    }
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

fn load_lighting(path: &str) -> Result<LightingSnapshot, Box<dyn std::error::Error>> {
    const MAX_LIGHTING_JSON: u64 = 1024 * 1024;
    if std::fs::metadata(path)?.len() > MAX_LIGHTING_JSON {
        return Err("Lighting snapshot file exceeds the 1 MiB JSON limit".into());
    }
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

fn load_archive(path: &str, max_bytes: u32) -> Result<NativeArchive, Box<dyn std::error::Error>> {
    let limit = u64::from(max_bytes).saturating_mul(4).saturating_add(1024);
    if std::fs::metadata(path)?.len() > limit {
        return Err("Archive input exceeds the supported JSON file size".into());
    }
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}
