//! Native composition only; the desktop library never imports Nia87 details.
mod demo;

use byakko_core::session::Session;
use byakko_devices::{
    Executor, KeymapDevice,
    nia87::{self, Nia87Adapter},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let (session, executor) = match arguments.as_slice() {
        [] => {
            let backups = byakko_devices::storage::user_data_dir()?.join("backups");
            (
                Session::new(nia87::descriptor())?
                    .with_macros(nia87::macro_adapter::capabilities())?
                    .with_lighting(nia87::lighting_adapter::capabilities())?
                    .with_picture(nia87::picture_adapter::capabilities())?
                    .with_settings(nia87::settings_adapter::capabilities())?
                    .with_archive(nia87::archive_adapter::capabilities())?,
                Executor::spawn(Nia87Adapter, backups)?,
            )
        }
        [flag] if flag == "--demo" => {
            let device = demo::device()?;
            (
                Session::new(device.descriptor().clone())?
                    .with_macros(
                        device
                            .macro_capabilities()
                            .expect("demo macros configured")
                            .clone(),
                    )?
                    .with_lighting(
                        device
                            .lighting_capabilities()
                            .expect("demo lighting configured")
                            .clone(),
                    )?
                    .with_picture(
                        device
                            .picture_capabilities()
                            .expect("demo picture configured")
                            .clone(),
                    )?
                    .with_settings(
                        device
                            .settings_capabilities()
                            .expect("demo settings configured")
                            .clone(),
                    )?
                    .with_archive(
                        device
                            .archive_capabilities()
                            .expect("demo archive configured"),
                    )?,
                Executor::spawn(device, Default::default())?,
            )
        }
        _ => return Err("Usage: byakko-desktop [--demo]".into()),
    };
    let probe = move || {
        use byakko_desktop::discovery::Availability;
        if arguments.as_slice() == ["--demo"] {
            return Availability::Ready { id: "demo".into() };
        }
        match nia87::device::availability() {
            nia87::device::Availability::Unavailable => Availability::Missing,
            nia87::device::Availability::Available(candidate) => {
                Availability::Ready { id: candidate.path }
            }
            nia87::device::Availability::Ambiguous(candidates) => Availability::Ambiguous {
                count: candidates.len(),
            },
            nia87::device::Availability::EnumerationFailed(reason) => Availability::Error(reason),
        }
    };
    byakko_desktop::run(session, executor, probe)
}
