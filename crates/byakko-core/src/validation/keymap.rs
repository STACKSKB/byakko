use crate::model::keymap::*;
use std::collections::{BTreeMap, BTreeSet};
impl ShortcutCapabilities {
    pub fn validate(&self) -> Result<(), String> {
        fn valid_choices(choices: &[UsageChoice]) -> bool {
            let mut usages = BTreeSet::new();
            let mut labels = BTreeSet::new();
            !choices.is_empty()
                && choices.iter().all(|choice| {
                    choice.usage != 0
                        && !choice.label.trim().is_empty()
                        && usages.insert(choice.usage)
                        && labels.insert(choice.label.as_str())
                })
        }
        if !valid_choices(&self.modifiers) || !valid_choices(&self.keys) {
            return Err("Shortcut choices must be nonempty with unique usages and labels".into());
        }
        if self.min_modifiers == 0
            || self.max_modifiers < self.min_modifiers
            || self.max_modifiers > self.modifiers.len()
        {
            return Err("Shortcut modifier bounds are invalid".into());
        }
        Ok(())
    }

    pub fn compose(&self, modifiers: &[u16], key: u16) -> Result<Action, String> {
        self.validate()?;
        if modifiers.len() < self.min_modifiers || modifiers.len() > self.max_modifiers {
            return Err("Shortcut modifier count is outside the advertised range".into());
        }
        let mut seen = BTreeSet::new();
        if modifiers.iter().any(|usage| {
            !seen.insert(*usage) || !self.modifiers.iter().any(|choice| choice.usage == *usage)
        }) {
            return Err("Shortcut has a duplicate or unadvertised modifier".into());
        }
        if !self.keys.iter().any(|choice| choice.usage == key) {
            return Err("Shortcut target key is not advertised".into());
        }
        Ok(Action::Shortcut {
            modifiers: modifiers.to_vec(),
            key,
        })
    }
}

/// Check the identity and shape of a complete state before presenting or editing it.
pub fn validate_state(descriptor: &Descriptor, state: &State) -> Result<(), String> {
    if let Some(shortcuts) = &descriptor.shortcuts {
        shortcuts.validate()?;
    }
    let layers: BTreeSet<_> = descriptor.layers.iter().map(|l| l.id.as_str()).collect();
    let keys: BTreeSet<_> = descriptor.keys.iter().map(|k| k.id.as_str()).collect();
    if descriptor.backend_id.is_empty()
        || layers.is_empty()
        || keys.is_empty()
        || layers.len() != descriptor.layers.len()
        || keys.len() != descriptor.keys.len()
        || layers.contains("")
        || keys.contains("")
    {
        return Err("Invalid backend descriptor IDs".into());
    }
    if descriptor.keys.iter().any(|key| {
        key.visible
            && (!key.x.is_finite()
                || !key.y.is_finite()
                || !key.width.is_finite()
                || !key.height.is_finite()
                || key.width <= 0.0
                || key.height <= 0.0)
    }) {
        return Err("Invalid visible key geometry".into());
    }
    for layer in &descriptor.layers {
        let protected: BTreeSet<_> = layer.read_only_keys.iter().collect();
        if protected.len() != layer.read_only_keys.len()
            || protected.iter().any(|key| !keys.contains(key.as_str()))
        {
            return Err("Layer has duplicate or unknown read-only keys".into());
        }
    }
    if state.bindings.len() != layers.len()
        || state
            .bindings
            .keys()
            .any(|id| !layers.contains(id.as_str()))
    {
        return Err("State has missing or unknown layers".into());
    }
    for bindings in state.bindings.values() {
        if bindings.len() != keys.len() || bindings.keys().any(|id| !keys.contains(id.as_str())) {
            return Err("State has missing or unknown keys".into());
        }
    }
    Ok(())
}

pub fn validate_changes(descriptor: &Descriptor, changes: &[Change]) -> Result<(), String> {
    let layers: BTreeSet<_> = descriptor.layers.iter().map(|l| l.id.as_str()).collect();
    let keys: BTreeMap<_, _> = descriptor
        .keys
        .iter()
        .map(|k| (k.id.as_str(), k.writable))
        .collect();
    let mut seen = BTreeSet::new();
    for change in changes {
        if !layers.contains(change.layer.as_str()) || !keys.contains_key(change.key.as_str()) {
            return Err(format!(
                "Unknown layer/key: {}/{}",
                change.layer, change.key
            ));
        }
        if !descriptor.key_is_writable(&change.layer, &change.key) {
            return Err("This key is reserved for an onboard keyboard command".into());
        }
        if !seen.insert((&change.layer, &change.key)) {
            return Err("Duplicate key change".into());
        }
    }
    Ok(())
}

/// Validate a portable key edit against the backend's advertised constraints.
pub fn validate_edit(descriptor: &Descriptor, change: &Change) -> Result<(), String> {
    validate_changes(descriptor, std::slice::from_ref(change))?;
    match &change.action {
        Action::Opaque { .. } => return Err("Opaque bindings cannot be programmed".into()),
        Action::Shortcut { modifiers, key } => {
            descriptor
                .shortcuts
                .as_ref()
                .ok_or("Shortcuts are not supported")?
                .compose(modifiers, *key)?;
        }
        action
            if !descriptor
                .actions
                .iter()
                .any(|choice| &choice.action == action) =>
        {
            return Err("Action is not advertised".into());
        }
        _ => {}
    }
    Ok(())
}
