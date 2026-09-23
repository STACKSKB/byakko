//! Nia87 composition root for the native CLI.
mod native_macro_restore;

use byakko_core::{
    State,
    archive::NativeArchive,
    lighting::Snapshot as LightingSnapshot,
    macros::{Content as MacroContent, Snapshot as MacroSnapshot},
    picture::Snapshot as PictureSnapshot,
    settings::Snapshot as SettingsSnapshot,
};
use byakko_devices::{
    Executor,
    nia87::{self, BoundNia87Adapter},
};
use native_macro_restore::NativeMacroBackup;
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

const USAGE: &str = "Usage: byakko-cli <devices|describe|read|list-macros|read-colors|read-lighting|read-settings|read-macro <slot-id>|capture-archive <new-file>|review-archive <file>|compare-archives <before-file> <after-file>|plan-keymap <state-file>|apply-keymap <state-file>|plan-settings <snapshot-file>|apply-settings <snapshot-file>|plan-lighting <snapshot-file>|apply-lighting <snapshot-file>|plan-macro <snapshot-file>|apply-macro <snapshot-file>|plan-restore-macro <native-backup-file>|restore-macro <native-backup-file>|plan-colors <snapshot-file>|apply-colors <snapshot-file>>";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Plan,
    Apply,
}

enum Command {
    Help,
    Devices,
    Describe,
    Read,
    ListMacros,
    ReadColors,
    ReadLighting,
    ReadSettings,
    ReadMacro(String),
    CaptureArchive(PathBuf),
    ReviewArchive(NativeArchive),
    CompareArchives(PathBuf, PathBuf),
    Keymap(Mode, State),
    Settings(Mode, SettingsSnapshot),
    Lighting(Mode, LightingSnapshot),
    Macro(Mode, MacroSnapshot),
    NativeMacroRestore(Mode, NativeMacroBackup),
    Colors(Mode, PictureSnapshot),
}

