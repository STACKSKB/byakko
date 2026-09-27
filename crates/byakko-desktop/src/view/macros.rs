//! Macro widgets project the shared editor, library and unsubmitted form.
use crate::{
    form::macros::{self, Composer, Form, Kind, Message},
    widget::panels::{self, UiStyle},
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
    widget::{button, checkbox, column, pick_list, row, text, text_input},
};
pub struct View<'a> {
    pub form: &'a Form,
    pub editor: &'a Editor<MacroRules>,
    pub descriptor: &'a Descriptor,
    pub library: &'a Library,
    pub names: &'a crate::form::files::Form,
    pub idle: bool,
    pub editing_allowed: bool,
    pub target: Option<(&'a str, &'a str)>,
    pub bound_action: Option<&'a KeyAction>,
    pub bound_slots: Vec<String>,
    pub scanning: bool,
    pub wide: bool,
    pub style: &'a UiStyle,
}
pub fn view<'a, M: Clone + 'a>(
    input: View<'a>,
    keyboard: Element<'a, M>,
    recording_controls: Element<'a, M>,
    recording_preview: Option<Element<'a, M>>,
    name_controls: Element<'a, M>,
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
        editing_allowed,
        target,
        bound_action,
        bound_slots,
        scanning,
        wide,
        style,
    } = input;
    let editable = editing_allowed && editor.status() == &Status::Ready && editor.draft().is_some();
    let repeat_valid = form.validate_repeat(editor).is_ok();
    let selected = editor.baseline().is_some();
    let visible_slots: Vec<_> = editor
        .capabilities()
        .slots
        .iter()
        .filter(|slot| {
            matches!(
                library.occupancy(&slot.id),
                Some(Occupancy::Configured | Occupancy::Opaque)
            ) || bound_slots.contains(&slot.id)
                || editor
                    .baseline()
                    .is_some_and(|snapshot| snapshot.slot == slot.id)
        })
        .collect();
    let candidate = editor.capabilities().slots.iter().any(|slot| {
        matches!(
            library.occupancy(&slot.id),
            Some(Occupancy::Empty | Occupancy::Unknown)
        ) && !bound_slots.contains(&slot.id)
    });
    let selected_empty = editor.baseline().is_some_and(|snapshot| matches!(&snapshot.content, Content::Editable(program) if program.events.is_empty()));
    let mut slots = column![button("+ New macro").on_press_maybe(
        (idle && !editor.dirty() && candidate && !selected_empty).then_some(Message::Add)
    )]
    .spacing(style.spacing.s);
    for slot in visible_slots {
        let label = if names.name(&slot.id).trim().is_empty() {
            slot.label.as_str()
        } else {
            names.name(&slot.id)
        };
        slots = slots.push(panels::selectable_button_fill_width(
            style,
            label,
            slot.id == editor.slot(),
            (idle && !editor.dirty()).then(|| Message::Select(slot.id.clone())),
        ));
    }
    if let Some(error) = library.error() {
        slots = slots.push(text(format!("Library scan failed: {error}")));
        slots = slots.push(
            button("Retry scan")
                .on_press_maybe((idle && !scanning).then_some(Message::ReadCatalog)),
        );
    } else if scanning {
        slots = slots.push(text("Finding saved macros…"));
    } else {
        let used = library
            .slots()
            .values()
            .filter(|occupancy| matches!(occupancy, Occupancy::Configured | Occupancy::Opaque))
            .count();
        slots = slots.push(text(format!(
            "{used} of {} slots used",
            editor.capabilities().slots.len()
        )));
    }
    let library: Element<'a, Message> = slots.width(Fill).into();
    let library = panels::panel(style, "Library", library.map(on_message));
    let mut detail = column![].spacing(style.spacing.s).width(Fill);
    if selected {
        detail = detail.push(name_controls).push(recording_controls);
    } else {
        detail = detail.push(text("Select a macro or add one to begin"));
    }
    if let Some(program) = editor.draft() {
        if !editor
            .capabilities()
            .editable_repeat_counts
            .contains(&program.repeat_count)
        {
            let count = *editor.capabilities().editable_repeat_counts.start();
            detail = detail.push(text(format!("This slot has a stored repeat count of {}. Choose a count before recording or saving.", program.repeat_count)))
                .push(button(text(format!("Use repeat {count}"))).on_press_maybe(editable.then_some(on_message(Message::Repeat(count.to_string())))));
        }
        detail = detail.push(text(format!("{} events", program.events.len())));
        let actions: Element<'a, Message> = row![
            button("Clear events")
                .on_press_maybe((editable && !program.events.is_empty()).then_some(Message::Clear)),
            button(if form.composer == Composer::Closed {
                "Edit events manually"
            } else {
                "Hide manual editor"
            })
            .on_press_maybe(editable.then_some(Message::ToggleComposer)),
        ]
        .spacing(style.spacing.s)
        .into();
        detail = detail.push(actions.map(on_message));
        if form.composer != Composer::Closed {
            detail =
                detail.push(composer(form, descriptor, editor, editable, style).map(on_message));
        }
        if let Some(preview) = recording_preview {
            detail = detail.push(preview);
        } else {
            let events = column(program.events.iter().enumerate().map(|(index, event)| {
                row![
                    button(text(format!(
                        "{:02}  {} · wait {} ms",
                        index + 1,
                        action_label(&event.action, descriptor, editor.capabilities()),
                        event.delay_ms
                    )))
                    .width(Fill)
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
                    button("×").on_press_maybe(editable.then_some(Message::Remove(index))),
                ]
                .spacing(style.spacing.xs)
                .into()
            }))
            .spacing(style.spacing.xs);
            let events: Element<'a, Message> =
                panels::vertical_scroll(style, events).height(Fill).into();
            detail = detail.push(events.map(on_message));
        }
    } else if let Some(snapshot) = editor.baseline()
        && let Content::Opaque { reason } = &snapshot.content
    {
        detail = detail.push(text(format!("Preserved as read-only: {reason}")));
    }
    if selected && editor.status() != &Status::Ready {
        detail = detail
            .push(text("Slot needs attention; read it again to continue."))
            .push(button("Retry read").on_press_maybe(idle.then_some(on_message(Message::Read))));
    }
    let detail = panels::panel(style, "Macro editor", detail.height(Fill).into());
    let binding = form.selected_binding(editor, bound_action);
    let modes = row(editor
        .capabilities()
        .bindings
        .iter()
        .filter(|binding| binding.slot == editor.slot())
        .map(|choice| {
            panels::selectable_button(
                style,
                &choice.label,
                binding.is_some_and(|selected| selected.id == choice.id),
                editable.then(|| Message::ChooseBinding(choice.id.clone())),
            )
        }))
    .spacing(style.spacing.s)
    .wrap();
    let repeat_restriction = binding
        .and_then(|choice| choice.required_repeat_count)
        .filter(|required| {
            editor
                .draft()
                .is_none_or(|program| program.repeat_count != *required)
        });
    let mut playback = column![
        text(target.map_or_else(
            || "Key: —".into(),
            |(_, key)| {
                let label = descriptor
                    .keys
                    .iter()
                    .find(|candidate| candidate.id == key)
                    .map_or(key, |candidate| candidate.label.as_str());
                format!("Key: {label}")
            }
        )),
        modes,
        row![
            text("Repeat"),
            text_input("Count", &form.repeat)
                .width(style.fields.compact)
                .on_input_maybe(editable.then_some(Message::Repeat))
        ]
        .spacing(style.spacing.s),
        button(if editor.dirty() {
            "Save & assign"
        } else {
            "Assign macro"
        })
        .on_press_maybe(
            binding
                .filter(|_| idle
                    && editable
                    && target.is_some()
                    && repeat_valid
                    && repeat_restriction.is_none())
                .map(|choice| Message::Assign(choice.id.clone()))
        ),
        row![
            button("Save only").on_press_maybe(
                (idle && editable && editor.dirty() && repeat_valid).then_some(Message::Save)
            ),
            button("Revert edits")
                .on_press_maybe((idle && editor.dirty()).then_some(Message::Revert)),
        ]
        .spacing(style.spacing.s),
    ]
    .spacing(style.spacing.s);
    if let Some(required) = repeat_restriction {
        playback = playback.push(text(format!(
            "This playback mode requires repeat {required}."
        )));
    }
    let playback: Element<'a, Message> = playback.into();
    let playback = panels::panel(
        style,
        "Playback",
        column![playback.map(on_message), files]
            .spacing(style.spacing.s)
            .into(),
    );
    if !wide {
        return column![
            keyboard,
            row![
                iced::widget::container(detail)
                    .width(iced::Length::FillPortion(style.panes.detail)),
                iced::widget::container(panels::vertical_scroll(
                    style,
                    column![library, playback].spacing(style.spacing.m)
                ))
                .width(iced::Length::FillPortion(style.panes.sidebar))
                .height(Fill),
            ]
            .spacing(style.spacing.m)
            .height(Fill),
        ]
        .spacing(style.spacing.m)
        .height(Fill)
        .into();
    }
    row![
        column![keyboard, detail]
            .spacing(style.spacing.m)
            .width(Fill)
            .height(Fill),
        panels::vertical_scroll(style, column![library, playback].spacing(style.spacing.m))
            .width(style.key_sidebar_width)
            .height(Fill),
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
    let direction = |pressed| if pressed { "down" } else { "up" };
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
        let mut form = Form::default();
        form.composer = Composer::Replace(0);
        form.value = "100".into();
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
            "B up"
        );
    }
}
