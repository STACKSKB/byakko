//! Reversible storage checks for onboard lighting parameter families.
mod support;
use byakko_devices::nia87::device;
use byakko_protocol::nia87::lighting::LightingSetting;

fn main() -> device::Result<()> {
    let access = support::access()?;
    let original = access.read_lighting()?;
    let original_setting = original
        .recognized_setting()
        .ok_or("Unrecognized original lighting")?;
    let maps = access.snapshot()?;
    let picture = access.read_picture_with_context()?.0;
    let settings = access.read_settings()?;
    let backups = std::path::Path::new("Research/captures/backups");
    let mut cases = vec![
        LightingSetting {
            effect_id: 1,
            value: Some(2),
            speed: None,
            option: None,
            rgb: Some([255, 255, 255]),
            dazzle: false,
        },
        LightingSetting {
            effect_id: 4,
            value: Some(2),
            speed: Some(3),
            option: Some(3),
            rgb: Some([8, 16, 24]),
            dazzle: false,
        },
        LightingSetting {
            effect_id: 4,
            value: Some(2),
            speed: Some(1),
            option: Some(1),
            rgb: Some([8, 16, 24]),
            dazzle: true,
        },
        LightingSetting {
            effect_id: 3,
            value: Some(2),
            speed: Some(2),
            option: None,
            rgb: None,
            dazzle: false,
        },
        LightingSetting {
            effect_id: 13,
            value: Some(2),
            speed: None,
            option: Some(1),
            rgb: None,
            dazzle: false,
        },
        LightingSetting {
            effect_id: 0,
            value: None,
            speed: None,
            option: None,
            rgb: None,
            dazzle: false,
        },
    ];
    if std::env::args().any(|arg| arg == "remaining") {
        cases.clear();
        for id in [2, 5, 6, 7, 8, 9, 10, 11, 12, 14, 15, 16, 17, 18, 19] {
            let effect =
                byakko_protocol::nia87::lighting::effect_by_id(id).expect("catalog effect");
            cases.push(LightingSetting {
                effect_id: id,
                value: effect.value.then_some(2),
                speed: effect.speed.then_some(2),
                option: (!effect.options.is_empty()).then(|| (effect.options.len() - 1) as u8),
                rgb: effect.rgb.then_some([8, 16, 24]),
                dazzle: false,
            });
        }
    }
    let count = cases.len();
    for desired in cases {
        let written = access.apply_lighting(&original, &desired, backups)?;
        let recognized = written.recognized_setting();
        let restored = access.apply_lighting(&written, &original_setting, backups)?;
        if restored.raw()[1..8] != original.raw()[1..8] {
            return Err("Original lighting was not restored".into());
        }
        if recognized.as_ref() != Some(&desired) {
            return Err(format!(
                "Effect {} decoder mismatch after verified restore",
                desired.effect_id
            )
            .into());
        }
        println!(
            "Effect {} parameters stored, decoded and restored",
            desired.effect_id
        );
    }
    if access.snapshot()? != maps
        || access.read_picture_with_context()?.0 != picture
        || access.read_settings()? != settings
    {
        return Err("Other keyboard state changed during lighting test".into());
    }
    println!(
        "{count} lighting parameter cases passed; keymaps, picture and settings unchanged. Visual effects untested."
    );
    Ok(())
}
