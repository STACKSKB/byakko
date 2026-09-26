//! Unsubmitted macro fields. Valid programs and library facts belong to core.
use crate::panels::UiStyle;
use byakko_core::macros::{
    Action, Content, Edit, Event, Program,
    editor::{Editor, Status},
    library::{Library, Occupancy},
};
use iced::{
    Element, Fill,
    widget::{button, checkbox, column, pick_list, row, scrollable, text, text_input},
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
    repeat: String,
    kind: Kind,
    value: String,
    second: String,
    delay: String,
    pressed: bool,
    selected: Option<usize>,
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

    pub fn update(&mut self, message: Message, editor: &Editor) -> Result<Option<Edit>, String> {
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

    fn event(&self, editor: &Editor) -> Result<Event, String> {
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

    pub fn view<'a>(
        &'a self,
        editor: &'a Editor,
        library: &'a Library,
        idle: bool,
        target: Option<(&str, &str)>,
        scanning: bool,
        style: &'a UiStyle,
    ) -> Element<'a, Message> {
        let editable = idle && editor.status() == &Status::Ready && editor.draft().is_some();
        let slots = column(editor.capabilities().slots.iter().map(|slot| {
            let occupancy = match library.occupancy(&slot.id) {
                Some(Occupancy::Empty) => "empty",
                Some(Occupancy::Configured) => "configured",
                Some(Occupancy::Opaque) => "preserved",
                Some(Occupancy::Unknown) | None => "unread",
            };
            button(text(format!("{} · {occupancy}", slot.label)))
                .on_press_maybe((idle && !editor.dirty()).then(|| Message::Select(slot.id.clone())))
                .into()
        }))
        .spacing(style.spacing.s);
        let library = column![
            text(if scanning {
                "Macro library · reading…"
            } else {
                "Macro library"
            }),
            button("Add macro").on_press_maybe((idle && !editor.dirty()).then_some(Message::Add)),
            scrollable(slots),
        ]
        .spacing(style.spacing.s);
        let mut detail = column![
            row![
                button("Read slot").on_press_maybe(idle.then_some(Message::Read)),
                button("Save macro")
                    .on_press_maybe((editable && editor.dirty()).then_some(Message::Save)),
                button("Revert macro")
                    .on_press_maybe((idle && editor.dirty()).then_some(Message::Revert)),
            ]
            .spacing(style.spacing.s),
        ]
        .spacing(style.spacing.m);
        if let Some(program) = editor.draft() {
            detail = detail.push(
                row![
                    text("Repeats"),
                    text_input("Count", &self.repeat)
                        .on_input(Message::Repeat)
                        .width(style.fields.compact),
                    button("Set count").on_press_maybe(editable.then_some(Message::ApplyRepeat)),
                    text(format!("Current: {}", program.repeat_count)),
                ]
                .spacing(style.spacing.s),
            );
            let events = column(program.events.iter().enumerate().map(|(index, event)| {
                row![
                    button(text(format!(
                        "{}: {} · {} ms",
                        index + 1,
                        action_label(&event.action),
                        event.delay_ms
                    )))
                    .on_press(Message::Event(index)),
                    button("↑").on_press_maybe((editable && index > 0).then_some(Message::Move {
                        from: index,
                        to: index.saturating_sub(1)
                    })),
                    button("↓").on_press_maybe(
                        (editable && index + 1 < program.events.len()).then_some(Message::Move {
                            from: index,
                            to: index + 1
                        })
                    ),
                    button("Remove").on_press_maybe(editable.then_some(Message::Remove(index))),
                ]
                .spacing(style.spacing.s)
                .into()
            }))
            .spacing(style.spacing.xs);
            detail = detail.push(scrollable(events).height(Fill));
            let caps = editor.capabilities();
            let kinds: Vec<_> = [
                caps.keys.is_some().then_some(Kind::Key),
                (!caps.buttons.is_empty()).then_some(Kind::Button),
                caps.movement.is_some().then_some(Kind::Movement),
                (!caps.backend_actions.is_empty()).then_some(Kind::Backend),
            ]
            .into_iter()
            .flatten()
            .collect();
            detail = detail.push(
                row![
                    pick_list(kinds, Some(self.kind), Message::Kind),
                    text_input("Usage / button / X / action ID", &self.value)
                        .on_input(Message::Value),
                    text_input("Delay (ms)", &self.delay)
                        .on_input(Message::Delay)
                        .width(style.fields.compact),
                ]
                .spacing(style.spacing.s),
            );
            if self.kind == Kind::Movement {
                detail =
                    detail.push(text_input("Y movement", &self.second).on_input(Message::Second));
            } else {
                detail = detail.push(
                    checkbox(self.pressed)
                        .label("Pressed (clear for release)")
                        .on_toggle(Message::Pressed),
                );
            }
            detail = detail.push(
                row![
                    button("Append event").on_press_maybe(editable.then_some(Message::Insert)),
                    button("Replace selected").on_press_maybe(
                        (editable && self.selected.is_some()).then_some(Message::Replace)
                    ),
                    button("Clear events").on_press_maybe(editable.then_some(Message::Clear)),
                ]
                .spacing(style.spacing.s),
            );
            detail = detail.push(text(target.map_or_else(
                || "Select a key in Assignments to bind this macro.".into(),
                |(layer, key)| format!("Assign to {layer} / {key}"),
            )));
            for binding in caps
                .bindings
                .iter()
                .filter(|binding| binding.slot == editor.slot())
            {
                detail = detail.push(
                    button(text(format!("Save and assign · {}", binding.label))).on_press_maybe(
                        (editable && target.is_some()).then(|| Message::Assign(binding.id.clone())),
                    ),
                );
            }
        } else if let Some(snapshot) = editor.baseline() {
            if let Content::Opaque { reason } = &snapshot.content {
                detail = detail.push(text(format!("Preserved macro: {reason}")));
            }
        } else {
            detail = detail.push(text("Read this slot to edit it."));
        }
        row![
            library.width(iced::Length::FillPortion(style.panes.sidebar)),
            detail.width(iced::Length::FillPortion(style.panes.detail))
        ]
        .spacing(style.spacing.l)
        .height(Fill)
        .into()
    }
}

fn action_label(action: &Action) -> String {
    let direction = |pressed| if pressed { "press" } else { "release" };
    match action {
        Action::Key { usage, pressed } => format!("Key {usage} {}", direction(*pressed)),
        Action::Button { button, pressed } => format!("Button {button} {}", direction(*pressed)),
        Action::Move { dx, dy } => format!("Move {dx}, {dy}"),
        Action::Backend { id, pressed, .. } => format!("{id} {}", direction(*pressed)),
    }
}
