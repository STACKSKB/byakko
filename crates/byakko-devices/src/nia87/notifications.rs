//! Selected-keyboard onboard notifications, received without feature polling.
use crate::hid::{self, HidApi, InputDevice, selection::Identity};
use byakko_core::contract::DeviceChange;
use std::time::Duration;

pub struct Listener {
    input: InputDevice,
}

impl Listener {
    /// Select the sibling input collection by its physical USB ancestor.
    /// Never infer a sibling path or choose an arbitrary matching keyboard.
    pub fn open(selected: &Identity) -> hid::Result<Self> {
        if selected.vendor_id != 0x3151
            || !matches!(selected.product_id, 0x4011 | 0x4015)
            || selected.usage_page != 0xffff
            || selected.usage != 2
        {
            return Err(
                "Notification listener requires a selected Nia87 configuration collection".into(),
            );
        }
        let inventory = HidApi::new()?;
        let selected_device = inventory
            .device_list()
            .find(|device| device.identity() == *selected)
            .ok_or("The selected keyboard is disconnected")?;
        let physical = selected_device
            .physical_device
            .as_deref()
            .ok_or("Could not identify the selected keyboard's USB parent")?;
        let mut siblings = inventory.device_list().filter(|device| {
            device.physical_device.as_deref() == Some(physical)
                && device.vendor_id() == selected.vendor_id
                && device.product_id() == selected.product_id
                && device.interface_number() == 1
                && device.usage_page() == 0xffff
                && device.usage() == 1
        });
        let sibling = siblings
            .next()
            .ok_or("This keyboard's notification collection is unavailable")?;
        if siblings.next().is_some() {
            return Err("This keyboard has ambiguous notification collections".into());
        }
        Ok(Self {
            input: InputDevice::open(sibling.path())?,
        })
    }

    /// Wait at most 100 ms, allowing the owner to stop or switch connections.
    /// This receives an outstanding input report; it does not query firmware.
    pub fn read_timeout(&mut self, timeout: Duration) -> hid::Result<Option<DeviceChange>> {
        self.input
            .read_timeout(timeout)
            .map(|report| report.and_then(|report| decode(&report)))
    }
}

fn decode(report: &[u8]) -> Option<DeviceChange> {
    if report.len() != 4 || report[0] != 5 {
        return None;
    }
    match report[1..4] {
        [4..=7, _, _] => Some(DeviceChange::Lighting),
        // Include the observed Windows-lock mask omitted by the vendor UI.
        [3, _, 0 | 1 | 2 | 4 | 9] | [2, _, 0] | [19, 0, 0] => Some(DeviceChange::Settings),
        [1, _, _] | [3, _, 8] | [13, 0, 0] => Some(DeviceChange::Configuration),
        // Start/stop (15), unknown commands and unrelated reports carry no edit.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report(bytes: [u8; 4]) -> [u8; 4] {
        let mut report = [0; 4];
        report[..4].copy_from_slice(&bytes);
        report
    }
    #[test]
    fn captured_onboard_notifications_route_only_affected_features() {
        for bytes in [[5, 4, 3, 0], [5, 5, 2, 0], [5, 6, 3, 0], [5, 7, 1, 0]] {
            assert_eq!(decode(&report(bytes)), Some(DeviceChange::Lighting));
        }
        for bytes in [[5, 3, 1, 1], [5, 3, 0, 1], [5, 3, 1, 9], [5, 3, 2, 2]] {
            assert_eq!(decode(&report(bytes)), Some(DeviceChange::Settings));
        }
        assert_eq!(
            decode(&report([5, 13, 0, 0])),
            Some(DeviceChange::Configuration)
        );
        assert_eq!(
            decode(&report([5, 1, 2, 0])),
            Some(DeviceChange::Configuration)
        );
        assert_eq!(
            decode(&report([5, 3, 1, 8])),
            Some(DeviceChange::Configuration)
        );
        assert_eq!(decode(&report([5, 15, 1, 0])), None);
        assert_eq!(decode(&report([5, 15, 0, 0])), None);
        assert_eq!(decode(&report([5, 255, 1, 0])), None);
        assert_eq!(decode(&report([4, 4, 1, 0])), None);
        assert_eq!(decode(&[5, 4, 1]), None);
    }
}
