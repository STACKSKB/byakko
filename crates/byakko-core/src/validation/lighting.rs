use crate::model::lighting::*;
use std::collections::BTreeSet;
pub fn validate_capabilities(caps: &Capabilities) -> Result<(), String> {
    if caps.backend_id.is_empty() || caps.effects.is_empty() {
        return Err("Lighting catalog is empty".into());
    }
    let mut ids = BTreeSet::new();
    for effect in &caps.effects {
        if effect.id.is_empty() || !ids.insert(&effect.id) {
            return Err("Invalid lighting effect ID".into());
        }
        validate_effect(effect)?;
    }
    for mode in &caps.host_modes {
        if mode.id.is_empty() || mode.label.is_empty() || !ids.insert(&mode.id) {
            return Err("Invalid host lighting mode ID or label".into());
        }
        if matches!(mode.source, HostSource::PlaybackAudio { bands: 0 }) {
            return Err("Audio host mode requires a positive band count".into());
        }
        if let Some(parameters) = &mode.parameters {
            validate_effect(&parameters.schema)?;
            validate_parameters(&parameters.schema, &parameters.default)?;
        }
    }
    Ok(())
}

fn validate_effect(effect: &Effect) -> Result<(), String> {
    if effect.id.is_empty() {
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
    Ok(())
}

pub fn validate_setting(caps: &Capabilities, setting: &Setting) -> Result<(), String> {
    validate_capabilities(caps)?;
    let effect = caps
        .effects
        .iter()
        .find(|effect| effect.id == setting.effect)
        .ok_or("Unknown lighting effect")?;
    validate_parameters(effect, setting)
}

pub fn validate_parameters(effect: &Effect, setting: &Setting) -> Result<(), String> {
    validate_effect(effect)?;
    if effect.id != setting.effect {
        return Err("Lighting parameter schema does not match setting".into());
    }
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
    match &snapshot.content {
        Content::Editable(setting) => validate_setting(caps, setting)?,
        Content::HostActive { mode_id }
            if caps.host_modes.iter().any(|mode| &mode.id == mode_id) => {}
        Content::HostActive { .. } => return Err("Unknown active host lighting mode".into()),
        Content::Opaque { .. } => {}
    }
    Ok(())
}
