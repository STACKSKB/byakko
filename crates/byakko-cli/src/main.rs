//! Nia87 composition root for the read-only CLI.
use byakko_devices::{
    Executor,
    nia87::{self, BoundNia87Adapter},
};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.len() > 1 {
        return Err("Usage: byakko-cli <devices|describe|read>".into());
    }
    match arguments.first().map(String::as_str) {
        None | Some("--help") => {
            println!(
                "Usage: byakko-cli <devices|describe|read>\n\nread exports the verified USB keymap as JSON; no command writes to the device."
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
        Some("read") => {
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
            let state = byakko_cli::read_keymap(&mut session, &executor, Duration::from_secs(30))?;
            println!("{}", serde_json::to_string_pretty(&state)?);
        }
        _ => return Err("Usage: byakko-cli <devices|describe|read>".into()),
    }
    Ok(())
}
