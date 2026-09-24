//! Transient widget text, separate from the session's validated program.
use byakko_core::macros::{Action, Capabilities, Event};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Key,
    Button { id: u16, label: String },
    Move,
    Backend { id: String, label: String },
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Key => "Keyboard key",
            Self::Move => "Pointer movement",
            Self::Button { label, .. } | Self::Backend { label, .. } => label,
        })
    }
}

#[derive(Clone, Debug)]
pub(super) enum Input {
    Kind(Kind),
    First(String),
    Second(String),
    Wait(String),
    Pressed(bool),
}

#[derive(Default)]
pub(super) struct Form {
    pub kind: Option<Kind>,
    pub first: String,
    pub second: String,
    pub wait: String,
    pub pressed: bool,
    pub target: Option<usize>,
}

pub(super) fn kinds(caps: &Capabilities) -> Vec<Kind> {
    caps.keys
        .as_ref()
        .map(|_| Kind::Key)
        .into_iter()
        .chain(caps.buttons.iter().map(|choice| Kind::Button {
            id: choice.button,
            label: choice.label.clone(),
        }))
        .chain(caps.movement.as_ref().map(|_| Kind::Move))
        .chain(caps.backend_actions.iter().map(|choice| Kind::Backend {
            id: choice.id.clone(),
            label: choice.label.clone(),
        }))
        .collect()
}

impl Form {
    pub fn update(&mut self, input: Input) {
        match input {
            Input::Kind(kind) => self.kind = Some(kind),
            Input::First(value) => self.first = value,
            Input::Second(value) => self.second = value,
            Input::Wait(value) => self.wait = value,
            Input::Pressed(value) => self.pressed = value,
        }
    }

    pub fn event(&self, caps: &Capabilities) -> Result<Event, String> {
        let action = match self.kind.as_ref().ok_or("Choose an event type")? {
            Kind::Key => Action::Key {
                usage: number(&self.first, "Keyboard usage")?,
                pressed: self.pressed,
            },
            Kind::Button { id, .. } => Action::Button {
                button: *id,
                pressed: self.pressed,
            },
            Kind::Move => Action::Move {
                dx: number(&self.first, "Horizontal movement")?,
                dy: number(&self.second, "Vertical movement")?,
            },
            Kind::Backend { id, .. } => Action::Backend {
                backend_id: caps.backend_id.clone(),
                id: id.clone(),
                pressed: self.pressed,
            },
        };
        Ok(Event {
            action,
            delay_ms: number(&self.wait, "Wait after event")?,
        })
    }

    pub fn from_event(index: usize, event: &Event, caps: &Capabilities) -> Self {
        let (kind, first, second, pressed) = match &event.action {
            Action::Key { usage, pressed } => {
                (Kind::Key, usage.to_string(), String::new(), *pressed)
            }
            Action::Button { button, pressed } => (
                Kind::Button {
                    id: *button,
                    label: caps
                        .buttons
                        .iter()
                        .find(|c| c.button == *button)
                        .map_or_else(|| button.to_string(), |c| c.label.clone()),
                },
                String::new(),
                String::new(),
                *pressed,
            ),
            Action::Move { dx, dy } => (Kind::Move, dx.to_string(), dy.to_string(), false),
            Action::Backend { id, pressed, .. } => (
                Kind::Backend {
                    id: id.clone(),
                    label: caps
                        .backend_actions
                        .iter()
                        .find(|c| c.id == *id)
                        .map_or_else(|| id.clone(), |c| c.label.clone()),
                },
                String::new(),
                String::new(),
                *pressed,
            ),
        };
        Self {
            kind: Some(kind),
            first,
            second,
            wait: event.delay_ms.to_string(),
            pressed,
            target: Some(index),
        }
    }
}

pub(super) fn number<T: std::str::FromStr>(value: &str, field: &str) -> Result<T, String> {
    value
        .trim()
        .parse()
        .map_err(|_| format!("{field}: enter a whole number"))
}
