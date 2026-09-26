//! Macro widgets project the shared editor, library and unsubmitted form.
use crate::{
    form::macros::{self, Composer, Form, Kind, Message},
    widget::panels::UiStyle,
};
use byakko_core::{
    editor::{Editor, Status, macros::MacroRules},
    library::macros::{Library, Occupancy},
    model::{
        keymap::{Action as KeyAction, Descriptor},
        macros::{Action, Capabilities, Content},
    },
};
use iced::{
    Element, Fill,
    widget::{button, checkbox, column, pick_list, row, scrollable, text, text_input},
};
pub struct View<'a> {
    pub form: &'a Form,
    pub editor: &'a Editor<MacroRules>,
    pub descriptor: &'a Descriptor,
    pub library: &'a Library,
    pub names: &'a crate::form::files::Form,
    pub idle: bool,
    pub target: Option<(&'a str, &'a str)>,
    pub scanning: bool,
    pub style: &'a UiStyle,
}
pub fn view<'a, M: 'a>(
    input: View<'a>,
    files: Element<'a, M>,
    on_message: fn(Message) -> M,
) -> Element<'a, M> {
    let View {
        form,
        editor,
        descriptor,
        library,
        names,
        idle,
        target,
        scanning,
        style,
    } = input;
    let editable = idle && editor.status() == &Status::Ready && editor.draft().is_some();
    let repeat_valid = form.validate_repeat(editor).is_ok();
    let slots = column(editor.capabilities().slots.iter().map(|slot| {
        let occupancy = match library.occupancy(&slot.id) {
            Some(Occupancy::Empty) => "empty",
            Some(Occupancy::Configured) => "configured",
            Some(Occupancy::Opaque) => "preserved",
            Some(Occupancy::Unknown) | None => "unread",
        };
        button(text(format!("{} · {occupancy}", names.name(&slot.id))))
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
        scrollable(slots).spacing(f32::from(style.scrollbar_inset)),
    ]
    .spacing(style.spacing.s);
    let mut detail = column![
        row![
            button("Read slot").on_press_maybe(idle.then_some(Message::Read)),
            button("Save macro").on_press_maybe(
                (editable && editor.dirty() && repeat_valid).then_some(Message::Save)
            ),
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
                text_input("Count", &form.repeat)
                    .on_input_maybe(editable.then_some(Message::Repeat))
                    .width(style.fields.compact),
                text(format!("Current: {}", program.repeat_count)),
            ]
            .spacing(style.spacing.s),
        );
        let events = column(program.events.iter().enumerate().map(|(index, event)| {
            row![
                button(text(format!(
                    "{}: {} · {} ms",
                    index + 1,
                    action_label(&event.action, descriptor, editor.capabilities()),
                    event.delay_ms
                )))
                .on_press_maybe(editable.then_some(Message::Event(index))),
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
        detail = detail.push(events);
        let caps = editor.capabilities();
        detail = detail.push(
            row![
                button(if form.composer == Composer::Closed {
                    "Edit events manually"
                } else {
                    "Hide manual editor"
                })
                .on_press_maybe(editable.then_some(Message::ToggleComposer)),
                button("Clear events").on_press_maybe(
                    (editable && !program.events.is_empty()).then_some(Message::Clear)
                ),
            ]
            .spacing(style.spacing.s),
        );
        if form.composer != Composer::Closed {
            detail = detail.push(composer(form, descriptor, editor, editable, style));
        }
        detail = detail.push(text(format!(
            "Wait after event: {}–{} ms · repeats: {}–{}",
            caps.delays_ms.start(),
            caps.delays_ms.end(),
            caps.editable_repeat_counts.start(),
            caps.editable_repeat_counts.end()
        )));
        if let Some(budget) = &caps.byte_budget {
            detail = detail.push(text(format!(
                "Storage limit: {} encoded bytes",
                budget.limit
            )));
        }
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
                    (editable && target.is_some() && repeat_valid)
                        .then(|| Message::Assign(binding.id.clone())),
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
    let library: Element<'a, Message> = library.into();
    let detail: Element<'a, Message> = detail.into();
    row![
        iced::widget::container(library.map(on_message))
            .width(iced::Length::FillPortion(style.panes.sidebar)),
        scrollable(column![detail.map(on_message), files].spacing(style.spacing.l))
            .spacing(f32::from(style.scrollbar_inset))
            .height(Fill)
            .width(iced::Length::FillPortion(style.panes.detail))
    ]
    .spacing(style.spacing.l)
    .height(Fill)
    .into()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct KeyChoice {
    usage: u16,
    label: String,
}
impl std::fmt::Display for KeyChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
fn key_choices(form: &Form, descriptor: &Descriptor, caps: &Capabilities) -> Vec<KeyChoice> {
    let mut choices = std::collections::BTreeMap::new();
    for choice in &descriptor.actions {
        if let KeyAction::Key(usage) = &choice.action
            && caps
                .keys
                .as_ref()
                .is_some_and(|range| range.contains(usage))
        {
            choices
                .entry(*usage)
                .or_insert_with(|| choice.label.clone());
        }
    }
    // A valid loaded event may be absent from the ordinary assignment catalog.
    if matches!(form.composer, Composer::Replace(_))
        && let Ok(usage) = form.value.parse::<u16>()
        && caps
            .keys
            .as_ref()
            .is_some_and(|range| range.contains(&usage))
    {
        choices
            .entry(usage)
            .or_insert_with(|| format!("Key {usage}"));
    }
    let mut choices: Vec<_> = choices
        .into_iter()
        .map(|(usage, label)| KeyChoice { usage, label })
        .collect();
    choices.sort_by(|a, b| a.label.cmp(&b.label));
    choices
}
fn composer<'a>(
    form: &'a Form,
    descriptor: &Descriptor,
    editor: &Editor<MacroRules>,
    editable: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    let caps = editor.capabilities();
    let mut fields = row![].spacing(style.spacing.s);
    if editable {
        fields = fields.push(
            pick_list(macros::kinds(caps), form.kind.clone(), Message::Kind)
                .placeholder("Choose an event type"),
        );
    } else {
        fields = fields.push(text(
            form.kind
                .as_ref()
                .map_or_else(|| "Choose an event type".into(), ToString::to_string),
        ));
    }
    match form.kind.as_ref() {
        Some(Kind::Key) if editable => {
            let choices = key_choices(form, descriptor, caps);
            let selected = form
                .value
                .parse::<u16>()
                .ok()
                .and_then(|usage| choices.iter().find(|choice| choice.usage == usage).cloned());
            fields = fields.push(
                pick_list(choices, selected, |choice: KeyChoice| {
                    Message::Value(choice.usage.to_string())
                })
                .placeholder("Choose a key"),
            );
        }
        Some(Kind::Movement) => {
            fields = fields
                .push(
                    text_input("Horizontal", &form.value)
                        .on_input_maybe(editable.then_some(Message::Value))
                        .width(style.fields.compact),
                )
                .push(
                    text_input("Vertical", &form.second)
                        .on_input_maybe(editable.then_some(Message::Second))
                        .width(style.fields.compact),
                );
        }
        _ => {}
    }
    if form.kind.is_some() && !matches!(form.kind, Some(Kind::Movement)) {
        fields = fields.push(
            checkbox(form.pressed)
                .label("Pressed / flag set")
                .on_toggle_maybe(editable.then_some(Message::Pressed)),
        );
    }
    let mut content = column![
        text(match form.composer {
            Composer::Replace(index) => format!("Replace event {}", index + 1),
            _ => "Append event".into(),
        }),
        fields,
        row![
            text("Wait after (ms)"),
            text_input("Wait", &form.delay)
                .on_input_maybe(editable.then_some(Message::Delay))
                .width(style.fields.compact),
            button("Stage event").on_press_maybe(editable.then_some(Message::StageEvent)),
            button("New event").on_press_maybe(editable.then_some(Message::NewEvent)),
        ]
        .spacing(style.spacing.s),
    ]
    .spacing(style.spacing.s);
    if let Some(Kind::Movement) = &form.kind
        && let Some(range) = &caps.movement
    {
        content = content.push(text(format!(
            "Each movement axis: {}–{}",
            range.start(),
            range.end()
        )));
    }
    content.into()
}
pub(super) fn action_label(
    action: &Action,
    descriptor: &Descriptor,
    caps: &Capabilities,
) -> String {
    let direction = |pressed| if pressed { "press" } else { "release" };
    match action {
        Action::Key { usage, pressed } => {
            let label = descriptor
                .actions
                .iter()
                .find(|choice| choice.action == KeyAction::Key(*usage))
                .map_or_else(|| format!("Key {usage}"), |choice| choice.label.clone());
            format!("{label} {}", direction(*pressed))
        }
        Action::Button { button, pressed } => {
            let label = caps
                .buttons
                .iter()
                .find(|choice| choice.button == *button)
                .map_or_else(|| format!("Button {button}"), |choice| choice.label.clone());
            format!("{label} {}", direction(*pressed))
        }
        Action::Move { dx, dy } => format!("Move {dx}, {dy}"),
        Action::Backend { id, pressed, .. } => {
            let label = caps
                .backend_actions
                .iter()
                .find(|choice| choice.id == *id)
                .map_or(id.as_str(), |choice| choice.label.as_str());
            format!("{label} {}", direction(*pressed))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_choices_intersect_capabilities_and_keep_a_loaded_unlisted_usage() {
        let device = byakko_devices::memory::demo().unwrap();
        let session = device.session().unwrap();
        let mut descriptor = session.descriptor().clone();
        descriptor
            .actions
            .retain(|choice| matches!(choice.action, KeyAction::Key(4 | 5)));
        let mut caps = session.macros().unwrap().capabilities().clone();
        caps.keys = Some(5..=100);
        let mut form = Form {
            composer: Composer::Replace(0),
            value: "100".into(),
            ..Form::default()
        };
        let choices = key_choices(&form, &descriptor, &caps);
        assert_eq!(
            choices,
            vec![
                KeyChoice {
                    usage: 5,
                    label: "B".into()
                },
                KeyChoice {
                    usage: 100,
                    label: "Key 100".into()
                }
            ]
        );
        form.composer = Composer::New;
        assert_eq!(
            key_choices(&form, &descriptor, &caps),
            vec![KeyChoice {
                usage: 5,
                label: "B".into()
            }]
        );
        assert_eq!(
            action_label(
                &Action::Key {
                    usage: 5,
                    pressed: false
                },
                session.descriptor(),
                &caps
            ),
            "B release"
        );
    }
}
