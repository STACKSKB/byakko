//! Conservative projection of Nia87 global lighting into shared values.
use crate::nia87::{device, lighting as native};
use byakko_core::{
    lighting::{
        self, Capabilities, Choice, Color, ColorCapability, Content, Effect, HostMode,
        HostParameters, HostSource, Setting, Snapshot,
    },
    session::{ApplyFailure, Recovery},
};
use std::path::Path;

pub const BACKEND_ID: &str = "nia87";

fn effect_schema(id: String, effect: &native::Effect) -> Effect {
    Effect {
        id,
        label: effect.name.into(),
        brightness: effect.value.then_some(0..=4),
        speed: effect.speed.then_some(0..=4),
        options: effect
            .options
            .iter()
            .map(|option| Choice {
                id: (*option).into(),
                label: (*option).into(),
            })
            .collect(),
        color: match (effect.rgb, effect.dazzle) {
            (true, true) => Some(ColorCapability::FixedOrRainbow),
            (true, false) => Some(ColorCapability::Fixed),
            (false, true) => Some(ColorCapability::Rainbow),
            (false, false) => None,
        },
    }
}

fn music_mode(id: &str, label: &str, native_id: u8) -> HostMode {
    let effect = native::effect_by_id(native_id).expect("known Nia87 music effect");
    HostMode {
        id: id.into(),
        label: label.into(),
        source: HostSource::PlaybackAudio { bands: 32 },
        parameters: Some(HostParameters {
            schema: effect_schema(id.into(), effect),
            default: Setting {
                effect: id.into(),
                brightness: Some(4),
                speed: None,
                option: Some("upright".into()),
                color: Some(Color::Rgb([0, 255, 0])),
            },
        }),
    }
}

pub fn capabilities() -> Capabilities {
    Capabilities {
        backend_id: BACKEND_ID.into(),
        effects: native::EFFECTS
            .iter()
            .filter(|effect| effect.id <= 19)
            .map(|effect| effect_schema(effect.id.to_string(), effect))
            .collect(),
        host_modes: vec![
            HostMode {
                id: "screen-average".into(),
                label: "Screen average".into(),
                source: HostSource::ScreenAverage,
                parameters: None,
            },
            music_mode("music-follow-2", "Music follow 2", 22),
            music_mode("music-follow-3", "Music follow 3", 20),
        ],
    }
}

fn from_native_setting(value: &native::LightingSetting) -> Setting {
    let effect = native::effect_by_id(value.effect_id).expect("recognized native effect");
    Setting {
        effect: value.effect_id.to_string(),
        brightness: value.value.map(u16::from),
        speed: value.speed.map(u16::from),
        option: value
            .option
            .map(|index| effect.options[usize::from(index)].into()),
        color: if value.dazzle {
            Some(Color::Rainbow)
        } else {
            value.rgb.map(Color::Rgb)
        },
    }
}

fn to_native(value: &Setting) -> Result<native::LightingSetting, String> {
    lighting::validate_setting(&capabilities(), value)?;
    let id = value
        .effect
        .parse::<u8>()
        .map_err(|_| "Invalid Nia87 effect ID")?;
    let effect = native::effect_by_id(id).ok_or("Unknown Nia87 effect")?;
    let option = value
        .option
        .as_ref()
        .map(|id| {
            effect
                .options
                .iter()
                .position(|item| item == id)
                .map(|position| position as u8)
                .ok_or("Unknown Nia87 option")
        })
        .transpose()?;
    let (rgb, dazzle) = match value.color {
        Some(Color::Rgb(rgb)) => (Some(rgb), false),
        Some(Color::Rainbow) => (Some([255; 3]), true),
        None => (None, false),
    };
    let setting = native::LightingSetting {
        effect_id: id,
        value: value.brightness.map(|value| value as u8),
        speed: value.speed.map(|value| value as u8),
        option,
        rgb,
        dazzle,
    };
    Ok(setting)
}

pub fn from_native(value: &native::Lighting) -> Snapshot {
    let content = if value.raw()[0] != native::LED_READ_COMMAND || value.effect_id() > 19 {
        Content::Opaque {
            reason: "Unsupported Nia87 lighting response or host effect".into(),
        }
    } else if let Some(setting) = value.recognized_setting() {
        let projected = from_native_setting(&setting);
        Content::Editable(projected)
    } else {
        Content::Opaque {
            reason: "Unrecognized Nia87 lighting fields".into(),
        }
    };
    Snapshot {
        backend_id: BACKEND_ID.into(),
        revision: value.raw().into(),
        content,
    }
}

