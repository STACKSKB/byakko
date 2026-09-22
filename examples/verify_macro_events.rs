//! Explicit hardware storage check. No binding or playback; restore an empty slot.
use byakko::{
    device,
    macros::{self, Macro, MacroEvent},
};

fn fixture() -> Macro {
    let mut events = Vec::new();
    for delay_ms in [0, 1, 127, 128, 65535] {
        for down in [true, false] {
            events.push(MacroEvent::Key {
                usage: 115,
                down,
                delay_ms,
            });
        }
        events.push(MacroEvent::Move {
            dx: -128,
            dy: 127,
            delay_ms,
        });
    }
    for button in 240..=248 {
        for down in [true, false] {
            events.push(MacroEvent::MouseButton {
                button,
                down,
                delay_ms: [0, 1, 127, 128, 65535][usize::from(button - 240) % 5],
            });
        }
    }
    Macro {
        repeat_count: 1,
        events,
    }
}

fn main() -> device::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 1 {
        return Err(
            "Usage: verify_macro_events BASELINE.json; writes unbound slot49, then restores it"
                .into(),
        );
    }
    let directory = std::path::Path::new("Research/captures/backups");
    std::fs::create_dir_all(directory)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let trace_path = directory.join(format!("macro-events-setters-{stamp}.json"));
    let mut trace_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&trace_path)?;
    let (result, trace) = byakko::research_trace::with_trace(|| run(&args[0]));
    serde_json::to_writer_pretty(
        &mut trace_file,
        &serde_json::json!({
            "format": "byakko-research-setter-trace", "version": 1,
            "result": format!("{result:?}"), "trace": trace,
            "scope": "Setter API calls, not USB bus capture or proof of firmware execution"
        }),
    )?;
    trace_file.sync_all()?;
    println!("Setter trace: {}", trace_path.display());
    result.map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { error.into() })?
}

fn run(baseline: &str) -> device::Result<()> {
    let original = byakko::configuration::load(std::path::Path::new(baseline))?;
    let slot = 49;
    if original.keymaps.firmware != 0x100
        || original.keymaps.profile != 0
        || original.macros[slot].iter().any(|&byte| byte != 0)
        || original
            .keymaps
            .base
            .iter()
            .chain(&original.keymaps.function)
            .any(|binding| binding[0] == 9 && usize::from(binding[2]) == slot)
    {
        return Err(
            "Expected firmware0100/profile0 with empty unbound slot49; no writes sent".into(),
        );
    }
    let desired = fixture();
    let encoded = macros::encode(&desired)?;
    let current = device::capture_configuration(|done, total| {
        if done % 20 == 0 {
            println!("Before: {done}/{total}");
        }
    })?;
    if current != original {
        return Err("Complete current configuration differs from baseline; no writes sent".into());
    }
    let backups = std::path::Path::new("Research/captures/backups");
    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> device::Result<()> {
        let written = device::apply_macro(slot as u8, &original.macros[slot], &desired, backups)?;
        if written != encoded || macros::decode(&written)? != desired {
            return Err("Mixed-event macro readback mismatch".into());
        }
        println!(
            "All {} events stored exactly; no playback attempted",
            desired.events.len()
        );
        Ok(())
    }))
    .unwrap_or_else(|_| Err("Macro check panicked; attempting explicit slot restoration".into()));

    // Always attempt restoration after the test, even if the test reported an
    // uncertain error. The original archive and per-write backups remain on disk.
    let restore = (|| -> device::Result<()> {
        let current = device::read_macro(slot as u8)?;
        if current != original.macros[slot] {
            let restored = device::apply_macro(
                slot as u8,
                &current,
                &macros::decode(&original.macros[slot])?,
                backups,
            )?;
            if restored != original.macros[slot] {
                return Err("Slot restoration mismatch".into());
            }
        }
        let after = device::capture_configuration(|done, total| {
            if done % 20 == 0 {
                println!("After: {done}/{total}");
            }
        })?;
        if after != original {
            return Err("Complete configuration differs after slot restoration".into());
        }
        Ok(())
    })();
    if let Err(error) = restore {
        return Err(format!(
            "Restoration unverified: {error}; test result: {run:?}; retain baseline {}",
            baseline
        )
        .into());
    }
    println!(
        "Complete baseline restored and verified: both keymaps, all50 macros, picture, lighting and settings"
    );
    run
}
