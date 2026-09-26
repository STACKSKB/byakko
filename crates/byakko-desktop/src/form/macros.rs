//! Unsubmitted text and composer intent; the core editor owns the program.
use byakko_core::{
    editor::{Editor, macros::MacroRules},
    model::macros::{Action, Capabilities, Edit, Event, Program},
};
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Kind {
    Key,
    Button { id: u16, label: String },
    Movement,
    Backend { id: String, label: String },
}
impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Key => "Keyboard key",
            Self::Movement => "Pointer movement",
            Self::Button { label, .. } | Self::Backend { label, .. } => label,
        })
    }
}
pub fn kinds(caps: &Capabilities) -> Vec<Kind> {
    caps.keys
        .as_ref()
        .map(|_| Kind::Key)
        .into_iter()
        .chain(caps.buttons.iter().map(|choice| Kind::Button {
            id: choice.button,
            label: choice.label.clone(),
        }))
        .chain(caps.movement.as_ref().map(|_| Kind::Movement))
        .chain(caps.backend_actions.iter().map(|choice| Kind::Backend {
            id: choice.id.clone(),
            label: choice.label.clone(),
        }))
        .collect()
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Composer {
    #[default]
    Closed,
    New,
    Replace(usize),
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
    Kind(Kind),
    Value(String),
    Second(String),
    Delay(String),
    Pressed(bool),
    Event(usize),
    NewEvent,
    ToggleComposer,
    StageEvent,
    Remove(usize),
    Move { from: usize, to: usize },
    Clear,
}
#[derive(Default)]
pub struct Form {
    pub(crate) repeat: String,
    pub(crate) kind: Option<Kind>,
    pub(crate) value: String,
    pub(crate) second: String,
    pub(crate) delay: String,
    pub(crate) pressed: bool,
    pub(crate) composer: Composer,
}
impl Form {
    pub fn sync(&mut self, program: Option<&Program>) {
        self.repeat = program.map_or_else(String::new, |program| program.repeat_count.to_string());
        self.reset_event();
    }
    fn reset_event(&mut self) {
        self.kind = None;
        self.value.clear();
        self.second.clear();
        self.delay.clear();
        self.pressed = false;
        self.composer = Composer::Closed;
    }
    pub fn accepted(&mut self, edit: &Edit) {
        if !matches!(edit, Edit::Repeat(_)) {
            self.reset_event();
        }
    }
    pub fn validate_repeat(&self, editor: &Editor<MacroRules>) -> Result<(), String> {
        let count = number::<u32>(&self.repeat, "Repeat count")?;
        if !editor
            .capabilities()
            .editable_repeat_counts
            .contains(&count)
        {
            return Err("Repeat count is outside the advertised editor range".into());
        }
        if editor
            .draft()
            .is_none_or(|program| program.repeat_count != count)
        {
            return Err("Apply the entered repeat count before saving or assigning".into());
        }
        Ok(())
    }
    pub fn update(
        &mut self,
        message: Message,
        editor: &Editor<MacroRules>,
    ) -> Result<Option<Edit>, String> {
        let edit = match message {
            Message::Repeat(value) => {
                self.repeat = value;
                let count = number::<u32>(&self.repeat, "Repeat count")?;
                if !editor
                    .capabilities()
                    .editable_repeat_counts
                    .contains(&count)
                {
                    return Err("Repeat count is outside the advertised editor range".into());
                }
                Some(Edit::Repeat(count))
            }
            Message::Kind(kind) => {
                if !kinds(editor.capabilities()).contains(&kind) {
                    return Err("Event type is not advertised by this backend".into());
                }
                self.kind = Some(kind);
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
                    .and_then(|program| program.events.get(index))
                    .ok_or("Select an existing event")?;
                self.load_event(index, event, editor.capabilities());
                None
            }
            Message::NewEvent => {
                self.reset_event();
                self.composer = Composer::New;
                None
            }
            Message::ToggleComposer => {
                if self.composer == Composer::Closed {
                    self.composer = Composer::New;
                } else {
                    self.reset_event();
                }
                None
            }
            Message::StageEvent => {
                let event = self.event(editor.capabilities())?;
                Some(match self.composer {
                    Composer::New => Edit::Insert {
                        at: editor.draft().ok_or("Read a macro first")?.events.len(),
                        event,
                    },
                    Composer::Replace(at) => Edit::Replace { at, event },
                    Composer::Closed => return Err("Open the event composer before staging".into()),
                })
            }
            Message::Remove(at) => Some(Edit::Remove { at }),
            Message::Move { from, to } => Some(Edit::Move { from, to }),
            Message::Clear => Some(Edit::Clear),
            Message::Select(_)
            | Message::Add
            | Message::Read
            | Message::Save
            | Message::Revert
            | Message::Assign(_) => None,
        };
        Ok(edit)
    }
    fn load_event(&mut self, index: usize, event: &Event, caps: &Capabilities) {
        self.reset_event();
        self.composer = Composer::Replace(index);
        self.delay = event.delay_ms.to_string();
        self.kind = Some(match &event.action {
            Action::Key { usage, pressed } => {
                self.value = usage.to_string();
                self.pressed = *pressed;
                Kind::Key
            }
            Action::Button { button, pressed } => {
                self.pressed = *pressed;
                Kind::Button {
                    id: *button,
                    label: caps
                        .buttons
                        .iter()
                        .find(|choice| choice.button == *button)
                        .map_or_else(|| button.to_string(), |choice| choice.label.clone()),
                }
            }
            Action::Move { dx, dy } => {
                self.value = dx.to_string();
                self.second = dy.to_string();
                Kind::Movement
            }
            Action::Backend { id, pressed, .. } => {
                self.pressed = *pressed;
                Kind::Backend {
                    id: id.clone(),
                    label: caps
                        .backend_actions
                        .iter()
                        .find(|choice| choice.id == *id)
                        .map_or_else(|| id.clone(), |choice| choice.label.clone()),
                }
            }
        });
    }
    fn event(&self, caps: &Capabilities) -> Result<Event, String> {
        let action = match self.kind.as_ref().ok_or("Choose an event type")? {
            Kind::Key => Action::Key {
                usage: number(&self.value, "Keyboard usage")?,
                pressed: self.pressed,
            },
            Kind::Button { id, .. } => Action::Button {
                button: *id,
                pressed: self.pressed,
            },
            Kind::Movement => Action::Move {
                dx: number(&self.value, "Horizontal movement")?,
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
            delay_ms: number(&self.delay, "Wait after event")?,
        })
    }
}
fn number<T: std::str::FromStr>(input: &str, field: &str) -> Result<T, String> {
    input
        .trim()
        .parse()
        .map_err(|_| format!("{field}: enter a whole number"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::model::macros::{ButtonChoice, Choice, Content, Snapshot};
    fn editor() -> Editor<MacroRules> {
        let caps = Capabilities {
            backend_id: "synthetic".into(),
            slots: vec![Choice {
                id: "one".into(),
                label: "One".into(),
            }],
            repeat_counts: 0..=10,
            editable_repeat_counts: 1..=10,
            delays_ms: 0..=50,
            keys: Some(4..=8),
            buttons: vec![ButtonChoice {
                button: 3,
                label: "Middle button".into(),
            }],
            movement: Some(-10..=10),
            backend_actions: vec![Choice {
                id: "wheel".into(),
                label: "Scroll left".into(),
            }],
            byte_budget: None,
            bindings: vec![],
        };
        let mut editor = Editor::new(MacroRules::new(caps).unwrap());
        editor.accept_read(Ok(Snapshot {
            backend_id: "synthetic".into(),
            slot: "one".into(),
            revision: vec![1],
            content: Content::Editable(Program {
                repeat_count: 1,
                events: vec![
                    Event {
                        action: Action::Key {
                            usage: 8,
                            pressed: false,
                        },
                        delay_ms: 0,
                    },
                    Event {
                        action: Action::Button {
                            button: 3,
                            pressed: false,
                        },
                        delay_ms: 0,
                    },
                    Event {
                        action: Action::Move { dx: -2, dy: 3 },
                        delay_ms: 0,
                    },
                    Event {
                        action: Action::Backend {
                            backend_id: "synthetic".into(),
                            id: "wheel".into(),
                            pressed: false,
                        },
                        delay_ms: 0,
                    },
                ],
            }),
        }));
        editor
    }
    #[test]
    fn loaded_event_actions_roundtrip_named_choices_release_edges_and_explicit_zero_wait() {
        let editor = editor();
        let mut form = Form::default();
        form.sync(editor.draft());
        let choices = kinds(editor.capabilities());
        assert_eq!(
            choices,
            vec![
                Kind::Key,
                Kind::Button {
                    id: 3,
                    label: "Middle button".into()
                },
                Kind::Movement,
                Kind::Backend {
                    id: "wheel".into(),
                    label: "Scroll left".into()
                }
            ]
        );
        for (index, event) in editor.draft().unwrap().events.iter().enumerate() {
            form.update(Message::Event(index), &editor).unwrap();
            assert_eq!(form.composer, Composer::Replace(index));
            assert_eq!(form.delay, "0");
            assert_eq!(
                form.update(Message::StageEvent, &editor).unwrap(),
                Some(Edit::Replace {
                    at: index,
                    event: event.clone()
                })
            );
        }
        assert!(!form.pressed);
        let mut button_only = editor.capabilities().clone();
        button_only.keys = None;
        button_only.movement = None;
        button_only.backend_actions.clear();
        assert_eq!(
            kinds(&button_only),
            vec![Kind::Button {
                id: 3,
                label: "Middle button".into()
            }]
        );
        let mut blank = Form::default();
        assert!(blank.kind.is_none());
        assert!(
            blank
                .update(
                    Message::Kind(Kind::Button {
                        id: 99,
                        label: "Unsupported".into()
                    }),
                    &editor
                )
                .is_err()
        );
        assert!(blank.kind.is_none());
    }
    #[test]
    fn repeat_stages_immediately_preserving_lexical_text_and_event_composer() {
        let mut editor = editor();
        let mut form = Form::default();
        form.sync(editor.draft());
        form.update(Message::Event(2), &editor).unwrap();
        form.update(Message::Value("-7".into()), &editor).unwrap();
        let edit = form
            .update(Message::Repeat(" 03 ".into()), &editor)
            .unwrap()
            .unwrap();
        assert!(form.validate_repeat(&editor).is_err());
        editor.edit(edit.clone()).unwrap();
        form.accepted(&edit);
        assert!(form.validate_repeat(&editor).is_ok());
        assert_eq!(form.repeat, " 03 ");
        assert_eq!(form.composer, Composer::Replace(2));
        assert_eq!(form.kind, Some(Kind::Movement));
        assert_eq!(form.value, "-7");
        assert_eq!(form.second, "3");
        assert_eq!(form.delay, "0");
        assert!(
            form.update(Message::Repeat("invalid".into()), &editor)
                .is_err()
        );
        assert!(form.validate_repeat(&editor).is_err());
        assert_eq!(editor.draft().unwrap().repeat_count, 3);
        assert_eq!(form.value, "-7");
        assert_eq!(form.composer, Composer::Replace(2));
    }
    #[test]
    fn rejected_sequence_edits_preserve_inputs_and_only_accepted_sequence_edits_reset_them() {
        let mut editor = editor();
        let mut form = Form::default();
        form.sync(editor.draft());
        form.update(Message::Event(0), &editor).unwrap();
        form.update(Message::Delay("51".into()), &editor).unwrap();
        let edit = form.update(Message::StageEvent, &editor).unwrap().unwrap();
        assert!(editor.edit(edit).is_err());
        assert_eq!(form.composer, Composer::Replace(0));
        assert_eq!(form.kind, Some(Kind::Key));
        assert_eq!(form.value, "8");
        assert_eq!(form.delay, "51");
        let edit = form.update(Message::Remove(99), &editor).unwrap().unwrap();
        assert!(editor.edit(edit).is_err());
        assert_eq!(form.composer, Composer::Replace(0));
        form.update(Message::Delay(" 10 ".into()), &editor).unwrap();
        let edit = form.update(Message::StageEvent, &editor).unwrap().unwrap();
        editor.edit(edit.clone()).unwrap();
        form.accepted(&edit);
        assert_eq!(form.composer, Composer::Closed);
        assert!(form.kind.is_none());
        assert!(form.value.is_empty());
        assert!(form.delay.is_empty());
        assert_eq!(form.repeat, "1");
        form.update(Message::NewEvent, &editor).unwrap();
        assert_eq!(form.composer, Composer::New);
        assert!(form.update(Message::StageEvent, &editor).is_err());
        form.update(Message::Kind(Kind::Key), &editor).unwrap();
        form.update(Message::Value("4".into()), &editor).unwrap();
        form.update(Message::Delay("0".into()), &editor).unwrap();
        let edit = form.update(Message::StageEvent, &editor).unwrap().unwrap();
        assert!(matches!(edit, Edit::Insert { at: 4, .. }));
        editor.edit(edit.clone()).unwrap();
        form.accepted(&edit);
        for message in [
            Message::Move { from: 0, to: 1 },
            Message::Remove(0),
            Message::Clear,
        ] {
            form.update(Message::Event(0), &editor).unwrap();
            let edit = form.update(message, &editor).unwrap().unwrap();
            editor.edit(edit.clone()).unwrap();
            form.accepted(&edit);
            assert_eq!(form.composer, Composer::Closed);
            assert!(form.kind.is_none());
        }
    }
}
