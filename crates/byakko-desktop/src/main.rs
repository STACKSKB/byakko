#![cfg_attr(windows, windows_subsystem = "windows")]
use byakko_desktop::{Availability, Discovery, Endpoint};
use byakko_devices::{
    Executor,
    nia87::{self, BoundNia87Adapter},
};

fn endpoint(candidate: &nia87::device::Candidate) -> Endpoint {
    Endpoint {
        id: candidate.identity().discovery_id(),
        label: match candidate.pid {
            0x4015 => "Wired USB",
            0x4011 => "2.4 GHz receiver",
            _ => "Keyboard",
        },
    }
}

fn inventory(availability: nia87::device::Availability) -> Availability {
    use nia87::device::Availability as Native;
    match availability {
        Native::Unavailable => Availability::Missing,
        Native::Available(candidate) => Availability::Ready(endpoint(&candidate)),
        Native::Ambiguous(candidates) => match nia87::device::transport_choices(&candidates) {
            Some((wired, receiver)) => Availability::Choices {
                wired: endpoint(wired),
                receiver: endpoint(receiver),
            },
            None => Availability::Ambiguous {
                count: candidates.len(),
            },
        },
        Native::EnumerationFailed(reason) => Availability::Error(reason),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = byakko_desktop::config::Config::from_environment()?;
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [] => {
            let data = byakko_devices::storage::user_data_dir()?;
            let backups = data.join("backups");
            config.appearance = byakko_devices::storage::appearance::load(&data);
            config.data_directory = Some(data);
            let discovery = Discovery::spawn(|| inventory(nia87::device::availability()))?;
            byakko_desktop::run(
                nia87::application::session()?,
                config,
                discovery,
                move |expected| {
                    let availability = nia87::device::availability();
                    let candidate = nia87::device::select_candidate(&availability, expected)
                        .map_err(|error| error.to_string())?;
                    let id = candidate.identity().discovery_id();
                    let target = nia87::device::Target::from_candidate(candidate)
                        .map_err(|error| error.to_string())?;
                    let mut listener = nia87::notifications::Listener::open(&candidate.identity())
                        .map_err(|error| error.to_string());
                    let adapter = BoundNia87Adapter::new(target);
                    let worker = Executor::spawn(adapter, backups.clone())
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
            let discovery = Discovery::spawn(|| {
                Availability::Ready(Endpoint {
                    id: "demo".into(),
                    label: "Demo",
                })
            })?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use nia87::device::{Availability as Native, Candidate};

    #[test]
    fn desktop_choices_label_the_exact_endpoints_in_either_order() {
        let wired = Candidate {
            path: "usb-path".into(),
            physical_device: Some("usb-parent".into()),
            vid: 0x3151,
            pid: 0x4015,
            interface: 2,
            usage_page: 0xffff,
            usage: 2,
            manufacturer: None,
            product: None,
        };
        let receiver = Candidate {
            path: "receiver-path".into(),
            physical_device: Some("receiver-parent".into()),
            pid: 0x4011,
            ..wired.clone()
        };
        let usb = Endpoint {
            id: wired.identity().discovery_id(),
            label: "Wired USB",
        };
        let radio = Endpoint {
            id: receiver.identity().discovery_id(),
            label: "2.4 GHz receiver",
        };
        assert_eq!(
            inventory(Native::Available(wired.clone())),
            Availability::Ready(usb.clone())
        );
        assert_eq!(
            inventory(Native::Available(receiver.clone())),
            Availability::Ready(radio.clone())
        );
        for candidates in [vec![wired.clone(), receiver.clone()], vec![receiver, wired]] {
            assert_eq!(
                inventory(Native::Ambiguous(candidates)),
                Availability::Choices {
                    wired: usb.clone(),
                    receiver: radio.clone()
                }
            );
        }
    }
}