fn parse_command(arguments: &[String]) -> Result<Command, Box<dyn std::error::Error>> {
    match arguments {
        [] => Ok(Command::Help),
        [name] if name == "--help" => Ok(Command::Help),
        [name] if name == "devices" => Ok(Command::Devices),
        [name] if name == "describe" => Ok(Command::Describe),
        [name] if name == "read" => Ok(Command::Read),
        [name] if name == "list-macros" => Ok(Command::ListMacros),
        [name] if name == "read-colors" => Ok(Command::ReadColors),
        [name] if name == "read-lighting" => Ok(Command::ReadLighting),
        [name] if name == "read-settings" => Ok(Command::ReadSettings),
        [name, slot] if name == "read-macro" => Ok(Command::ReadMacro(slot.clone())),
        [name, path] if name == "capture-archive" => {
            let path = PathBuf::from(path);
            if path.exists() {
                return Err("Archive output already exists; choose a new file".into());
            }
            Ok(Command::CaptureArchive(path))
        }
        [name, path] if name == "review-archive" => {
            let max_bytes = nia87::archive_adapter::capabilities().max_bytes;
            Ok(Command::ReviewArchive(load_archive(
                Path::new(path),
                max_bytes,
            )?))
        }
        [name, before, after] if name == "compare-archives" => Ok(Command::CompareArchives(
            PathBuf::from(before),
            PathBuf::from(after),
        )),
        [name, path] if name == "plan-keymap" => Ok(Command::Keymap(Mode::Plan, load_state(path)?)),
        [name, path] if name == "apply-keymap" => {
            Ok(Command::Keymap(Mode::Apply, load_state(path)?))
        }
        [name, path] if name == "plan-settings" => {
            Ok(Command::Settings(Mode::Plan, load_settings(path)?))
        }
        [name, path] if name == "apply-settings" => {
            Ok(Command::Settings(Mode::Apply, load_settings(path)?))
        }
        [name, path] if name == "plan-lighting" => {
            Ok(Command::Lighting(Mode::Plan, load_lighting(path)?))
        }
        [name, path] if name == "apply-lighting" => {
            Ok(Command::Lighting(Mode::Apply, load_lighting(path)?))
        }
        [name, path] if name == "plan-macro" => Ok(Command::Macro(Mode::Plan, load_macro(path)?)),
        [name, path] if name == "apply-macro" => Ok(Command::Macro(Mode::Apply, load_macro(path)?)),
        [name, path] if name == "plan-restore-macro" => Ok(Command::NativeMacroRestore(
            Mode::Plan,
            native_macro_restore::load(path)?,
        )),
        [name, path] if name == "restore-macro" => Ok(Command::NativeMacroRestore(
            Mode::Apply,
            native_macro_restore::load(path)?,
        )),
        [name, path] if name == "plan-colors" => {
            Ok(Command::Colors(Mode::Plan, load_colors(path)?))
        }
        [name, path] if name == "apply-colors" => {
            Ok(Command::Colors(Mode::Apply, load_colors(path)?))
        }
        _ => Err(USAGE.into()),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command = parse_command(&std::env::args().skip(1).collect::<Vec<_>>())?;
    match command {
        Command::Help => {
            println!(
                "{USAGE}\n\nRead commands export verified USB state as JSON; compare-archives is offline. To edit keys, one setting, global lighting, a macro slot, or per-key colors, save the matching read output, change its editable value, run the matching plan command, then explicitly run apply. Apply writes to the device with a durable backup and complete readback. To recover a previously stored macro, use plan-restore-macro and restore-macro with a native macro-*-before-*.json backup."
            );
            Ok(())
        }
        Command::Devices => {
            match nia87::device::availability() {
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
            }
            Ok(())
        }
        Command::Describe => {
            println!("{}", serde_json::to_string_pretty(&nia87::descriptor())?);
            Ok(())
        }
        Command::CompareArchives(before, after) => {
            let max_bytes = nia87::archive_adapter::capabilities().max_bytes;
            let before = load_archive(&before, max_bytes)?;
            let after = load_archive(&after, max_bytes)?;
            let changes = nia87::archive_adapter::compare(&before, &after)?;
            println!("{}", serde_json::to_string_pretty(&changes)?);
            Ok(())
        }
        device_command => run_device(device_command),
    }
}

fn run_device(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    let candidate = match nia87::device::availability() {
        nia87::device::Availability::Available(candidate) => candidate,
        nia87::device::Availability::Unavailable => return Err("Nia87 not connected".into()),
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
    if let Command::NativeMacroRestore(mode, backup) = &command {
        return native_macro_restore::run(*mode, backup, target, &backups, &candidate.path);
    }
    let executor = Executor::spawn(BoundNia87Adapter::new(target), backups.clone())?;
    let mut session = nia87::application::session()?;
    let json = match command {
        Command::ListMacros => {
            byakko_cli::read_keymap(&mut session, &executor, Duration::from_secs(30))?;
            serde_json::to_string_pretty(&byakko_cli::read_macro_library(
                &mut session,
                &executor,
                Duration::from_secs(90),
            )?)?
        }
        Command::ReadMacro(slot) => serde_json::to_string_pretty(&byakko_cli::read_macro(
            &mut session,
            &executor,
            &slot,
            Duration::from_secs(30),
        )?)?,
        Command::Macro(mode, target) => {
            let current = byakko_cli::read_macro(
                &mut session,
                &executor,
                &target.slot,
                Duration::from_secs(30),
            )?;
            let desired = byakko_cli::plan_macro(&session, &target)?;
            if let Some(ref program) = desired {
                nia87::macro_adapter::draft(&current, program)?;
            }
            match mode {
                Mode::Plan => {
                    let MacroContent::Editable(before) = &current.content else {
                        return Err("Opaque macros are available only as raw backups".into());
                    };
                    serde_json::to_string_pretty(&serde_json::json!({
                        "slot": target.slot,
                        "changed": desired.is_some(),
                        "before": before,
                        "after": desired.as_ref().unwrap_or(before),
                    }))?
                }
                Mode::Apply => {
                    if desired.is_none() {
                        return Err("Macro file contains no changes".into());
                    }
                    eprintln!("Applying macro {} to {}", target.slot, candidate.path);
                    eprintln!("Before-image backup directory: {}", backups.display());
                    serde_json::to_string_pretty(&byakko_cli::apply_macro(
                        &mut session,
                        &executor,
                        &target,
                    )?)?
                }
            }
        }
        other => {
            let keymap = byakko_cli::read_keymap(&mut session, &executor, Duration::from_secs(30))?;
            match other {
                Command::Read => serde_json::to_string_pretty(&keymap)?,
                Command::ReadColors => serde_json::to_string_pretty(&byakko_cli::read_colors(
                    &mut session,
                    &executor,
                    Duration::from_secs(30),
                )?)?,
                Command::ReadLighting => serde_json::to_string_pretty(&byakko_cli::read_lighting(
                    &mut session,
                    &executor,
                    Duration::from_secs(30),
                )?)?,
                Command::ReadSettings => serde_json::to_string_pretty(&byakko_cli::read_settings(
                    &mut session,
                    &executor,
                    Duration::from_secs(30),
                )?)?,
                Command::CaptureArchive(path) => {
                    let archive = byakko_cli::capture_archive(
                        &mut session,
                        &executor,
                        Duration::from_secs(180),
                    )?;
                    let bytes = serde_json::to_vec(&archive)?;
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)?;
                    file.write_all(&bytes)?;
                    file.write_all(b"\n")?;
                    file.sync_all()?;
                    eprintln!("Verified archive saved to {}", path.display());
                    return Ok(());
                }
                Command::ReviewArchive(target) => serde_json::to_string_pretty(
                    &byakko_cli::review_archive(
                        &mut session,
                        &executor,
                        target,
                        Duration::from_secs(180),
                    )?
                    .changes,
                )?,
                Command::Keymap(mode, target) => {
                    let canonical =
                        nia87::adapter::from_snapshot(&nia87::adapter::to_snapshot(&target)?)?;
                    if canonical.bindings != target.bindings {
                        return Err("Keymap file contains noncanonical actions; export a fresh `read` state and edit its bindings".into());
                    }
                    let changes = byakko_cli::plan_keymap(&session, &target)?;
                    nia87::Nia87Adapter.validate(&keymap, &changes)?;
                    match mode {
                        Mode::Plan => serde_json::to_string_pretty(&changes)?,
                        Mode::Apply => {
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
                                &target,
                            )?)?
                        }
                    }
                }
                Command::Settings(mode, target) => {
                    byakko_cli::read_settings(&mut session, &executor, Duration::from_secs(30))?;
                    let changes = byakko_cli::plan_settings(&session, &target)?;
                    match mode {
                        Mode::Plan => serde_json::to_string_pretty(&changes)?,
                        Mode::Apply => {
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
                                &target,
                            )?)?
                        }
                    }
                }
                Command::Lighting(mode, target) => {
                    let current = byakko_cli::read_lighting(
                        &mut session,
                        &executor,
                        Duration::from_secs(30),
                    )?;
                    let desired = byakko_cli::plan_lighting(&session, &target)?;
                    if let Some(ref setting) = desired {
                        nia87::lighting_adapter::draft(&current, setting)?;
                    }
                    match mode {
                        Mode::Plan => serde_json::to_string_pretty(&desired)?,
                        Mode::Apply => {
                            if desired.is_none() {
                                return Err("Lighting file contains no changes".into());
                            }
                            eprintln!("Applying global lighting to {}", candidate.path);
                            eprintln!("Before-image backup directory: {}", backups.display());
                            serde_json::to_string_pretty(&byakko_cli::apply_lighting(
                                &mut session,
                                &executor,
                                &target,
                            )?)?
                        }
                    }
                }
                Command::Colors(mode, target) => {
                    byakko_cli::read_colors(&mut session, &executor, Duration::from_secs(30))?;
                    let changes = byakko_cli::plan_colors(&session, &target)?;
                    match mode {
                        Mode::Plan => serde_json::to_string_pretty(&changes)?,
                        Mode::Apply => {
                            if changes.is_empty() {
                                return Err("Color file contains no changes".into());
                            }
                            eprintln!(
                                "Applying {} key colors to {}",
                                changes.len(),
                                candidate.path
                            );
                            eprintln!("Before-image backup directory: {}", backups.display());
                            serde_json::to_string_pretty(&byakko_cli::apply_colors(
                                &mut session,
                                &executor,
                                &target,
                            )?)?
                        }
                    }
                }
                Command::Help
                | Command::Devices
                | Command::Describe
                | Command::CompareArchives(..)
                | Command::ListMacros
                | Command::ReadMacro(_)
                | Command::Macro(..)
                | Command::NativeMacroRestore(..) => unreachable!("handled before keymap read"),
            }
        }
    };
    println!("{json}");
    Ok(())
}