pub fn from_bytes(raw: &[u8]) -> Result<Snapshot, String> {
    Ok(from_native(&native::Lighting::decode(raw)?))
}

pub fn draft(expected: &Snapshot, desired: &Setting) -> Result<native::LightingSetting, String> {
    if expected.backend_id != BACKEND_ID {
        return Err("Lighting snapshot belongs to another backend".into());
    }
    let projected = from_bytes(&expected.revision)?;
    if &projected != expected {
        return Err("Lighting snapshot differs from its revision; reload before editing".into());
    }
    if !matches!(expected.content, Content::Editable(_)) {
        return Err("Unrecognized Nia87 lighting is available only as a raw backup".into());
    }
    let mut target = to_native(desired)?;
    // Rainbow leaves the current fixed color dormant in native state. Keep it
    // when editing the same effect so a later switch back to fixed retains it.
    if matches!(desired.color, Some(Color::Rainbow))
        && matches!(&expected.content, Content::Editable(setting) if setting.effect == desired.effect)
    {
        let baseline = native::Lighting::decode(&expected.revision)?;
        target.rgb = baseline
            .recognized_setting()
            .and_then(|setting| setting.rgb);
    }
    let report = native::write_report(&target)?;
    let mut simulated = expected.revision.clone();
    simulated[1..8].copy_from_slice(&report[1..8]);
    let projected = from_bytes(&simulated)?;
    if projected.content != Content::Editable(desired.clone()) {
        return Err("Nia87 lighting setting would not read back as requested".into());
    }
    Ok(target)
}

pub fn read() -> Result<Snapshot, String> {
    read_with(&device::Access::unique())
}

pub(super) fn read_with(access: &device::Access) -> Result<Snapshot, String> {
    let raw = access.read_lighting().map_err(|error| error.to_string())?;
    Ok(from_native(&raw))
}

pub fn apply(
    expected: &Snapshot,
    desired: &Setting,
    backup: &Path,
) -> Result<Snapshot, ApplyFailure> {
    apply_with(&device::Access::unique(), expected, desired, backup)
}

