use byakko_core::{
    model::{keymap::State, macros::Snapshot},
    session::Session,
};
use byakko_devices::{
    Executor,
    nia87::{self, BoundNia87Adapter},
};
use std::{path::PathBuf, time::Duration};

const USAGE: &str = "byakko-cli [--demo] <devices|describe|read|plan-keymap FILE|apply-keymap FILE|list-macros|read-macro SLOT|plan-macro FILE|apply-macro FILE|assign-macro SLOT LAYER KEY BINDING>";

#[derive(Debug, PartialEq)]
enum Command {
    Help,
    Devices,
    Describe,
    Read,
    Keymap {
        apply: bool,
        path: PathBuf,
    },
    ListMacros,
    ReadMacro(String),
    Macro {
        apply: bool,
        path: PathBuf,
    },
    AssignMacro {
        slot: String,
        layer: String,
        key: String,
        binding: String,
    },
}

fn parse(arguments: &[String]) -> Result<(bool, Command), String> {
    let (demo, arguments) = match arguments {
        [flag, rest @ ..] if flag == "--demo" => (true, rest),
        _ => (false, arguments),
    };
    let command = match arguments {
        [] => Command::Help,
        [name] if name == "--help" => Command::Help,
        [name] if name == "devices" => Command::Devices,
        [name] if name == "describe" => Command::Describe,
        [name] if name == "read" => Command::Read,
        [name] if name == "list-macros" => Command::ListMacros,
        [name, slot] if name == "read-macro" => Command::ReadMacro(slot.clone()),
        [name, path] if name == "plan-macro" || name == "apply-macro" => Command::Macro {
            apply: name == "apply-macro",
            path: path.into(),
        },
        [name, slot, layer, key, binding] if name == "assign-macro" => Command::AssignMacro {
            slot: slot.clone(),
            layer: layer.clone(),
            key: key.clone(),
            binding: binding.clone(),
        },
        [name, path] if name == "plan-keymap" || name == "apply-keymap" => Command::Keymap {
            apply: name == "apply-keymap",
            path: path.into(),
        },
        _ => return Err(format!("Usage: {USAGE}")),
    };
    Ok((demo, command))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (demo, command) = parse(&std::env::args().skip(1).collect::<Vec<_>>())?;
    if command == Command::Help {
        println!(
            "Usage: {USAGE}\n\nRead returns JSON. Edit its bindings while retaining its revision, then plan-keymap and apply-keymap. Apply backs up and verifies the write. --demo never opens hardware. Other features are under reconstruction on this branch."
        );
        return Ok(());
    }
    if command == Command::Devices {
        if demo {
            println!("Demo keyboard (memory)");
        } else {
            println!("{:?}", nia87::device::availability());
        }
        return Ok(());
    }
    let memory = demo.then(byakko_devices::memory::demo).transpose()?;
    let descriptor = memory
        .as_ref()
        .map_or_else(nia87::descriptor, |device| device.descriptor().clone());
    if command == Command::Describe {
        println!("{}", serde_json::to_string_pretty(&descriptor)?);
        return Ok(());
    }
    // Load only the associated file, before opening a device.
    enum FileInput {
        Keymap(State),
        Macro(Snapshot),
    }
    let target = match &command {
        Command::Keymap { path, .. } => Some(FileInput::Keymap(byakko_cli::read_json(
            std::fs::File::open(path)?,
            1024 * 1024,
        )?)),
        Command::Macro { path, .. } => Some(FileInput::Macro(byakko_cli::read_json(
            std::fs::File::open(path)?,
            1024 * 1024,
        )?)),
        _ => None,
    };
    let mut session = if let Some(device) = &memory {
        Session::new(descriptor)?.with_macros(
            device
                .macro_capabilities()
                .expect("demo supports macros")
                .clone(),
        )?
    } else {
        nia87::application::session()?
    };
    let executor = if let Some(device) = memory {
        Executor::spawn(device, PathBuf::new())?
    } else {
        let candidate = match nia87::device::availability() {
            nia87::device::Availability::Available(candidate) => candidate,
            nia87::device::Availability::Unavailable => return Err("Nia87 not connected".into()),
            nia87::device::Availability::Ambiguous(_) => {
                return Err("Connect one supported keyboard at a time".into());
            }
            nia87::device::Availability::EnumerationFailed(reason) => return Err(reason.into()),
        };
        let target = nia87::device::Target::from_candidate(&candidate)?;
        Executor::spawn(
            BoundNia87Adapter::new(target),
            byakko_devices::storage::user_data_dir()?.join("backups"),
        )?
    };
    let timeout = Duration::from_secs(30);
    match command {
        Command::Read => println!(
            "{}",
            serde_json::to_string_pretty(&byakko_cli::read_keymap(
                &mut session,
                &executor,
                timeout
            )?)?
        ),
        Command::Keymap { apply, .. } => {
            let Some(FileInput::Keymap(target)) = target else {
                unreachable!("keymap command loaded its file")
            };
            byakko_cli::read_keymap(&mut session, &executor, timeout)?;
            if apply {
                let actual = byakko_cli::apply_keymap(&mut session, &executor, &target)?;
                println!("{}", serde_json::to_string_pretty(&actual)?);
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&byakko_cli::plan_keymap(&session, &target)?)?
                );
            }
        }
        Command::ListMacros => {
            byakko_cli::list_macros(&mut session, &executor, timeout)?;
            for (slot, occupancy) in session
                .macro_library()
                .expect("configured macro feature")
                .slots()
            {
                println!("{slot}: {occupancy:?}");
            }
        }
        Command::ReadMacro(slot) => println!(
            "{}",
            serde_json::to_string_pretty(&byakko_cli::read_macro(
                &mut session,
                &executor,
                &slot,
                timeout
            )?)?
        ),
        Command::Macro { apply, .. } => {
            let Some(FileInput::Macro(target)) = target else {
                unreachable!("macro command loaded its file")
            };
            byakko_cli::read_macro(&mut session, &executor, &target.slot, timeout)?;
            if apply {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&byakko_cli::apply_macro(
                        &mut session,
                        &executor,
                        &target
                    )?)?
                );
            } else {
                session.stage_macro_snapshot(&target)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &session.macros().expect("configured macro feature").draft()
                    )?
                );
            }
        }
        Command::AssignMacro {
            slot,
            layer,
            key,
            binding,
        } => {
            byakko_cli::read_keymap(&mut session, &executor, timeout)?;
            byakko_cli::read_macro(&mut session, &executor, &slot, timeout)?;
            byakko_cli::assign_macro(&mut session, &executor, &layer, &key, &binding)?;
            println!("Macro assigned.");
        }
        _ => unreachable!("offline commands return before device access"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parsing_is_closed_and_does_not_load_files() {
        let args = ["--demo", "plan-keymap", "not-yet-created.json"].map(str::to_owned);
        assert_eq!(
            parse(&args).unwrap(),
            (
                true,
                Command::Keymap {
                    apply: false,
                    path: "not-yet-created.json".into()
                }
            )
        );
        assert!(parse(&["read".into(), "extra".into()]).is_err());
        assert!(parse(&["restore-archive".into()]).is_err());
    }
}
