//! Unsubmitted macro fields. Valid programs and library facts belong to core.
use byakko_core::{
    editor::{Editor, macros::MacroRules},
    model::macros::{Action, Edit, Event, Program},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Key,
    Button,
    Movement,
    Backend,
}
impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Key => "Key",
            Self::Button => "Button",
            Self::Movement => "Movement",
            Self::Backend => "Device action",
        })
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    Select(String),
    Add,
    Read,
    Save,
    Revert,
    Assign(String),
    Repeat(String),
    ApplyRepeat,
    Kind(Kind),
    Value(String),
    Second(String),
    Delay(String),
    Pressed(bool),
    Event(usize),
    Insert,
    Replace,
    Remove(usize),
    Move { from: usize, to: usize },
    Clear,
}

pub struct Form {
    pub(crate) repeat: String,
    pub(crate) kind: Kind,
    pub(crate) value: String,
    pub(crate) second: String,
    pub(crate) delay: String,
    pub(crate) pressed: bool,
    pub(crate) selected: Option<usize>,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            repeat: "1".into(),
            kind: Kind::Key,
            value: "4".into(),
            second: "0".into(),
            delay: "0".into(),
            pressed: true,
            selected: None,
        }
    }
}

impl Form {
    pub fn sync(&mut self, program: Option<&Program>) {
        self.repeat =
            program.map_or_else(|| "1".into(), |program| program.repeat_count.to_string());
        self.selected = None;
    }

    pub fn update(
        &mut self,
        message: Message,
        editor: &Editor<MacroRules>,
    ) -> Result<Option<Edit>, String> {
        let edit = match message {
            Message::Repeat(value) => {
                self.repeat = value;
                None
            }
            Message::ApplyRepeat => Some(Edit::Repeat(
                self.repeat
                    .parse()
                    .map_err(|_| "Enter a whole repeat count")?,
            )),
            Message::Kind(kind) => {
                self.kind = kind;
                None
            }
            Message::Value(value) => {
                self.value = value;
                None
            }
            Message::Second(value) => {
                self.second = value;
                None
            }
            Message::Delay(value) => {
                self.delay = value;
                None
            }
            Message::Pressed(pressed) => {
                self.pressed = pressed;
                None
            }
            Message::Event(index) => {
                let event = editor
                    .draft()
                    .and_then(|draft| draft.events.get(index))
                    .ok_or("Select an existing event")?;
                self.selected = Some(index);
                self.delay = event.delay_ms.to_string();
                match &event.action {
                    Action::Key { usage, pressed } => {
                        self.kind = Kind::Key;
                        self.value = usage.to_string();
                        self.pressed = *pressed;
                    }
                    Action::Button { button, pressed } => {
                        self.kind = Kind::Button;
                        self.value = button.to_string();
                        self.pressed = *pressed;
                    }
                    Action::Move { dx, dy } => {
                        self.kind = Kind::Movement;
                        self.value = dx.to_string();
                        self.second = dy.to_string();
                    }
                    Action::Backend { id, pressed, .. } => {
                        self.kind = Kind::Backend;
                        self.value = id.clone();
                        self.pressed = *pressed;
                    }
                }
                None
            }
            Message::Insert => Some(Edit::Insert {
                at: editor.draft().ok_or("Read a macro first")?.events.len(),
                event: self.event(editor)?,
            }),
            Message::Replace => Some(Edit::Replace {
                at: self.selected.ok_or("Select an event to replace")?,
                event: self.event(editor)?,
            }),
            Message::Remove(at) => {
                self.selected = None;
                Some(Edit::Remove { at })
            }
            Message::Move { from, to } => {
                self.selected = None;
                Some(Edit::Move { from, to })
            }
            Message::Clear => {
                self.selected = None;
                Some(Edit::Clear)
            }
            Message::Select(_)
            | Message::Add
            | Message::Read
            | Message::Save
            | Message::Revert
            | Message::Assign(_) => None,
        };
        Ok(edit)
    }

    fn event(&self, editor: &Editor<MacroRules>) -> Result<Event, String> {
        let action = match self.kind {
            Kind::Key => Action::Key {
                usage: self.value.parse().map_err(|_| "Enter a key usage number")?,
                pressed: self.pressed,
            },
            Kind::Button => Action::Button {
                button: self.value.parse().map_err(|_| "Enter a button number")?,
                pressed: self.pressed,
            },
            Kind::Movement => Action::Move {
                dx: self.value.parse().map_err(|_| "Enter an X movement")?,
                dy: self.second.parse().map_err(|_| "Enter a Y movement")?,
            },
            Kind::Backend => Action::Backend {
                backend_id: editor.capabilities().backend_id.clone(),
                id: self.value.clone(),
                pressed: self.pressed,
            },
        };
        Ok(Event {
            action,
            delay_ms: self
                .delay
                .parse()
                .map_err(|_| "Enter a delay in milliseconds")?,
        })
    }
}
