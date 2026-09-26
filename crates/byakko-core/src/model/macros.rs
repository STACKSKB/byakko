//! Macro values and atomic draft edits. Encoded capacity and wire formats belong to backends.
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Program {
    /// Stored count. A backend must describe special values such as zero;
    /// zero is not universally interpreted as infinite playback.
    pub repeat_count: u32,
    pub events: Vec<Event>,
}

/// Portable file contents; source identity and binding are metadata, not write targets.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub format_version: u32,
    pub backend_id: String,
    pub source_slot: String,
    pub name: String,
    pub binding: Option<String>,
    pub program: Program,
}

/// Informational labels from an imported document; never a write target or binding action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentMetadata {
    pub name: String,
    pub binding: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub action: Action,
    /// Wait after this action, not before it. Explicit zero is preserved.
    pub delay_ms: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Key {
        usage: u16,
        pressed: bool,
    },
    /// HID pointer button usage; never a firmware action byte.
    Button {
        button: u16,
        pressed: bool,
    },
    Move {
        dx: i32,
        dy: i32,
    },
    /// Backend-local semantics, e.g. a wheel action with a firmware edge flag.
    Backend {
        backend_id: String,
        id: String,
        pressed: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ButtonChoice {
    pub button: u16,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub slot: String,
    pub id: String,
    pub label: String,
    pub action: crate::model::keymap::Action,
    pub required_repeat_count: Option<u32>,
}

/// Additive encoded-size model supplied by a backend. The backend codec remains
/// responsible for final encoding and validation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ByteBudget {
    pub limit: u32,
    pub overhead: u32,
    pub key: u32,
    pub button: u32,
    pub movement: u32,
    pub backend: u32,
    pub inline_delays: RangeInclusive<u32>,
    pub extended_delay: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub backend_id: String,
    pub slots: Vec<Choice>,
    pub repeat_counts: RangeInclusive<u32>,
    /// Counts the editor may newly stage or save. The storage range above may
    /// include legacy values whose playback meaning has not been established.
    pub editable_repeat_counts: RangeInclusive<u32>,
    pub delays_ms: RangeInclusive<u32>,
    pub keys: Option<RangeInclusive<u16>>,
    pub buttons: Vec<ButtonChoice>,
    pub movement: Option<RangeInclusive<i32>>,
    pub backend_actions: Vec<Choice>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
    #[serde(default)]
    pub byte_budget: Option<ByteBudget>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub backend_id: String,
    pub slot: String,
    /// Exact backend-owned before-image, including unrecognized bytes.
    pub revision: Vec<u8>,
    pub content: Content,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Content {
    Editable(Program),
    /// Preserve for inspection/export; do not offer editing without a safe codec.
    Opaque {
        reason: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Edit {
    Insert {
        at: usize,
        event: Event,
    },
    Replace {
        at: usize,
        event: Event,
    },
    Remove {
        at: usize,
    },
    /// Destination index in the final sequence.
    Move {
        from: usize,
        to: usize,
    },
    Repeat(u32),
    Clear,
}

#[cfg(test)]
#[path = "../tests/model_macros.rs"]
mod tests;
