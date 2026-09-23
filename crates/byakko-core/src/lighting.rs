//! Device-neutral lighting catalog and snapshot values.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, ops::RangeInclusive};

pub mod controls;
pub mod editor;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub id: String,
    pub label: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Color {
    Rgb([u8; 3]),
    Rainbow,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ColorCapability {
    Fixed,
    Rainbow,
    FixedOrRainbow,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub id: String,
    pub label: String,
    pub brightness: Option<RangeInclusive<u16>>,
    pub speed: Option<RangeInclusive<u16>>,
    pub options: Vec<Choice>,
    pub color: Option<ColorCapability>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum HostSource {
    ScreenAverage,
    PlaybackAudio { bands: u8 },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostMode {
    pub id: String,
    pub label: String,
    pub source: HostSource,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub backend_id: String,
    pub effects: Vec<Effect>,
    pub host_modes: Vec<HostMode>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Setting {
    pub effect: String,
    pub brightness: Option<u16>,
    pub speed: Option<u16>,
    pub option: Option<String>,
    pub color: Option<Color>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Channel {
    Red,
    Green,
    Blue,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Edit {
    Effect(String),
    Brightness(u16),
    Speed(u16),
    Option(String),
    Color(Color),
    Channel(Channel, u8),
}

pub fn edit(caps: &Capabilities, current: &Setting, change: Edit) -> Result<Setting, String> {
    validate_setting(caps, current)?;
    let mut next = current.clone();
    match change {
        Edit::Effect(id) if id != current.effect => return default_setting(caps, &id),
        Edit::Effect(_) => {}
        Edit::Brightness(value) => next.brightness = Some(value),
        Edit::Speed(value) => next.speed = Some(value),
        Edit::Option(id) => next.option = Some(id),
        Edit::Color(color) => next.color = Some(color),
        Edit::Channel(channel, value) => {
            let Some(Color::Rgb(rgb)) = next.color.as_mut() else {
                return Err("Lighting color is not fixed RGB".into());
            };
            rgb[match channel {
                Channel::Red => 0,
                Channel::Green => 1,
                Channel::Blue => 2,
            }] = value;
        }
    }
    validate_setting(caps, &next)?;
    Ok(next)
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Content {
    Editable(Setting),
    Opaque { reason: String },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub backend_id: String,
    pub revision: Vec<u8>,
    pub content: Content,
}

pub fn validate_capabilities(caps: &Capabilities) -> Result<(), String> {
    if caps.backend_id.is_empty() || caps.effects.is_empty() {
        return Err("Lighting catalog is empty".into());
    }
    let mut ids = BTreeSet::new();
    for effect in &caps.effects {
        if effect.id.is_empty() || !ids.insert(&effect.id) {
            return Err("Invalid lighting effect ID".into());
        }
        if effect
            .brightness
            .as_ref()
            .is_some_and(|range| range.is_empty())
            || effect.speed.as_ref().is_some_and(|range| range.is_empty())
        {
            return Err("Invalid lighting range".into());
        }
        let mut options = BTreeSet::new();
        for choice in &effect.options {
            if choice.id.is_empty() || !options.insert(&choice.id) {
                return Err("Invalid lighting option ID".into());
            }
        }
    }
    for mode in &caps.host_modes {
        if mode.id.is_empty() || mode.label.is_empty() || !ids.insert(&mode.id) {
            return Err("Invalid host lighting mode ID or label".into());
        }
        if matches!(mode.source, HostSource::PlaybackAudio { bands: 0 }) {
            return Err("Audio host mode requires a positive band count".into());
        }
    }
    Ok(())
}

pub fn validate_setting(caps: &Capabilities, setting: &Setting) -> Result<(), String> {
    validate_capabilities(caps)?;
    let effect = caps
        .effects
        .iter()
        .find(|effect| effect.id == setting.effect)
        .ok_or("Unknown lighting effect")?;
    if !matches!((&effect.brightness, setting.brightness), (None, None))
        && !matches!((&effect.brightness, setting.brightness), (Some(range), Some(value)) if range.contains(&value))
    {
        return Err("Invalid lighting brightness".into());
    }
    if !matches!((&effect.speed, setting.speed), (None, None))
        && !matches!((&effect.speed, setting.speed), (Some(range), Some(value)) if range.contains(&value))
    {
        return Err("Invalid lighting speed".into());
    }
    match &setting.option {
        None if effect.options.is_empty() => {}
        Some(id) if effect.options.iter().any(|choice| &choice.id == id) => {}
        _ => return Err("Invalid lighting option".into()),
    }
    match (&effect.color, &setting.color) {
        (None, None)
        | (Some(ColorCapability::Fixed), Some(Color::Rgb(_)))
        | (Some(ColorCapability::Rainbow), Some(Color::Rainbow))
        | (Some(ColorCapability::FixedOrRainbow), Some(_)) => Ok(()),
        _ => Err("Invalid lighting color".into()),
    }
}

pub fn validate_snapshot(caps: &Capabilities, snapshot: &Snapshot) -> Result<(), String> {
    validate_capabilities(caps)?;
    if snapshot.backend_id != caps.backend_id {
        return Err("Lighting result belongs to a different backend".into());
    }
    if let Content::Editable(setting) = &snapshot.content {
        validate_setting(caps, setting)?;
    }
    Ok(())
}

pub fn default_setting(caps: &Capabilities, effect_id: &str) -> Result<Setting, String> {
    validate_capabilities(caps)?;
    let effect = caps
        .effects
        .iter()
        .find(|effect| effect.id == effect_id)
        .ok_or("Unknown lighting effect")?;
    Ok(Setting {
        effect: effect.id.clone(),
        brightness: effect.brightness.as_ref().map(|range| *range.end()),
        speed: effect.speed.as_ref().map(|range| *range.start()),
        option: effect.options.first().map(|choice| choice.id.clone()),
        color: effect.color.as_ref().map(|capability| match capability {
            ColorCapability::Rainbow => Color::Rainbow,
            _ => Color::Rgb([255; 3]),
        }),
    })
}
