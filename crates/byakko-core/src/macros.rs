//! Macro values and atomic draft edits. Encoded capacity and wire formats belong to backends.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, ops::RangeInclusive};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Program {
    /// Stored count. A backend must describe special values such as zero;
    /// zero is not universally interpreted as infinite playback.
    pub repeat_count: u32,
    pub events: Vec<Event>,
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
pub struct Capabilities {
    pub backend_id: String,
    pub slots: Vec<Choice>,
    pub repeat_counts: RangeInclusive<u32>,
    pub delays_ms: RangeInclusive<u32>,
    pub keys: Option<RangeInclusive<u16>>,
    pub buttons: Vec<ButtonChoice>,
    pub movement: Option<RangeInclusive<i32>>,
    pub backend_actions: Vec<Choice>,
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

fn valid_choices(choices: &[Choice]) -> bool {
    let ids: BTreeSet<_> = choices.iter().map(|choice| choice.id.as_str()).collect();
    ids.len() == choices.len() && !ids.contains("")
}

pub fn validate_capabilities(capabilities: &Capabilities) -> Result<(), String> {
    if capabilities.backend_id.is_empty()
        || capabilities.slots.is_empty()
        || !valid_choices(&capabilities.slots)
        || !valid_choices(&capabilities.backend_actions)
        || capabilities.repeat_counts.is_empty()
        || capabilities.delays_ms.is_empty()
        || capabilities
            .keys
            .as_ref()
            .is_some_and(RangeInclusive::is_empty)
        || capabilities
            .movement
            .as_ref()
            .is_some_and(RangeInclusive::is_empty)
    {
        return Err("Invalid macro capability IDs or ranges".into());
    }
    let buttons: BTreeSet<_> = capabilities
        .buttons
        .iter()
        .map(|choice| choice.button)
        .collect();
    if buttons.len() != capabilities.buttons.len() || buttons.contains(&0) {
        return Err("Invalid macro pointer button capabilities".into());
    }
    Ok(())
}

pub fn validate_program(capabilities: &Capabilities, program: &Program) -> Result<(), String> {
    validate_capabilities(capabilities)?;
    if !capabilities.repeat_counts.contains(&program.repeat_count) {
        return Err("Macro repeat count is outside backend limits".into());
    }
    for (index, event) in program.events.iter().enumerate() {
        if !capabilities.delays_ms.contains(&event.delay_ms) {
            return Err(format!("Event {index}: wait is outside backend limits"));
        }
        let supported = match &event.action {
            Action::Key { usage, .. } => capabilities
                .keys
                .as_ref()
                .is_some_and(|range| range.contains(usage)),
            Action::Button { button, .. } => capabilities
                .buttons
                .iter()
                .any(|choice| choice.button == *button),
            Action::Move { dx, dy } => capabilities
                .movement
                .as_ref()
                .is_some_and(|range| range.contains(dx) && range.contains(dy)),
            Action::Backend { backend_id, id, .. } => {
                backend_id == &capabilities.backend_id
                    && capabilities
                        .backend_actions
                        .iter()
                        .any(|choice| choice.id == *id)
            }
        };
        if !supported {
            return Err(format!(
                "Event {index}: action is unsupported by this backend"
            ));
        }
    }
    Ok(())
}

/// Rejection leaves the caller's draft untouched. Backend encoding validation
/// still runs before accepting/storing a draft that has a variable byte cost.
pub fn edit(capabilities: &Capabilities, program: &Program, edit: Edit) -> Result<Program, String> {
    let mut next = program.clone();
    match edit {
        Edit::Insert { at, event } if at <= next.events.len() => next.events.insert(at, event),
        Edit::Replace { at, event } if at < next.events.len() => next.events[at] = event,
        Edit::Remove { at } if at < next.events.len() => {
            next.events.remove(at);
        }
        Edit::Move { from, to } if from < next.events.len() && to < next.events.len() => {
            let event = next.events.remove(from);
            next.events.insert(to, event);
        }
        Edit::Repeat(count) => next.repeat_count = count,
        Edit::Clear => next.events.clear(),
        _ => return Err("Macro edit index is outside the event sequence".into()),
    }
    validate_program(capabilities, &next)?;
    Ok(next)
}

#[cfg(test)]
mod tests;
