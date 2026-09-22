//! Reversible capacity/page/slot checks, restricted to empty unbound macros.
use byakko::{
    device,
    macros::{self, Macro, MacroEvent},
};

fn main() -> device::Result<()> {
    let maps = device::snapshot()?;
    if maps.firmware != 0x100 || maps.profile != 0 {
        return Err("Unverified firmware/profile".into());
    }
    // Slot0 already contains data on the attached board; leave it untouched.
    let slots = [1, 24, 49];
    let mut originals = Vec::new();
    for slot in slots {
        if maps
            .base
            .iter()
            .chain(&maps.function)
            .any(|binding| binding[0] == 9 && binding[2] == slot)
        {
            return Err(format!("Slot {slot} is bound; no writes sent").into());
        }
        let bytes = device::read_macro(slot)?;
        if bytes.iter().any(|&byte| byte != 0) {
            return Err(format!("Slot {slot} is not empty; no writes sent").into());
        }
        originals.push(bytes);
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
    for (index, slot) in slots.into_iter().enumerate() {
        let result = (|| -> device::Result<()> {
            let written = device::apply_macro(slot, &originals[index], &boundary, backups)?;
            if written != boundary_bytes || macros::decode(&written)? != boundary {
                return Err("Boundary readback mismatch".into());
            }
            let written = device::apply_macro(slot, &written, &short, backups)?;
            if macros::decode(&written)? != short {
                return Err("Short replacement mismatch".into());
            }
            Ok(())
        })();
        let current = device::read_macro(slot)?;
        let restored =
            device::apply_macro(slot, &current, &macros::decode(&originals[index])?, backups)?;
        if restored != originals[index] {
            return Err(format!("Slot {slot} restoration failed; original backup retained").into());
        }
        result?;
        for (other, original) in slots.iter().zip(&originals) {
            if device::read_macro(*other)? != *original {
                return Err(format!("Slot {other} changed unexpectedly").into());
            }
        }
        println!(
            "Slot {slot}: full248-byte macro → short → empty; all three tested slots matched originals"
        );
    }
    if device::snapshot()? != maps {
        return Err("Keymaps changed unexpectedly".into());
    }
    println!("All boundary/slot checks passed; no macros were bound or played; keymaps unchanged");
    Ok(())
}
