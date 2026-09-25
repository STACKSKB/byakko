//! Native composition only; the desktop library never imports Nia87 details.
#![cfg_attr(windows, windows_subsystem = "windows")]
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
    type Attach = Box<dyn Fn(Option<&str>) -> Result<(String, Executor), String>>;
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
                    nia87::device::Availability::Available(candidate) => Availability::Ready {
                        id: candidate.identity().discovery_id(),
                    },
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
                            if id.is_none_or(|id| candidate.identity().discovery_id() == id) =>
                        {
                            candidate
                        }
                        nia87::device::Availability::Available(_) => {
                            return Err("Keyboard identity changed before attachment".into());
                        }
                        nia87::device::Availability::Unavailable => {
                            return Err("No supported keyboard is connected".into());
                        }
                        nia87::device::Availability::Ambiguous(candidates) => {
                            return Err(format!(
                                "Found {} matching keyboards; connect one keyboard",
                                candidates.len()
                            ));
                        }
                        nia87::device::Availability::EnumerationFailed(reason) => {
                            return Err(format!("Could not enumerate keyboards: {reason}"));
                        }
                    };
                    let target = nia87::device::Target::from_candidate(&candidate)
                        .map_err(|error| error.to_string())?;
                    let executor = Executor::spawn(BoundNia87Adapter::new(target), backups.clone())
                        .map_err(|error| error.to_string())?;
                    Ok((candidate.identity().discovery_id(), executor))
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
                    Executor::spawn(device, Default::default())
                        .map(|executor| ("demo".into(), executor))
                        .map_err(|e| e.to_string())
                }),
                None,
            )
        }
        _ => return Err("Usage: byakko-desktop [--demo]".into()),
    };
    byakko_desktop::run(
        session,
        probe,
        attach,
        labels_directory,
        byakko_desktop::config::Config::from_environment()?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_identity_includes_metadata_even_when_path_is_reused() {
        let candidate = nia87::device::Candidate {
            path: "/dev/hidraw3".into(),
            vid: 0x3151,
            pid: 0x4015,
            interface: 2,
            usage_page: 0xffff,
            usage: 2,
            manufacturer: None,
            product: None,
        };
        let original = candidate.identity().discovery_id();
        let mut renamed = candidate;
        renamed.product = Some("Localized product name".into());
        assert_eq!(original, renamed.identity().discovery_id());
    }
}
