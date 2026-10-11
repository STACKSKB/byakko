// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only USB descriptor metadata tied to an explicit OS input device.
use crate::tester_input::InputDevice;
use std::fmt;
#[cfg(target_os = "windows")]
#[path = "tester_usb/windows.rs"]
mod platform;
#[cfg(target_os = "linux")]
#[path = "tester_usb/linux.rs"]
mod platform;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Speed {
    Low,
    Full,
    High,
    Super,
}
impl fmt::Display for Speed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Low => "low speed",
            Self::Full => "full speed",
            Self::High => "high speed",
            Self::Super => "SuperSpeed",
        })
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Endpoint {
    pub interface: u8,
    pub alternate: u8,
    pub address: u8,
    pub b_interval: u8,
    pub interval_us: u64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Metadata {
    pub source: String,
    pub speed: Speed,
    pub endpoints: Vec<Endpoint>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unavailable(pub String);
impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Unavailable {}
pub fn inspect(device: &InputDevice) -> Result<Metadata, Unavailable> {
    platform::inspect(device)
}

/// USB 2.0 9.6.6 and USB 3.x interrupt endpoint bInterval interpretation.
fn interval_us(speed: Speed, value: u8) -> Result<u64, Unavailable> {
    match (speed, value) {
        (Speed::Low | Speed::Full, 1..=255) => Ok(u64::from(value) * 1_000),
        (Speed::High | Speed::Super, 1..=16) => Ok(125 * (1_u64 << (value - 1))),
        _ => Err(Unavailable("Invalid interrupt endpoint bInterval".into())),
    }
}

/// Select only one active configuration/interface/alternate, preserving each
/// interrupt IN endpoint rather than assuming the first endpoint is a keyboard.
pub(crate) fn interrupt_endpoints(
    bytes: &[u8],
    configuration: u8,
    interface: u8,
    alternate: u8,
    speed: Speed,
) -> Result<Vec<Endpoint>, Unavailable> {
    let invalid = || Unavailable("Incomplete or inconsistent USB descriptors".into());
    let mut at = 0;
    let mut selected = None;
    while at < bytes.len() {
        let header = bytes.get(at..at + 2).ok_or_else(invalid)?;
        let length = usize::from(header[0]);
        if length < 2 {
            return Err(invalid());
        }
        let descriptor = bytes.get(at..at + length).ok_or_else(invalid)?;
        if header[1] == 2 {
            if length < 9 {
                return Err(invalid());
            }
            let total = usize::from(u16::from_le_bytes([descriptor[2], descriptor[3]]));
            if total < length {
                return Err(invalid());
            }
            let config = bytes.get(at..at + total).ok_or_else(invalid)?;
            if descriptor[5] == configuration && selected.replace(config).is_some() {
                return Err(invalid());
            }
            at += total;
        } else {
            at += length;
        }
    }
    let config = selected
        .ok_or_else(|| Unavailable("Active USB configuration descriptor unavailable".into()))?;
    let mut at = usize::from(config[0]);
    let mut target = false;
    let mut expected = None;
    let mut seen = 0_usize;
    let mut endpoints = Vec::new();
    while at < config.len() {
        let header = config.get(at..at + 2).ok_or_else(invalid)?;
        let length = usize::from(header[0]);
        if length < 2 {
            return Err(invalid());
        }
        let descriptor = config.get(at..at + length).ok_or_else(invalid)?;
        match header[1] {
            4 => {
                if length < 9 {
                    return Err(invalid());
                }
                target = descriptor[2] == interface && descriptor[3] == alternate;
                if target {
                    if expected.is_some() || descriptor[5] != 3 {
                        return Err(invalid());
                    }
                    expected = Some(usize::from(descriptor[4]));
                }
            }
            5 if target => {
                if length < 7 {
                    return Err(invalid());
                }
                seen += 1;
                let address = descriptor[2];
                if address & 0x70 != 0 || address & 0x0f == 0 {
                    return Err(invalid());
                }
                if address & 0x80 != 0 && address & 0x0f != 0 && descriptor[3] & 3 == 3 {
                    if endpoints.iter().any(|ep: &Endpoint| ep.address == address) {
                        return Err(invalid());
                    }
                    endpoints.push(Endpoint {
                        interface,
                        alternate,
                        address,
                        b_interval: descriptor[6],
                        interval_us: interval_us(speed, descriptor[6])?,
                    });
                }
            }
            _ => {}
        }
        at += length;
    }
    if expected != Some(seen) || endpoints.is_empty() {
        return Err(Unavailable(
            "Selected HID interface has no inspectable interrupt IN endpoint".into(),
        ));
    }
    Ok(endpoints)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(value: u8, iface: u8, alt: u8, interval: u8) -> Vec<u8> {
        vec![
            9, 2, 25, 0, 1, value, 0, 0x80, 50, 9, 4, iface, alt, 1, 3, 1, 1, 0, 7, 5, 0x81, 3, 8,
            0, interval,
        ]
    }
    #[test]
    fn interval_uses_bus_speed_without_inferring_observed_polling() {
        assert_eq!(interval_us(Speed::Low, 10).unwrap(), 10_000);
        assert_eq!(interval_us(Speed::Full, 1).unwrap(), 1_000);
        assert_eq!(interval_us(Speed::High, 4).unwrap(), 1_000);
        assert_eq!(interval_us(Speed::Super, 1).unwrap(), 125);
        assert!(interval_us(Speed::Full, 0).is_err());
        assert!(interval_us(Speed::High, 17).is_err());
    }
    #[test]
    fn selects_active_configuration_interface_and_alternate_only() {
        let mut bytes = config(1, 0, 0, 8);
        bytes.extend(config(2, 3, 1, 4));
        let endpoints = interrupt_endpoints(&bytes, 2, 3, 1, Speed::High).unwrap();
        assert_eq!(
            endpoints,
            vec![Endpoint {
                interface: 3,
                alternate: 1,
                address: 0x81,
                b_interval: 4,
                interval_us: 1_000
            }]
        );
        assert!(interrupt_endpoints(&bytes, 2, 0, 0, Speed::High).is_err());
        assert!(interrupt_endpoints(&bytes, 2, 3, 0, Speed::High).is_err());
        bytes.pop();
        assert!(interrupt_endpoints(&bytes, 2, 3, 1, Speed::High).is_err());
    }
    #[test]
    fn rejects_missing_endpoints_invalid_lengths_and_out_only_interfaces() {
        for (index, value) in [(18, 0), (20, 1), (20, 0xf1), (21, 2), (24, 0), (13, 2)] {
            let mut bytes = config(1, 0, 0, 1);
            bytes[index] = value;
            assert!(
                interrupt_endpoints(&bytes, 1, 0, 0, Speed::Full).is_err(),
                "field {index}"
            );
        }
    }
}