pub(super) fn apply_with(
    access: &device::Access,
    expected: &Snapshot,
    desired: &Setting,
    backup: &Path,
) -> Result<Snapshot, ApplyFailure> {
    let native_setting = draft(expected, desired).map_err(|message| ApplyFailure {
        message,
        recovery: Recovery::NotAttempted,
    })?;
    let expected_native =
        native::Lighting::decode(&expected.revision).map_err(|message| ApplyFailure {
            message,
            recovery: Recovery::NotAttempted,
        })?;
    let actual = access.apply_lighting_detailed(&expected_native, &native_setting, backup)?;
    let snapshot = from_native(&actual);
    if snapshot.content != Content::Editable(desired.clone()) {
        return Err(ApplyFailure {
            message: "Lighting readback cannot be represented as requested".into(),
            recovery: Recovery::Unverified,
        });
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_excludes_host_effects_and_codec_round_trips() {
        let caps = capabilities();
        assert_eq!(caps.effects.len(), 20);
        lighting::validate_capabilities(&caps).unwrap();
        let setting = Setting {
            effect: "4".into(),
            brightness: Some(3),
            speed: Some(2),
            option: Some("up".into()),
            color: Some(Color::Rgb([9, 8, 7])),
        };
        let report = native::write_report(&to_native(&setting).unwrap()).unwrap();
        let mut response = report;
        response[0] = native::LED_READ_COMMAND;
        response[63] = 0xa5;
        let snapshot = from_bytes(&response).unwrap();
        assert_eq!(snapshot.content, Content::Editable(setting));
        assert_eq!(snapshot.revision, response);
    }

    #[test]
    fn forged_or_unrepresentable_baselines_never_reach_write() {
        let mut raw = [0u8; 64];
        raw[0] = native::LED_READ_COMMAND;
        raw[1] = 21;
        let host = from_bytes(&raw).unwrap();
        assert!(matches!(host.content, Content::Opaque { .. }));
        let desired = lighting::default_setting(&capabilities(), "1").unwrap();
        assert!(draft(&host, &desired).is_err());
        let mut forged = host.clone();
        forged.content = Content::Editable(desired.clone());
        assert!(draft(&forged, &desired).is_err());
        raw[1] = 1;
        raw[2] = 4;
        raw[4] = 9;
        raw[5..8].copy_from_slice(&[3, 4, 5]);
        assert!(matches!(
            from_bytes(&raw).unwrap().content,
            Content::Opaque { .. }
        ));
    }

    #[test]
    fn captured_baseline_is_editable_and_keeps_raw_revision() {
        // Research/captures/configuration-getter-trace-baseline.json: lighting raw[0..8].
        let mut raw = [0u8; 64];
        raw[..8].copy_from_slice(&[135, 5, 4, 4, 7, 8, 8, 8]);
        let snapshot = from_bytes(&raw).unwrap();
        assert_eq!(
            snapshot.content,
            Content::Editable(Setting {
                effect: "5".into(),
                brightness: Some(4),
                speed: Some(0),
                option: None,
                color: Some(Color::Rgb([8, 8, 8])),
            })
        );
        assert_eq!(snapshot.revision, raw);
    }

    #[test]
    fn preflight_covers_every_declared_effect_and_rejects_sentinel_alias() {
        let caps = capabilities();
        let mut raw = [0u8; 64];
        raw[..8].copy_from_slice(&[135, 0, 4, 0, 7, 0, 0, 0]);
        let baseline = from_bytes(&raw).unwrap();
        for effect in &caps.effects {
            let brightnesses: Vec<_> = effect.brightness.as_ref().map_or(vec![None], |range| {
                vec![Some(*range.start()), Some(*range.end())]
            });
            let speeds: Vec<_> = effect.speed.as_ref().map_or(vec![None], |range| {
                vec![Some(*range.start()), Some(*range.end())]
            });
            let options: Vec<_> = if effect.options.is_empty() {
                vec![None]
            } else {
                effect
                    .options
                    .iter()
                    .map(|choice| Some(choice.id.clone()))
                    .collect()
            };
            let colors = match effect.color {
                None => vec![None],
                Some(ColorCapability::Fixed) => vec![Some(Color::Rgb([9, 8, 7]))],
                Some(ColorCapability::Rainbow) => vec![Some(Color::Rainbow)],
                Some(ColorCapability::FixedOrRainbow) => {
                    vec![Some(Color::Rgb([9, 8, 7])), Some(Color::Rainbow)]
                }
            };
            for brightness in &brightnesses {
                for speed in &speeds {
                    for option in &options {
                        for color in &colors {
                            let desired = Setting {
                                effect: effect.id.clone(),
                                brightness: *brightness,
                                speed: *speed,
                                option: option.clone(),
                                color: color.clone(),
                            };
                            let native = draft(&baseline, &desired).unwrap();
                            let report = native::write_report(&native).unwrap();
                            assert_eq!(report[0], native::LED_WRITE_COMMAND);
                            assert_eq!(report[1].to_string(), desired.effect);
                            assert_eq!(report[2], 4 - desired.speed.unwrap_or(0) as u8);
                            assert_eq!(report[3], desired.brightness.unwrap_or(0) as u8);
                            assert_eq!(
                                report[4] >> 4,
                                option.as_ref().map_or(0, |id| effect
                                    .options
                                    .iter()
                                    .position(|choice| &choice.id == id)
                                    .unwrap()
                                    as u8)
                            );
                            assert_eq!(
                                report[4] & 0x0f,
                                if effect.id == "13" {
                                    0
                                } else if matches!(color, Some(Color::Rainbow)) {
                                    8
                                } else {
                                    7
                                }
                            );
                            assert_eq!(
                                report[5..8],
                                if effect.id == "13" {
                                    [0, 200, 200]
                                } else if matches!(color, Some(Color::Rgb(_))) {
                                    [9, 8, 7]
                                } else if matches!(color, Some(Color::Rainbow)) {
                                    [250, 255, 250]
                                } else {
                                    [0, 0, 0]
                                }
                            );
                        }
                    }
                }
            }
        }
        let alias = Setting {
            effect: "5".into(),
            brightness: Some(4),
            speed: Some(0),
            option: None,
            color: Some(Color::Rgb([250, 255, 250])),
        };
        assert!(draft(&baseline, &alias).is_err());
    }

    #[test]
    fn same_effect_rainbow_keeps_dormant_native_rgb() {
        let mut raw = [0u8; 64];
        raw[..8].copy_from_slice(&[135, 5, 4, 4, 8, 10, 20, 30]);
        raw[63] = 0xb7;
        let snapshot = from_bytes(&raw).unwrap();
        let Content::Editable(mut desired) = snapshot.content.clone() else {
            panic!("expected editable rainbow")
        };
        desired.brightness = Some(2);
        let native = draft(&snapshot, &desired).unwrap();
        let report = native::write_report(&native).unwrap();
        assert_eq!(&report[5..8], &[10, 20, 30]);
        assert_eq!(snapshot.revision[63], 0xb7);
    }
}
