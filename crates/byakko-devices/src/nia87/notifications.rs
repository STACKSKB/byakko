//! Selected-keyboard onboard notifications, received without feature polling.
use crate::hid::{self, HidApi, InputDevice, selection::Identity};
use byakko_core::contract::DeviceChange;
use byakko_protocol::nia87::notifications;
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
        self.input.read_timeout(timeout).map(|report| {
            report.and_then(|report| {
                let (report_id, payload) = report.split_first()?;
                notifications::decode(*report_id, payload)
            })
        })
    }
}
