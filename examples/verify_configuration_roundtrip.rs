//! Reversible multi-section archive check, with no macro binding or playback.
use byakko::{
    configuration, device,
    lighting::Lighting,
    macros::{self, Macro, MacroEvent},
    settings::Settings,
};

fn main() -> device::Result<()> {
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
