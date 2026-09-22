//! Replay the independently captured Fn Pause transaction and always restore.
use byakko::device::{self, Result};
use std::{fs::OpenOptions, time::Duration};

fn save(name: &str, value: &device::Snapshot) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(name)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.sync_all()?;
    Ok(())
}

fn send(function: bool, binding: [u8; 4], read_response: bool) -> Result<()> {
    println!("Opening configuration collection for function={function}, binding={binding:?}");
    let (_, device) = device::open_unique()?;
    let mut host = [0; 65];
    host[1..].copy_from_slice(&byakko::protocol::single_key_report(
        function, 0, 91, binding,
    )?);
    device.send_feature_report(&host)?;
    println!("Feature write returned successfully");
    std::thread::sleep(Duration::from_millis(1000));
    if read_response {
        let mut reply = [0; 65];
        let count = device.get_feature_report(&mut reply)?;
        println!(
            "Post-write response: length={count}, prefix={:02x?}",
            &reply[..13]
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    let mode = std::env::args().nth(1);
    let read_response = mode.as_deref() == Some("mixed-response");
    let binding = match mode.as_deref() {
        None => [3, 0, 205, 0],
        Some("f24") => [0, 0, 0x73, 0],
        Some("mixed-response") => [0, 0, 0x73, 0],
        _ => return Err("Expected no argument, f24 or mixed-response".into()),
    };
    let before = device::snapshot()?;
    if before.firmware != 0x100
        || before.profile != 0
        || before.base[91] != [0, 0, 0x48, 0]
        || before.function[91] != [0; 4]
    {
        return Err("Unexpected fixture; no writes sent".into());
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let prefix = format!("Research/captures/fn-replay-{stamp}");
    save(&format!("{prefix}-before.json"), &before)?;
    let result = (|| -> Result<bool> {
        if read_response {
            send(false, [0, 0, 0x72, 0], true)?;
        }
        send(true, binding, read_response)?;
        let actual = device::snapshot()?;
        save(&format!("{prefix}-after.json"), &actual)?;
        let mut desired = before.clone();
        desired.function[91] = binding;
        if read_response {
            desired.base[91] = [0, 0, 0x72, 0];
        }
        Ok(actual == desired)
    })();
    send(true, before.function[91], read_response)?;
    let restored = device::snapshot()?;
    if restored.base[91] != before.base[91] {
        send(false, before.base[91], read_response)?;
    }
    let restored = device::snapshot()?;
    save(&format!("{prefix}-restored.json"), &restored)?;
    if restored != before {
        return Err("Complete restoration mismatch; inspect saved snapshots".into());
    }
    println!(
        "Captured 0x15 replay matched intended maps: {:?}; complete restoration verified",
        result?
    );
    Ok(())
}
