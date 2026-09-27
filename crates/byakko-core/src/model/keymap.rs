//! Device-neutral keymap values and validation. No platform or I/O dependencies.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: String,
    pub label: String,
    /// Physical positions reserved for onboard commands on this layer.
    #[serde(default)]
    pub read_only_keys: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub enum ActionCategory {
    Alphanumeric,
    Modifiers,
    Navigation,
    Function,
    Numpad,
    Media,
    Mouse,
    System,
    Shortcuts,
    #[default]
    Other,
}

impl ActionCategory {
    /// Classify USB HID keyboard-page usages without depending on a display label.
    pub const fn for_keyboard_usage(usage: u16) -> Self {
        match usage {
            0x04..=0x27 | 0x2c..=0x38 | 0x64 => Self::Alphanumeric,
            0x39 | 0xe0..=0xe7 => Self::Modifiers,
            0x28..=0x2b | 0x49..=0x52 => Self::Navigation,
            0x3a..=0x45 | 0x68..=0x73 => Self::Function,
            0x53..=0x63 | 0x67 => Self::Numpad,
            0x46..=0x48 | 0x66 => Self::System,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActionChoice {
    pub label: String,
    pub action: Action,
    #[serde(default)]
    pub category: ActionCategory,
}

#[cfg(test)]
mod action_category_tests {
    use super::{ActionCategory as Category, ActionChoice};

    #[test]
    fn keyboard_usage_groups_are_semantic() {
        for (usage, expected) in [
            (0x04, Category::Alphanumeric),
            (0x39, Category::Modifiers),
            (0x4f, Category::Navigation),
            (0x3a, Category::Function),
            (0x68, Category::Function),
            (0x59, Category::Numpad),
            (0xe7, Category::Modifiers),
            (0xff, Category::Other),
        ] {
            assert_eq!(Category::for_keyboard_usage(usage), expected);
        }
    }

    #[test]
    fn old_action_choices_without_category_remain_readable() {
        let choice: ActionChoice =
            serde_json::from_str(r#"{"label":"A","action":{"Key":4}}"#).unwrap();
        assert_eq!(choice.category, Category::Other);
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UsageChoice {
    pub label: String,
    pub usage: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ShortcutCapabilities {
    pub modifiers: Vec<UsageChoice>,
    pub keys: Vec<UsageChoice>,
    pub min_modifiers: usize,
    pub max_modifiers: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Descriptor {
    pub backend_id: String,
    pub device_name: String,
    pub keys: Vec<PhysicalKey>,
    pub layers: Vec<Layer>,
    pub actions: Vec<ActionChoice>,
    #[serde(default)]
    pub shortcuts: Option<ShortcutCapabilities>,
}

impl Descriptor {
    pub fn key_is_writable(&self, layer: &str, key: &str) -> bool {
        self.keys
            .iter()
            .any(|entry| entry.id == key && entry.writable)
            && self
                .layers
                .iter()
                .any(|entry| entry.id == layer && !entry.read_only_keys.iter().any(|id| id == key))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// Opaque backend-owned snapshot token; callers must retain it unchanged.
    pub revision: Vec<u8>,
    pub bindings: BTreeMap<String, BTreeMap<String, Action>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub layer: String,
    pub key: String,
    pub action: Action,
}

pub type Bindings = BTreeMap<String, BTreeMap<String, Action>>;
