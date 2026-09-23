//! Native composition only; the desktop library never imports Nia87 details.
mod demo;

use byakko_core::session::Session;
use byakko_desktop::discovery::Availability;
use byakko_devices::{
    Executor, KeymapDevice,
    nia87::{self, BoundNia87Adapter},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    type Probe = Box<dyn Fn() -> Availability + Send>;
    type Attach = Box<dyn Fn(&str) -> Result<Executor, String>>;
    let (session, probe, attach, labels_directory): (
        Session,
        Probe,
        Attach,
        Option<std::path::PathBuf>,
    ) = match arguments.as_slice() {
        [] => {
            let data = byakko_devices::storage::user_data_dir()?;
            let backups = data.join("backups");
            (
                nia87::application::session()?,
                Box::new(|| match nia87::device::availability() {
                    nia87::device::Availability::Unavailable => Availability::Missing,
                    nia87::device::Availability::Available(candidate) => {
                        Availability::Ready { id: candidate.path }
                    }
                    nia87::device::Availability::Ambiguous(candidates) => Availability::Ambiguous {
                        count: candidates.len(),
                    },
                    nia87::device::Availability::EnumerationFailed(reason) => {
                        Availability::Error(reason)
                    }
                }),
                Box::new(move |id| {
                    let candidate = match nia87::device::availability() {
                        nia87::device::Availability::Available(candidate)
                            if candidate.path == id =>
                        {
                            candidate
                        }
                        _ => return Err("Keyboard identity changed before attachment".into()),
                    };
                    let target = nia87::device::Target::from_candidate(&candidate)
                        .map_err(|error| error.to_string())?;
                    Executor::spawn(BoundNia87Adapter::new(target), backups.clone())
                        .map_err(|error| error.to_string())
                }),
                Some(data.join("macro-labels").join("nia87")),
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
                Box::new(|| Availability::Ready { id: "demo".into() }),
                Box::new(|_id| {
                    let device = demo::device()?;
                    Executor::spawn(device, Default::default()).map_err(|e| e.to_string())
                }),
                None,
            )
        }
        _ => return Err("Usage: byakko-desktop [--demo]".into()),
    };
    byakko_desktop::run(session, probe, attach, labels_directory)
}
