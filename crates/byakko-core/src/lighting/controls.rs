//! Device-neutral descriptions of the controls for an editable lighting setting.
use super::{
    Capabilities, Channel, Color, ColorCapability, Edit, Effect, Setting, validate_parameters,
    validate_setting,
};
use std::ops::RangeInclusive;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LevelEdit {
    Brightness,
    Speed,
    Channel(Channel),
}

impl LevelEdit {
    pub fn edit(self, value: u16) -> Result<Edit, String> {
        match self {
            Self::Brightness => Ok(Edit::Brightness(value)),
            Self::Speed => Ok(Edit::Speed(value)),
            Self::Channel(channel) => u8::try_from(value)
                .map(|value| Edit::Channel(channel, value))
                .map_err(|_| "Lighting color channel is outside the byte range".into()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChoiceEdit {
    pub label: String,
    pub selected: bool,
    pub edit: Edit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Control {
    Level {
        label: &'static str,
        range: RangeInclusive<u16>,
        value: u16,
        edit: LevelEdit,
    },
    Choices {
        label: &'static str,
        choices: Vec<ChoiceEdit>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Controls {
    pub effects: Vec<ChoiceEdit>,
    pub settings: Vec<Control>,
}

pub fn controls(caps: &Capabilities, setting: &Setting) -> Result<Controls, String> {
    validate_setting(caps, setting)?;
    let effect = caps
        .effects
        .iter()
        .find(|effect| effect.id == setting.effect)
        .ok_or("Unknown lighting effect")?;

    let effects = caps
        .effects
        .iter()
        .map(|candidate| ChoiceEdit {
            label: candidate.label.clone(),
            selected: candidate.id == setting.effect,
            edit: Edit::Effect(candidate.id.clone()),
        })
        .collect();
    let settings = parameter_controls(effect, setting)?;
    Ok(Controls { effects, settings })
}

pub fn parameter_controls(effect: &Effect, setting: &Setting) -> Result<Vec<Control>, String> {
    validate_parameters(effect, setting)?;
    let mut settings = Vec::new();

    if let (Some(range), Some(value)) = (&effect.brightness, setting.brightness) {
        settings.push(Control::Level {
            label: "Brightness",
            range: range.clone(),
            value,
            edit: LevelEdit::Brightness,
        });
    }
    if let (Some(range), Some(value)) = (&effect.speed, setting.speed) {
        settings.push(Control::Level {
            label: "Speed",
            range: range.clone(),
            value,
            edit: LevelEdit::Speed,
        });
    }
    if !effect.options.is_empty() {
        settings.push(Control::Choices {
            label: "Option",
            choices: effect
                .options
                .iter()
                .map(|choice| ChoiceEdit {
                    label: choice.label.clone(),
                    selected: setting.option.as_deref() == Some(choice.id.as_str()),
                    edit: Edit::Option(choice.id.clone()),
                })
                .collect(),
        });
    }
    if matches!(effect.color, Some(ColorCapability::FixedOrRainbow)) {
        settings.push(Control::Choices {
            label: "Color mode",
            choices: vec![
                ChoiceEdit {
                    label: "Fixed".into(),
                    selected: matches!(setting.color, Some(Color::Rgb(_))),
                    edit: Edit::Color(Color::Rgb([255; 3])),
                },
                ChoiceEdit {
                    label: "Rainbow".into(),
                    selected: matches!(setting.color, Some(Color::Rainbow)),
                    edit: Edit::Color(Color::Rainbow),
                },
            ],
        });
    }
    if let Some(Color::Rgb(rgb)) = setting.color {
        for (label, channel, value) in [
            ("Red", Channel::Red, rgb[0]),
            ("Green", Channel::Green, rgb[1]),
            ("Blue", Channel::Blue, rgb[2]),
        ] {
            settings.push(Control::Level {
                label,
                range: 0..=255,
                value: value.into(),
                edit: LevelEdit::Channel(channel),
            });
        }
    }
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::{Choice, Effect, edit};

    fn caps() -> Capabilities {
        Capabilities {
            backend_id: "synthetic".into(),
            host_modes: vec![],
            effects: vec![
                Effect {
                    id: "static".into(),
                    label: "Static".into(),
                    brightness: Some(1..=10),
                    speed: None,
                    options: vec![],
                    color: Some(ColorCapability::Fixed),
                },
                Effect {
                    id: "wave".into(),
                    label: "Wave".into(),
                    brightness: None,
                    speed: Some(2..=8),
                    options: vec![
                        Choice {
                            id: "left".into(),
                            label: "Left".into(),
                        },
                        Choice {
                            id: "right".into(),
                            label: "Right".into(),
                        },
                    ],
                    color: Some(ColorCapability::Rainbow),
                },
                Effect {
                    id: "dual".into(),
                    label: "Dual".into(),
                    brightness: Some(0..=20),
                    speed: Some(0..=5),
                    options: vec![],
                    color: Some(ColorCapability::FixedOrRainbow),
                },
            ],
        }
    }

    #[test]
    fn projects_only_advertised_controls_for_three_effect_shapes() {
        let caps = caps();
        let static_controls = controls(
            &caps,
            &Setting {
                effect: "static".into(),
                brightness: Some(7),
                speed: None,
                option: None,
                color: Some(Color::Rgb([1, 2, 3])),
            },
        )
        .unwrap();
        assert_eq!(static_controls.effects.len(), 3);
        assert_eq!(
            static_controls
                .settings
                .iter()
                .filter(|control| matches!(control, Control::Level { .. }))
                .count(),
            4
        );

        let wave_controls = controls(
            &caps,
            &Setting {
                effect: "wave".into(),
                brightness: None,
                speed: Some(4),
                option: Some("left".into()),
                color: Some(Color::Rainbow),
            },
        )
        .unwrap();
        assert!(wave_controls.settings.iter().any(|control| matches!(control, Control::Choices { label: "Option", choices } if choices.len() == 2)));
        assert!(!wave_controls.settings.iter().any(|control| matches!(
            control,
            Control::Level {
                label: "Brightness",
                ..
            }
        )));

        let dual_controls = controls(
            &caps,
            &Setting {
                effect: "dual".into(),
                brightness: Some(12),
                speed: Some(3),
                option: None,
                color: Some(Color::Rainbow),
            },
        )
        .unwrap();
        assert!(dual_controls.settings.iter().any(|control| matches!(control, Control::Choices { label: "Color mode", choices } if choices.len() == 2)));
        assert_eq!(
            dual_controls
                .settings
                .iter()
                .filter(|control| matches!(control, Control::Level { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn generated_edits_preserve_unedited_fields() {
        let caps = caps();
        let current = Setting {
            effect: "dual".into(),
            brightness: Some(12),
            speed: Some(3),
            option: None,
            color: Some(Color::Rgb([1, 2, 3])),
        };
        let projected = controls(&caps, &current).unwrap();
        let brightness = projected
            .settings
            .iter()
            .find_map(|control| match control {
                Control::Level {
                    label: "Brightness",
                    edit,
                    ..
                } => Some(*edit),
                _ => None,
            })
            .unwrap();
        let changed = edit(&caps, &current, brightness.edit(9).unwrap()).unwrap();
        assert_eq!(
            changed,
            Setting {
                brightness: Some(9),
                ..current.clone()
            }
        );

        let blue = projected
            .settings
            .iter()
            .find_map(|control| match control {
                Control::Level {
                    label: "Blue",
                    edit,
                    ..
                } => Some(*edit),
                _ => None,
            })
            .unwrap();
        let changed = edit(&caps, &current, blue.edit(200).unwrap()).unwrap();
        assert_eq!(
            changed,
            Setting {
                color: Some(Color::Rgb([1, 2, 200])),
                ..current
            }
        );
    }

    #[test]
    fn rejects_invalid_settings_before_projection() {
        let invalid = Setting {
            effect: "static".into(),
            brightness: Some(99),
            speed: None,
            option: None,
            color: Some(Color::Rgb([1, 2, 3])),
        };
        assert_eq!(
            controls(&caps(), &invalid),
            Err("Invalid lighting brightness".into())
        );
    }

    #[test]
    fn host_parameters_use_the_same_control_projection() {
        let schema = Effect {
            id: "audio_parameters".into(),
            label: "Audio parameters".into(),
            brightness: Some(1..=9),
            speed: None,
            options: vec![Choice {
                id: "wide".into(),
                label: "Wide".into(),
            }],
            color: Some(ColorCapability::Fixed),
        };
        let setting = Setting {
            effect: schema.id.clone(),
            brightness: Some(6),
            speed: None,
            option: Some("wide".into()),
            color: Some(Color::Rgb([1, 2, 3])),
        };
        let controls = parameter_controls(&schema, &setting).unwrap();
        assert!(controls.iter().any(|control| matches!(
            control,
            Control::Level {
                label: "Brightness",
                value: 6,
                ..
            }
        )));
        assert!(controls.iter().any(|control| matches!(control, Control::Choices { label: "Option", choices } if choices.len() == 1)));
        assert_eq!(
            controls
                .iter()
                .filter(|control| matches!(control, Control::Level { .. }))
                .count(),
            4
        );
        assert!(
            parameter_controls(
                &schema,
                &Setting {
                    brightness: Some(10),
                    ..setting
                }
            )
            .is_err()
        );
        let mut catalog = caps();
        catalog.host_modes.push(crate::lighting::HostMode {
            id: "audio".into(),
            label: "Audio".into(),
            source: crate::lighting::HostSource::PlaybackAudio { bands: 32 },
            parameters: Some(crate::lighting::HostParameters {
                schema,
                default: Setting {
                    effect: "audio_parameters".into(),
                    brightness: Some(10),
                    speed: None,
                    option: Some("wide".into()),
                    color: Some(Color::Rgb([1, 2, 3])),
                },
            }),
        });
        assert_eq!(
            crate::lighting::validate_capabilities(&catalog),
            Err("Invalid lighting brightness".into())
        );
    }
}
