#![cfg_attr(windows, windows_subsystem = "windows")]
use byakko_core::session::Session;
use byakko_devices::{
    Executor,
    nia87::{self, BoundNia87Adapter},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [] => {
            let backups = byakko_devices::storage::user_data_dir()?.join("backups");
            byakko_desktop::run(Session::new(nia87::descriptor())?, move |expected| {
                let candidate = match nia87::device::availability() {
                    nia87::device::Availability::Available(candidate) => candidate,
                    nia87::device::Availability::Unavailable => {
                        return Err("Connect a supported keyboard, then select Read.".into());
                    }
                    nia87::device::Availability::Ambiguous(_) => {
                        return Err("Connect one supported keyboard at a time.".into());
                    }
                    nia87::device::Availability::EnumerationFailed(reason) => return Err(reason),
                };
                let id = candidate.identity().discovery_id();
                if expected.is_some_and(|expected| expected != id) {
                    return Err(
                        "The selected keyboard is unavailable. Reconnect that keyboard.".into(),
                    );
                }
                let target = nia87::device::Target::from_candidate(&candidate)
                    .map_err(|error| error.to_string())?;
                let worker = Executor::spawn(BoundNia87Adapter::new(target), backups.clone())
                    .map_err(|error| error.to_string())?;
                Ok((id, worker))
            })?;
        }
        [flag] if flag == "--demo" => {
            byakko_desktop::run(
                Session::new(byakko_devices::memory::demo()?.descriptor().clone())?,
                |_| {
                    let worker =
                        Executor::spawn(byakko_devices::memory::demo()?, Default::default())
                            .map_err(|error| error.to_string())?;
                    Ok(("demo".into(), worker))
                },
            )?;
        }
        _ => return Err("Usage: byakko-desktop [--demo]".into()),
    }
    Ok(())
}
