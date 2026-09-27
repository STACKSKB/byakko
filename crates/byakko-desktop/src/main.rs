#![cfg_attr(windows, windows_subsystem = "windows")]
use byakko_desktop::{Availability, Discovery};
use byakko_devices::{
    Executor,
    nia87::{self, BoundNia87Adapter},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = byakko_desktop::config::Config::from_environment()?;
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [] => {
            let data = byakko_devices::storage::user_data_dir()?;
            let backups = data.join("backups");
            config.data_directory = Some(data);
            let discovery = Discovery::spawn(|| match nia87::device::availability() {
                nia87::device::Availability::Available(candidate) => Availability::Ready {
                    id: candidate.identity().discovery_id(),
                },
                nia87::device::Availability::Unavailable => Availability::Missing,
                nia87::device::Availability::Ambiguous(candidates) => Availability::Ambiguous {
                    count: candidates.len(),
                },
                nia87::device::Availability::EnumerationFailed(reason) => {
                    Availability::Error(reason)
                }
            })?;
            byakko_desktop::run(
                nia87::application::session()?,
                config,
                discovery,
                move |expected| {
                    let candidate = match nia87::device::availability() {
                        nia87::device::Availability::Available(candidate) => candidate,
                        nia87::device::Availability::Unavailable => {
                            return Err(
                                "Connect the keyboard by USB, then choose Read / reconnect.".into(),
                            );
                        }
                        nia87::device::Availability::Ambiguous(_) => {
                            return Err("Connect one supported keyboard at a time.".into());
                        }
                        nia87::device::Availability::EnumerationFailed(reason) => {
                            return Err(reason);
                        }
                    };
                    let id = candidate.identity().discovery_id();
                    if expected.is_some_and(|expected| expected != id) {
                        return Err(
                            "The selected keyboard is unavailable. Reconnect that keyboard.".into(),
                        );
                    }
                    let target = nia87::device::Target::from_candidate(&candidate)
                        .map_err(|error| error.to_string())?;
                    let mut listener = nia87::notifications::Listener::open(&candidate.identity())
                        .map_err(|error| error.to_string());
                    let worker = Executor::spawn(BoundNia87Adapter::new(target), backups.clone())
                        .and_then(|worker| {
                            worker.with_notifications(move |timeout| match &mut listener {
                                Ok(listener) => listener
                                    .read_timeout(timeout)
                                    .map_err(|error| error.to_string()),
                                Err(reason) => Err(reason.clone()),
                            })
                        })
                        .map_err(|error| error.to_string())?;
                    Ok((id, worker))
                },
            )?;
        }
        [flag] if flag == "--demo" => {
            let device = byakko_devices::memory::demo()?;
            let discovery = Discovery::spawn(|| Availability::Ready { id: "demo".into() })?;
            byakko_desktop::run(device.session()?, config, discovery, |_| {
                let worker = Executor::spawn(byakko_devices::memory::demo()?, Default::default())
                    .map_err(|error| error.to_string())?;
                Ok(("demo".into(), worker))
            })?;
        }
        _ => return Err("Usage: byakko-desktop [--demo]".into()),
    }
    Ok(())
}
