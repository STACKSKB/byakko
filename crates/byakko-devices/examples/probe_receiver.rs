//! Bounded read-only receiver timing probe. No configuration setters are sent.
use byakko_devices::{hid::HidApi, nia87::device};
use byakko_protocol::nia87::protocol;
use std::{ffi::CString, fs::OpenOptions, time::Duration};

fn main() -> device::Result<()> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let (output, opcodes): (&String, &[u8]) = match arguments.as_slice() {
        [output] => (output, &[0x80, 0x85]),
        [flag, output] if flag == "--status" => (output, &[0xf7]),
        _ => return Err("Usage: probe_receiver [--status] NEW_CAPTURE.json".into()),
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let candidates = device::candidates()?;
    let [candidate] = candidates.as_slice() else {
        return Err("Connect exactly one Nia87 configuration transport".into());
    };
    if candidate.pid != 0x4011 {
        return Err("This probe requires the 3151:4011 receiver".into());
    }
    let api = HidApi::new()?;
    let path = CString::new(candidate.path.as_str())?;
    let handle = api.open_path(&path)?;
    let mut observations = Vec::new();
    for &opcode in opcodes {
        let mut request = [0u8; 65];
        request[1..].copy_from_slice(&protocol::read_request(opcode, 0, 0));
        handle.send_feature_report(&request)?;
        for wait_ms in [30, 100, 300, 1000] {
            std::thread::sleep(Duration::from_millis(wait_ms));
            let mut reply = [0u8; 65];
            let observation = match handle.get_feature_report(&mut reply) {
                Ok(length) => {
                    println!(
                        "Read {opcode:02x} after {wait_ms} ms: length={length}, prefix={:02x?}",
                        &reply[..9]
                    );
                    serde_json::json!({
                        "opcode": opcode, "wait_ms": wait_ms, "length": length,
                        "reply": reply.as_slice(),
                    })
                }
                Err(error) => {
                    println!("Read {opcode:02x} after {wait_ms} ms: {error}");
                    serde_json::json!({
                        "opcode": opcode, "wait_ms": wait_ms, "error": error.to_string(),
                    })
                }
            };
            observations.push(observation);
        }
    }
    serde_json::to_writer_pretty(
        &mut file,
        &serde_json::json!({
            "candidate": candidate, "observations": observations,
        }),
    )?;
    file.sync_all()?;
    Ok(())
}
