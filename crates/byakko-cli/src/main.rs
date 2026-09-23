//! Nia87 composition root for the read-only CLI.
use byakko_devices::{
    Executor,
    nia87::{self, BoundNia87Adapter},
};
use std::io::Write;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const USAGE: &str = "Usage: byakko-cli <devices|describe|read|read-colors|read-lighting|read-settings|read-macro <slot-id>|capture-archive <new-file>|review-archive <file>>";
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let macro_read = arguments
        .first()
        .is_some_and(|command| command == "read-macro");
    let archive_capture = arguments
        .first()
        .is_some_and(|command| command == "capture-archive");
    let archive_review = arguments
        .first()
        .is_some_and(|command| command == "review-archive");
    if arguments.len() > 2
        || (arguments.len() == 2 && !(macro_read || archive_capture || archive_review))
        || ((macro_read || archive_capture || archive_review) && arguments.len() != 2)
    {
        return Err(USAGE.into());
    }
    if archive_capture && std::path::Path::new(&arguments[1]).exists() {
        return Err("Archive output already exists; choose a new file".into());
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
        Some(
            "read" | "read-colors" | "read-lighting" | "read-settings" | "read-macro"
            | "capture-archive" | "review-archive",
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
            let executor = Executor::spawn(BoundNia87Adapter::new(target), backups)?;
            let mut session = nia87::application::session()?;
            let mut review_target = if archive_review {
                let max_bytes = session
                    .archive_capabilities()
                    .ok_or("Nia87 has no native archive capability")?
                    .max_bytes;
                let limit = u64::from(max_bytes).saturating_mul(4).saturating_add(1024);
                if std::fs::metadata(&arguments[1])?.len() > limit {
                    return Err("Archive input exceeds the supported JSON file size".into());
                }
                Some(
                    serde_json::from_slice::<byakko_core::archive::NativeArchive>(&std::fs::read(
                        &arguments[1],
                    )?)?,
                )
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
                _ => unreachable!("read command matched above"),
            };
            println!("{json}");
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}
