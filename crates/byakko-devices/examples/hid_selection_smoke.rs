//! Read-only smoke check of one HID selection path on unrelated hardware.
//! No collection is opened and no feature or input reports are sent.

use byakko_devices::hid::{
    HidApi,
    selection::{SelectionError, Target},
};

fn inspect(
    label: &str,
    identities: &[byakko_devices::hid::selection::Identity],
) -> Result<(), String> {
    let Some(first) = identities.first() else {
        return Err(format!("No {label} HID collection found"));
    };
    let target = Target::from_identity(first.clone());
    target
        .select(std::slice::from_ref(first))
        .map_err(|error| error.to_string())?;
    if identities.len() > 1
        && target.select(identities) != Err(SelectionError::Ambiguous(identities.len()))
    {
        return Err(format!(
            "{label} selection accepted an ambiguous collection set"
        ));
    }
    println!(
        "{label}: {} collection(s); selected {:04x}:{:04x} interface={} usage={:04x}:{:04x}",
        identities.len(),
        first.vendor_id,
        first.product_id,
        first.interface,
        first.usage_page,
        first.usage
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let api = HidApi::new()?;
    let mut wacom = Vec::new();
    let mut nia87 = Vec::new();
    for device in api.device_list() {
        let identity = device.identity();
        let named_wacom = [device.manufacturer_string(), device.product_string()]
            .into_iter()
            .flatten()
            .any(|name| name.to_ascii_lowercase().contains("wacom"));
        if identity.vendor_id == 0x056a || named_wacom {
            wacom.push(identity);
        } else if identity.vendor_id == 0x3151 {
            nia87.push(identity);
        }
    }
    inspect("Wacom", &wacom)?;
    inspect("Nia OEM", &nia87)?;
    Ok(())
}
