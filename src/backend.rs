//! Backend-independent keymap data and editing contract.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub mod nia87;

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalKey {
    pub id: String,
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub visible: bool,
    pub writable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layer {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    /// USB HID keyboard usage; not a physical position or firmware keycode.
    Key(u16),
    Disabled,
    /// Backend-local slot and playback mode. Not portable profile identifiers.
    Macro {
        slot: u16,
        mode: u8,
    },
    Shortcut {
        modifiers: Vec<u16>,
        key: u16,
    },
    /// Backend-local action ID from its advertised catalog.
    Named {
        id: String,
    },
    Opaque {
        backend_id: String,
        data: Vec<u8>,
        label: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionChoice {
    pub label: String,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Descriptor {
    pub backend_id: String,
    pub device_name: String,
    pub keys: Vec<PhysicalKey>,
    pub layers: Vec<Layer>,
    pub actions: Vec<ActionChoice>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    /// Opaque backend-owned snapshot token; callers must retain it unchanged.
    pub revision: Vec<u8>,
    pub bindings: BTreeMap<String, BTreeMap<String, Action>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Change {
    pub layer: String,
    pub key: String,
    pub action: Action,
}

pub trait KeymapBackend: Send + Sync {
    fn descriptor(&self) -> Descriptor;
    fn read(&self) -> Result<State, String>;
    fn validate(&self, expected: &State, changes: &[Change]) -> Result<(), String> {
        let descriptor = self.descriptor();
        validate_state(&descriptor, expected)?;
        validate_changes(&descriptor, changes)
    }
    fn apply(
        &self,
        expected: &State,
        changes: &[Change],
        backup_dir: &Path,
    ) -> Result<State, String>;
}

/// Check the identity and shape of a complete state before presenting or editing it.
pub fn validate_state(descriptor: &Descriptor, state: &State) -> Result<(), String> {
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
        if !keys[change.key.as_str()] {
            return Err(format!("Key {} is read-only", change.key));
        }
        if !seen.insert((&change.layer, &change.key)) {
            return Err("Duplicate key change".into());
        }
    }
    Ok(())
}
