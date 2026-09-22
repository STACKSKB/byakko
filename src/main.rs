use std::process::ExitCode;

fn run() -> byakko::device::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--help"] || args.as_slice() == ["-h"] {
        println!(
            "Byakko — native Nia87 configurator\n\n  gui                         Open native workbench\n  devices                     Enumerate matching HID collections\n  inspect                     Read version/profile\n  inspect-lighting             Read global lighting\n  inspect-settings             Read debounce, auto OS, sleep and flags\n  export PATH                  Save raw keymap snapshot to new file\n  export-macro SLOT PATH       Save raw macro bytes to new file\n  export-picture PATH          Save RGB picture to new file\n  restore-keymaps BACKUP        Restore a verified keymap backup\n  restore-macro BACKUP          Restore a macro backup\n\nResearch round-trip checks (write, verify and restore):\n  verify-keymap-roundtrip\n  verify-fn-roundtrip\n  verify-bindings-roundtrip\n  verify-macro-roundtrip\n  verify-lighting-roundtrip\n  verify-picture-roundtrip\n  verify-settings-roundtrip\n\nSimultaneous changes to both layers of the same key are guarded."
        );
        return Ok(());
    }
    if args.as_slice() == ["verify-fn-roundtrip"] {
        let original = byakko::device::snapshot()?;
        if original.base[91] != [0, 0, 0x48, 0] || original.function[91] != [0; 4] {
            return Err("Expected original Pause fixture; no writes sent".into());
        }
        let backups = std::path::Path::new("Research/captures/backups");
        for binding in [[0, 0, 0x73, 0], [3, 0, 205, 0]] {
            let base = original.base.clone();
            let mut function = original.function.clone();
            function[91] = binding;
            let written = byakko::device::apply_keymaps(&original, &base, &function, backups)?;
            let restored = byakko::device::apply_keymaps(
                &written,
                &original.base,
                &original.function,
                backups,
            )?;
            if restored != original {
                return Err("Complete Fn/base restoration mismatch".into());
            }
            println!(
                "Fn {binding:?}: complete write/readback/restoration verified; base unchanged."
            );
        }
        println!("Physical key output remains untested.");
        return Ok(());
    }
    if args.as_slice() == ["inspect-settings"] {
        let settings = byakko::device::read_settings()?;
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"debounce":settings.debounce(),"auto_os":settings.auto_os(),"sleep_seconds":settings.sleep_seconds(),"options_flags":settings.option_flags(),"fn_matrix_enabled":settings.fn_matrix_enabled(),"power_save":settings.power_save_value(),"raw":settings})
            )?
        );
        return Ok(());
    }
    if args.as_slice() == ["verify-settings-roundtrip"] {
        use byakko::settings::Setting;
        let before = byakko::device::read_settings()?;
        if before.debounce() != 1 || before.auto_os() {
            return Err("Expected captured debounce1/auto-off fixture; no writes sent".into());
        }
        let maps = byakko::device::snapshot()?;
        let lighting = byakko::device::read_lighting()?;
        let backups = std::path::Path::new("Research/captures/backups");
        let changed = byakko::device::apply_setting(&before, Setting::Debounce(2), backups)?;
        let restored = byakko::device::apply_setting(&changed, Setting::Debounce(1), backups)?;
        if restored != before {
            return Err("Debounce restoration mismatch".into());
        }
        let changed = byakko::device::apply_setting(&before, Setting::AutoOs(true), backups)?;
        let restored = byakko::device::apply_setting(&changed, Setting::AutoOs(false), backups)?;
        if restored != before
            || byakko::device::snapshot()? != maps
            || byakko::device::read_lighting()? != lighting
        {
            return Err("Settings test did not restore all checked state".into());
        }
        println!(
            "Debounce1->2->1 and auto-off->on->off verified. All settings, keymaps and global lighting restored; sleep/options not written."
        );
        return Ok(());
    }
    if args.len() == 2 && args[0] == "restore-keymaps" {
        let saved: byakko::device::Snapshot =
            serde_json::from_reader(std::fs::File::open(&args[1])?)?;
        let current = byakko::device::snapshot()?;
        let restored = byakko::device::apply_keymaps(
            &current,
            &saved.base,
            &saved.function,
            std::path::Path::new("backups"),
        )?;
        if restored != saved {
            return Err("Backup restoration mismatch".into());
        }
        println!("Both keymaps restored and verified against backup.");
        return Ok(());
    }
    if args.as_slice() == ["verify-bindings-roundtrip"] {
        let original = byakko::device::snapshot()?;
        if byakko::device::read_macro(49)?.iter().any(|b| *b != 0)
            || original
                .base
                .iter()
                .chain(&original.function)
                .any(|b| b[0] == 9 && b[2] == 49)
        {
            return Err("Test requires empty, unbound macro49; no writes sent".into());
        }
        let backup_dir = std::path::Path::new("Research/captures/backups");
        for mode in 0..3 {
            let mut base = original.base.clone();
            base[91] = [9, mode, 49, 0];
            let changed =
                byakko::device::apply_keymaps(&original, &base, &original.function, backup_dir)?;
            let restored = byakko::device::apply_keymaps(
                &changed,
                &original.base,
                &original.function,
                backup_dir,
            )?;
            if restored != original {
                return Err("Binding restoration mismatch".into());
            }
        }
        println!(
            "All three base-layer macro binding modes passed complete readback and restoration. Physical playback remains unverified."
        );
        return Ok(());
    }
    if args.as_slice() == ["verify-picture-roundtrip"] {
        let before = byakko::device::read_picture()?;
        let maps = byakko::device::snapshot()?;
        let lighting = byakko::device::read_lighting()?;
        let mut changed = before.clone();
        changed[91] = if before[91] == [8, 16, 24] {
            [24, 16, 8]
        } else {
            [8, 16, 24]
        };
        let backups = std::path::Path::new("Research/captures/backups");
        let written = byakko::device::apply_picture(&before, &changed, backups)?;
        let restored = byakko::device::apply_picture(&written, &before, backups)?;
        if restored != before
            || byakko::device::snapshot()? != maps
            || byakko::device::read_lighting()? != lighting
        {
            return Err("Picture test changed other state".into());
        }
        println!(
            "Pause color changed and restored; all128 colors, both keymaps and global lighting verified unchanged afterward. Visible picture untested."
        );
        return Ok(());
    }
    if args.len() == 2 && args[0] == "export-picture" {
        let colors = byakko::device::read_picture()?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[1])?;
        serde_json::to_writer_pretty(file, &colors)?;
        println!("Saved 128 RGB triples; repeated reads matched.");
        return Ok(());
    }
    if args.as_slice() == ["verify-lighting-roundtrip"] {
        let original = byakko::device::read_lighting()?;
        let original_setting = original
            .recognized_setting()
            .ok_or("Unrecognized lighting; no writes sent")?;
        if original.effect_id() != 5 || original_setting.value != Some(4) {
            return Err("Expected captured ripple/brightness4 fixture; no writes sent".into());
        }
        let maps = byakko::device::snapshot()?;
        let mut changed = original_setting.clone();
        changed.value = Some(3);
        let backup_dir = std::path::Path::new("Research/captures/backups");
        let written = byakko::device::apply_lighting(&original, &changed, backup_dir)?;
        let restored = byakko::device::apply_lighting(&written, &original_setting, backup_dir)?;
        if restored.raw()[1..8] != original.raw()[1..8] || byakko::device::snapshot()? != maps {
            return Err("Lighting restore or keymap comparison failed".into());
        }
        println!(
            "Ripple brightness4 ->3 ->4 verified by repeated readback; original lighting fields restored and keymaps unchanged. Visible output untested."
        );
        return Ok(());
    }
    if args.len() == 2 && args[0] == "restore-macro" {
        #[derive(serde::Deserialize)]
        struct Backup {
            slot: u8,
            bytes: Vec<u8>,
        }
        let backup: Backup = serde_json::from_reader(std::fs::File::open(&args[1])?)?;
        let value = byakko::macros::decode(&backup.bytes)?;
        let current = byakko::device::read_macro(backup.slot)?;
        byakko::device::apply_macro(
            backup.slot,
            &current,
            &value,
            std::path::Path::new("backups"),
        )?;
        println!("Macro {} restored from backup and verified.", backup.slot);
        return Ok(());
    }
    if args.as_slice() == ["inspect-lighting"] {
        let lighting = byakko::device::read_lighting()?;
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "raw": lighting.raw(), "setting": lighting.recognized_setting(),
                "effect_id": lighting.effect_id(), "repeated_reads_matched": true
            }))?
        );
        return Ok(());
    }
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
        let mut long_value = value.clone();
        long_value.events = (0..60)
            .map(|i| MacroEvent::Key {
                usage: 0x73,
                down: i % 2 == 0,
                delay_ms: if i % 3 == 0 { 0 } else { 300 },
            })
            .collect();
        let backup_dir = std::path::Path::new("Research/captures/backups");
        let exercise = (|| -> byakko::device::Result<()> {
            let long_bytes = byakko::device::apply_macro(49, &bytes, &long_value, backup_dir)?;
            byakko::device::apply_macro(49, &long_bytes, &value, backup_dir)?;
            Ok(())
        })();
        // Attempt restoration even when an intermediate validation fails.
        let written = byakko::device::read_macro(49)?;
        byakko::device::apply_macro(
            49,
            &written,
            &Macro {
                repeat_count: 0,
                events: vec![],
            },
            std::path::Path::new("Research/captures/backups"),
        )?;
        exercise?;
        if byakko::device::snapshot()? != maps {
            return Err("Keymaps changed during macro test".into());
        }
        println!(
            "Unbound macro49: five-page macro, short replacement and empty restoration verified byte-for-byte. Zero/long delays preserved. Keymaps unchanged; no playback triggered."
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
        return Err("Usage: byakko [gui|devices|descriptor|inspect|export PATH|export-macro SLOT PATH|verify-keymap-roundtrip|verify-macro-roundtrip]".into());
    }
    let api = byakko::hid::HidApi::new()?;
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
