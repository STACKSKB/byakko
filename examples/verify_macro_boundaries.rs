//! Reversible capacity/page/slot checks, restricted to empty unbound macros.
mod support;
use byakko_devices::nia87::device;
use byakko_protocol::nia87::macros::{self, Macro, MacroEvent};

fn main() -> device::Result<()> {
    let directory = std::path::Path::new("Research/captures/backups");
    std::fs::create_dir_all(directory)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = directory.join(format!("macro-boundaries-transport-{stamp}.json"));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let (result, trace) = byakko_devices::research_trace::with_trace(run);
    serde_json::to_writer_pretty(
        &mut file,
        &serde_json::json!({
            "result": format!("{result:?}"), "trace": trace,
            "scope": "Host feature-report attempts; not USB bus timing or playback evidence"
        }),
    )?;
    file.sync_all()?;
    println!("Transport trace: {}", path.display());
    result.map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { error.into() })?
}

fn run() -> device::Result<()> {
    let access = support::access()?;
    let original = access.capture_configuration(|_, _| {})?;
    let maps = &original.keymaps;
    if maps.firmware != 0x100 || maps.profile != 0 {
        return Err("Unverified firmware/profile".into());
    }
    // Slots0/1 may contain the official-app playback fixture.
    let slots = match std::env::args().nth(1) {
        Some(slot) => vec![slot.parse::<u8>()?],
        None => vec![2, 24, 49],
    };
    if slots.iter().any(|slot| *slot >= 50) {
        return Err("Macro slot must be in 0..49".into());
    }
    for &slot in &slots {
        if maps
            .base
            .iter()
            .chain(&maps.function)
            .any(|binding| binding[0] == 9 && binding[2] == slot)
        {
            return Err(format!("Slot {slot} is bound; no writes sent").into());
        }
        let bytes = &original.macros[usize::from(slot)];
        if bytes.iter().any(|&byte| byte != 0) {
            return Err(format!("Slot {slot} is not empty; no writes sent").into());
        }
    }
    let mut events: Vec<_> = (0..120)
        .map(|i| MacroEvent::Key {
            usage: 115,
            down: i % 2 == 0,
            delay_ms: 127,
        })
        .collect();
    events.push(MacroEvent::Move {
        dx: -127,
        dy: 127,
        delay_ms: 65535,
    });
    let boundary = Macro {
        repeat_count: 65535,
        events,
    };
    let boundary_bytes = macros::encode(&boundary)?;
    if boundary_bytes[247] == 0 || boundary_bytes[248..].iter().any(|&b| b != 0) {
        return Err("Boundary fixture does not end at byte248".into());
    }
    let short = Macro {
        repeat_count: 1,
        events: vec![
            MacroEvent::Key {
                usage: 115,
                down: true,
                delay_ms: 0,
            },
            MacroEvent::Key {
                usage: 115,
                down: false,
                delay_ms: 128,
            },
        ],
    };
    let backups = std::path::Path::new("Research/captures/backups");
    for slot in slots {
        let before = &original.macros[usize::from(slot)];
        let result = (|| -> device::Result<()> {
            let written = access.apply_macro(slot, before, &boundary, backups)?;
            if written != boundary_bytes || macros::decode(&written)? != boundary {
                return Err("Boundary readback mismatch".into());
            }
            let written = access.apply_macro(slot, &written, &short, backups)?;
            if macros::decode(&written)? != short {
                return Err("Short replacement mismatch".into());
            }
            Ok(())
        })();
        let current = access.read_macro(slot)?;
        let restored = access.apply_macro(slot, &current, &macros::decode(before)?, backups)?;
        if &restored != before {
            return Err(format!("Slot {slot} restoration failed; original backup retained").into());
        }
        result?;
        println!("Slot {slot}: full248-byte macro → short → empty; restored exactly");
    }
    if access.capture_configuration(|_, _| {})? != original {
        return Err("Complete configuration changed unexpectedly".into());
    }
    println!(
        "All boundary/slot checks passed; no macros were bound or played; complete configuration unchanged"
    );
    Ok(())
}
