//! Reversible multi-section archive check, with no macro binding or playback.
use byakko::{
    configuration, device,
    lighting::Lighting,
    macros::{self, Macro, MacroEvent},
    settings::Settings,
};

fn main() -> device::Result<()> {
    let fault_opcode = match std::env::args().nth(2).as_deref() {
        None => None,
        Some("--fault-after-lighting") => Some(0x07),
        Some("--fault-after-macro-page") => Some(0x16),
        _ => return Err("Unknown verification option; no device access".into()),
    };
    #[cfg(not(feature = "research-tools"))]
    if fault_opcode.is_some() {
        return Err("Fault verification requires the research-tools feature".into());
    }
    let path = std::env::args()
        .nth(1)
        .ok_or("Provide the baseline archive path")?;
    let original = configuration::load(std::path::Path::new(&path))?;
    if original.keymaps.base[91] != [0, 0, 0x48, 0]
        || original.macros[49].iter().any(|&b| b != 0)
        || original
            .keymaps
            .base
            .iter()
            .chain(&original.keymaps.function)
            .any(|b| b[0] == 9 && b[2] == 49)
        || original.settings.debounce() != 1
        || original.lighting.effect_id() != 5
        || original.lighting.value() != 4
    {
        return Err("Expected reversible fixture not present; no writes sent".into());
    }
    let mut target = original.clone();
    target.keymaps.base[91] = [0, 0, 0x73, 0];
    target.macros[49] = macros::encode(&Macro {
        repeat_count: 1,
        events: vec![
            MacroEvent::Key {
                usage: 115,
                down: true,
                delay_ms: 30,
            },
            MacroEvent::Key {
                usage: 115,
                down: false,
                delay_ms: 30,
            },
        ],
    })?;
    target.picture[91] = [12, 34, 56];
    let mut light = target.lighting.raw().to_vec();
    light[3] = 3;
    target.lighting = Lighting::decode(&light)?;
    let mut replies =
        [0x91, 0x97, 0x92, 0x86].map(|op| original.settings.raw_reply(op).unwrap().to_vec());
    replies[0][2] = 2;
    target.settings = Settings::decode(&replies[0], &replies[1], &replies[2], &replies[3])?;
    let backups = std::path::Path::new("Research/captures/backups");
    #[cfg(feature = "research-tools")]
    if let Some(opcode) = fault_opcode {
        // Reserve the trace path before any test write. Actual trace collection
        // is in memory, so diagnostic disk I/O cannot interrupt a setter.
        use std::io::Write;
        std::fs::create_dir_all(backups)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let trace_path = backups.join(format!("configuration-fault-setters-{stamp}.json"));
        let mut trace_file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&trace_path)?;
        let (run, trace) = byakko::research_trace::with_trace(|| {
            byakko::research_fault::with_fault(opcode, true, || {
                device::apply_configuration(&original, &target, backups, |s| {
                    println!("Fault test: {s}")
                })
            })
        });
        let outcome = match &run {
            Ok((Ok(_), fired)) => format!("apply succeeded; fault fired: {fired}"),
            Ok((Err(error), fired)) => format!("{error}; fault fired: {fired}"),
            Err(error) => error.clone(),
        };
        serde_json::to_writer_pretty(
            &mut trace_file,
            &serde_json::json!({
                "format": "byakko-research-setter-trace", "version": 1,
                "fault_opcode": opcode, "outcome": outcome, "trace": trace,
                "scope": "Setter calls only; not a USB bus capture or proof of firmware delivery"
            }),
        )?;
        trace_file.write_all(b"\n")?;
        trace_file.sync_all()?;
        println!("Setter trace saved to {}", trace_path.display());
        let (result, fired) =
            run.map_err(|error| format!("{error}; trace {}", trace_path.display()))?;
        let message = result
            .err()
            .ok_or("Injected error was not reported; inspect setter trace")?
            .to_string();
        if !fired
            || !message.contains("injected configuration fault")
            || !message.contains("restore: verified;")
        {
            return Err(format!("Fault recovery did not verify: {message}").into());
        }
        // Independently read changed sections after the transaction released
        // its lock; recovery itself compared every section and all50 macros.
        if device::snapshot()? != original.keymaps
            || device::read_macro(49)? != original.macros[49]
            || device::read_picture()? != original.picture
            || device::read_lighting()? != original.lighting
            || device::read_settings()? != original.settings
        {
            return Err("Post-recovery independent readback mismatch".into());
        }
        println!(
            "Delivered setter 0x{opcode:02x}, injected one error, and verified automatic full recovery plus independent section reads. {message}"
        );
        return Ok(());
    }
    let applied =
        device::apply_configuration(&original, &target, backups, |s| println!("Apply: {s}"))?;
    let restored =
        device::apply_configuration(&applied, &original, backups, |s| println!("Restore: {s}"))?;
    if restored != original {
        return Err("Complete restoration mismatch".into());
    }
    println!(
        "Keymap, macro49, picture, brightness and debounce changed together and restored; all archive sections verified. No physical playback tested."
    );
    Ok(())
}
