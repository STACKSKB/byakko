//! Read-only research probe for the official configurator's macro slot boundary.
//! This does not advertise slot 50 or send a macro setter.
use byakko_devices::{
    hid::HidDevice,
    nia87::{device, protocol},
    rongyuan::yc500::{macro_io, macro_program},
};
use std::{fs::OpenOptions, time::Duration};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn exchange(device: &HidDevice, opcode: u8, index: u8, page: u8) -> Result<[u8; 64]> {
    let mut request = [0u8; 65];
    request[1..].copy_from_slice(&protocol::read_request(opcode, index, page));
    device.send_feature_report(&request)?;
    std::thread::sleep(Duration::from_millis(30));
    let mut reply = [0u8; 65];
    let count = device.get_feature_report(&mut reply)?;
    if count != reply.len() || reply[0] != 0 {
        return Err(format!("Unexpected HID response to {opcode:02x}: length {count}").into());
    }
    Ok(reply[1..].try_into()?)
}

fn complete(device: &HidDevice, slot: u8) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(256);
    for page in 0..4 {
        let barrier = exchange(device, 0x80, 0, 0)?;
        if barrier[0] != 0x80 {
            return Err("Macro read identity barrier failed".into());
        }
        let data = exchange(device, 0x8b, slot, page)?;
        if data == barrier {
            return Err("Macro read returned stale identity data".into());
        }
        bytes.extend_from_slice(&data);
    }
    Ok(bytes)
}

fn main() -> Result<()> {
    let maps_before = device::snapshot()?;
    if maps_before.firmware != 0x0100 || maps_before.profile != 0 {
        return Err("Unverified Nia87 firmware or profile".into());
    }
    // The product uses the same OS-held lock. A research probe must not race
    // its worker even though it sends only documented read requests.
    let lock_path = std::env::temp_dir().join("byakko-nia87-configuration.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    lock.try_lock()?;
    let (candidate, hid) = device::open_unique()?;
    let mut captures = Vec::new();
    for slot in [0, 49, 50] {
        let bytes = macro_io::stable_reads(|| complete(&hid, slot))?;
        let nonzero = bytes
            .iter()
            .enumerate()
            .filter_map(|(offset, byte)| (*byte != 0).then_some((offset, *byte)))
            .collect::<Vec<_>>();
        println!(
            "slot {slot}: nonzero={nonzero:02x?} prefix={:02x?}",
            &bytes[..16],
        );
        println!(
            "slot {slot}: program={}",
            macro_program::decode(&bytes)
                .map_or_else(|error| format!("invalid ({error})"), |_| "valid".into())
        );
        captures.push(bytes);
    }
    println!("collection={}", candidate.path);
    println!("slot50_eq_slot0={}", captures[2] == captures[0]);
    println!("slot50_eq_slot49={}", captures[2] == captures[1]);
    drop(hid);
    drop(lock);
    if device::snapshot()? != maps_before {
        return Err("Keymap changed across read-only macro probe".into());
    }
    Ok(())
}
