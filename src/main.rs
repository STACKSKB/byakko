use std::process::ExitCode;

fn run() -> byakko::device::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["verify-macro-roundtrip"] {
        use byakko::macros::{Macro, MacroEvent};
        let maps = byakko::device::snapshot()?;
        if maps
            .base
            .iter()
            .chain(maps.function.iter())
            .any(|b| b[0] == 9 && b[2] == 49)
        {
            return Err("Macro49 is bound to a key; test will not change it".into());
        }
        let bytes = byakko::device::read_macro(49)?;
        if bytes.iter().any(|b| *b != 0) {
            return Err("Macro49 is not empty; test will not overwrite it".into());
        }
        let value = Macro {
            repeat_count: 1,
            events: vec![
                MacroEvent::Key {
                    usage: 0x73,
                    down: true,
                    delay_ms: 50,
                },
                MacroEvent::Key {
                    usage: 0x73,
                    down: false,
                    delay_ms: 50,
                },
            ],
        };
        let written = byakko::device::apply_macro(
            49,
            &bytes,
            &value,
            std::path::Path::new("Research/captures/backups"),
        )?;
        byakko::device::apply_macro(
            49,
            &written,
            &Macro {
                repeat_count: 0,
                events: vec![],
            },
            std::path::Path::new("Research/captures/backups"),
        )?;
        if byakko::device::snapshot()? != maps {
            return Err("Keymaps changed during macro test".into());
        }
        println!(
            "Unbound macro49 F24 press/release stored and read back exactly, then empty original restored. Keymaps unchanged; no playback triggered."
        );
        return Ok(());
    }
    if args.len() == 3 && args[0] == "export-macro" {
        let slot = args[1].parse::<u8>()?;
        let bytes = byakko::device::read_macro(slot)?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[2])?;
        serde_json::to_writer_pretty(file, &serde_json::json!({"slot":slot,"bytes":bytes}))?;
        println!(
            "Saved macro slot {slot}, 256 raw bytes; repeated reads matched. Contents not printed."
        );
        return Ok(());
    }
    #[cfg(feature = "gui")]
    if args.is_empty() || args.as_slice() == ["gui"] {
        return byakko::app::run().map_err(|e| e.to_string().into());
    }
    if args.as_slice() == ["verify-keymap-roundtrip"] {
        let original = byakko::device::snapshot()?;
        let mut changed = original.base.clone();
        // Pause is slot 91 in this board's observed unmodified matrix.
        if original.firmware != 0x0100 || original.profile != 0 || changed[91] != [0, 0, 0x48, 0] {
            return Err("Round-trip fixture precondition not met; no writes sent".into());
        }
        changed[91] = [0, 0, 0x73, 0]; // F24, a non-character key.
        let applied = byakko::device::apply_keymaps(
            &original,
            &changed,
            &original.function,
            std::path::Path::new("Research/captures/backups"),
        )?;
        let restored = byakko::device::apply_keymaps(
            &applied,
            &original.base,
            &original.function,
            std::path::Path::new("Research/captures/backups"),
        )?;
        if restored != original {
            return Err("Restore mismatch".into());
        }
        println!(
            "Pause -> F24 verified by complete keymap readback; original base and Fn maps restored and verified. Physical output remains untested."
        );
        return Ok(());
    }
    if args.len() == 2 && args[0] == "export" {
        let data = byakko::device::snapshot()?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[1])?;
        serde_json::to_writer_pretty(file, &data)?;
        println!(
            "Saved verified duplicate reads: {} base slots, {} Fn slots; raw version {:04x}, profile {}",
            data.base.len(),
            data.function.len(),
            data.firmware,
            data.profile
        );
        return Ok(());
    }
    if args.as_slice() == ["inspect"] {
        println!(
            "{}",
            serde_json::to_string_pretty(&byakko::device::inspect()?)?
        );
        return Ok(());
    }
    if args.as_slice() == ["descriptor"] {
        println!(
            "{}",
            serde_json::to_string_pretty(&byakko::device::descriptor()?)?
        );
        return Ok(());
    }
    if args.as_slice() != ["devices"] {
        return Err("Usage: byakko devices (enumeration only; no configuration commands)".into());
    }
    let api = hidapi::HidApi::new()?;
    let mut count = 0;
    for device in api
        .device_list()
        .filter(|d| d.vendor_id() == 0x3151 && matches!(d.product_id(), 0x4011 | 0x4015))
    {
        count += 1;
        println!(
            "candidate {:04x}:{:04x} interface={} usage={:04x}:{:04x} release={:04x} bus={:?}",
            device.vendor_id(),
            device.product_id(),
            device.interface_number(),
            device.usage_page(),
            device.usage(),
            device.release_number(),
            device.bus_type()
        );
        println!(
            "  manufacturer={:?} product={:?}",
            device.manufacturer_string(),
            device.product_string()
        );
        println!("  path={}", device.path().to_string_lossy());
        if device.usage_page() == 0xffff && device.usage() == 2 {
            println!(
                "  matches vendor configuration collection; board identity still needs protocol verification"
            );
        }
    }
    if count == 0 {
        return Err("No candidate Nia87 HID collections found. Check USB connection and device permissions.".into());
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
