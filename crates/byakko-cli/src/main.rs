//! Nia87 composition root for the read-only CLI.
use byakko_devices::{
    Executor,
    nia87::{self, BoundNia87Adapter},
};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const USAGE: &str = "Usage: byakko-cli <devices|describe|read|read-colors|read-lighting|read-settings|read-macro <slot-id>>";
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let macro_read = arguments
        .first()
        .is_some_and(|command| command == "read-macro");
    if arguments.len() > 2
        || (arguments.len() == 2 && !macro_read)
        || (macro_read && arguments.len() != 2)
    {
        return Err(USAGE.into());
    }
    match arguments.first().map(String::as_str) {
        None | Some("--help") => {
            println!(
                "{USAGE}\n\nRead commands export verified USB state as JSON; no command writes to the device."
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
        Some("read" | "read-colors" | "read-lighting" | "read-settings" | "read-macro") => {
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
            let executor = Executor::spawn(BoundNia87Adapter::new(target), backups)?;
            let mut session = nia87::application::session()?;
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
                _ => unreachable!("read command matched above"),
            };
            println!("{json}");
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}