fn load_state(path: &str) -> Result<State, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&read_bounded(
        Path::new(path),
        1024 * 1024,
        "Keymap state file exceeds the 1 MiB JSON limit",
    )?)?)
}

fn load_settings(path: &str) -> Result<SettingsSnapshot, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&read_bounded(
        Path::new(path),
        1024 * 1024,
        "Settings snapshot file exceeds the 1 MiB JSON limit",
    )?)?)
}

fn load_lighting(path: &str) -> Result<LightingSnapshot, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&read_bounded(
        Path::new(path),
        1024 * 1024,
        "Lighting snapshot file exceeds the 1 MiB JSON limit",
    )?)?)
}

fn load_macro(path: &str) -> Result<MacroSnapshot, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&read_bounded(
        Path::new(path),
        64 * 1024,
        "Macro snapshot file exceeds the 64 KiB JSON limit",
    )?)?)
}

fn load_colors(path: &str) -> Result<PictureSnapshot, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&read_bounded(
        Path::new(path),
        1024 * 1024,
        "Color snapshot file exceeds the 1 MiB JSON limit",
    )?)?)
}

fn load_archive(path: &Path, max_bytes: u32) -> Result<NativeArchive, Box<dyn std::error::Error>> {
    let limit = u64::from(max_bytes).saturating_mul(4).saturating_add(1024);
    Ok(serde_json::from_slice(&read_bounded(
        path,
        limit,
        "Archive input exceeds the supported JSON file size",
    )?)?)
}

fn read_bounded(
    path: &Path,
    limit: u64,
    size_error: &'static str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if std::fs::metadata(path)?.len() > limit {
        return Err(size_error.into());
    }
    Ok(std::fs::read(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).into()).collect()
    }

    #[test]
    fn parser_rejects_wrong_arity_and_unknown_commands() {
        assert!(matches!(parse_command(&args(&[])).unwrap(), Command::Help));
        assert!(matches!(
            parse_command(&args(&["list-macros"])).unwrap(),
            Command::ListMacros
        ));
        assert!(parse_command(&args(&["read", "extra"])).is_err());
        assert!(parse_command(&args(&["read-macro"])).is_err());
        assert!(parse_command(&args(&["restore-macro"])).is_err());
        assert!(parse_command(&args(&["compare-archives", "only-one"])).is_err());
        assert!(parse_command(&args(&["unknown"])).is_err());
    }

    #[test]
    fn archive_comparison_is_classified_as_offline() {
        assert!(matches!(
            parse_command(&args(&["compare-archives", "first.json", "second.json"])).unwrap(),
            Command::CompareArchives(_, _)
        ));
    }
}
