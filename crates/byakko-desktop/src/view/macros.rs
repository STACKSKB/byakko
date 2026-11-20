//! Macro widgets project the shared editor, library and unsubmitted form.
use crate::widget::controls::{pick_list, text_input};
use crate::{
    form::macros::{self, Composer, Form, Kind, Message},
    widget::{
        focus_button::button,
        focus_control,
        panels::{self, UiStyle},
    },
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
    widget::{checkbox, column, container, row, text},
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
    pub bound_keys: Vec<String>,
    pub recording: bool,
    pub assignment_ready: bool,
    pub scanning: bool,
    pub style: &'a UiStyle,
}
pub fn view<'a, M: Clone + 'a>(
    input: View<'a>,
    recording_controls: Element<'a, M>,
    _recording_preview: Option<Element<'a, M>>,
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
        bound_keys,
        recording,
        assignment_ready,
        scanning,
        style,
    } = input;
    let selected = editor.baseline().is_some();
    let editable = idle
        && editing_allowed
        && !recording
        && editor.status() == &Status::Ready
        && editor.draft().is_some();
    let nonempty = editor
        .draft()
        .is_some_and(|program| !program.events.is_empty());
    let repeat_valid = form.validate_repeat(editor).is_ok();
    let binding = form.selected_binding(editor, bound_action);
    let selected_label = names.name(editor.slot());
    let selected_label = if selected_label.trim().is_empty() {
        editor
            .capabilities()
            .slots
            .iter()
            .find(|slot| slot.id == editor.slot())
            .map_or("Macro", |slot| slot.label.as_str())
    } else {
        selected_label
    };

    let content: Element<'a, M> = match form.screen {
        macros::Screen::Library => {
            let new_allowed = idle
                && !recording
                && !editor.dirty()
                && editor.capabilities().slots.iter().any(|slot| {
                    matches!(
                        library.occupancy(&slot.id),
                        Some(Occupancy::Empty | Occupancy::Unknown)
                    ) && !bound_slots.contains(&slot.id)
                });
            let heading = row![
                text("Macros").size(style.type_scale.page_title).width(Fill),
                button("+ New macro")
                    .on_press_maybe(new_allowed.then_some(on_message(Message::Add))),
            ]
            .spacing(style.spacing.s);
            let mut entries = column![].spacing(style.spacing.xs);
            for slot in editor.capabilities().slots.iter().filter(|slot| {
                matches!(
                    library.occupancy(&slot.id),
                    Some(Occupancy::Configured | Occupancy::Opaque)
                ) || bound_slots.contains(&slot.id)
                    || (selected && editor.dirty() && editor.slot() == slot.id)
            }) {
                let label = if names.name(&slot.id).trim().is_empty() {
                    slot.label.as_str()
                } else {
                    names.name(&slot.id)
                };
                let same_slot = selected && editor.slot() == slot.id;
                let selectable = idle && !recording && (!editor.dirty() || same_slot);
                let assignable = selectable
                    && !editor.dirty()
                    && matches!(library.occupancy(&slot.id), Some(Occupancy::Configured));
                let summary = if same_slot && editor.dirty() {
                    "Draft"
                } else if bound_slots.contains(&slot.id) {
                    "Assigned"
                } else if matches!(library.occupancy(&slot.id), Some(Occupancy::Opaque)) {
                    "Read only"
                } else {
                    "Saved"
                };
                entries = entries.push(
                    container(
                        row![
                            column![text(label.to_owned()), text(summary).size(12)]
                                .spacing(style.spacing.xs)
                                .width(Fill),
                            secondary(
                                "Edit",
                                selectable.then(|| on_message(Message::Select(slot.id.clone())))
                            ),
                            secondary(
                                "Assign",
                                assignable
                                    .then(|| on_message(Message::AssignExisting(slot.id.clone())))
                            ),
                        ]
                        .spacing(style.spacing.s),
                    )
                    .padding(style.spacing.s as u16)
                    .width(Fill),
                );
            }
            if library.error().is_some() {
                entries = entries.push(secondary(
                    "Retry library",
                    (idle && !scanning).then_some(on_message(Message::ReadCatalog)),
                ));
            } else if scanning {
                entries = entries.push(text("Finding macros…"));
            } else if library
                .slots()
                .values()
                .any(|state| matches!(state, Occupancy::Unknown))
            {
                entries = entries.push(text("Library not fully read"));
            }
            if editor.dirty() {
                entries = entries
                    .push(text("Save or revert this draft before opening another macro.").size(12));
            }
            column![
                heading,
                panels::vertical_scroll(style, entries).height(Fill)
            ]
            .spacing(style.spacing.s)
            .height(Fill)
            .into()
        }
        macros::Screen::Edit | macros::Screen::Assign => {
            let assigning = form.screen == macros::Screen::Assign;
            let heading = row![
                text(if assigning {
                    "Assign macro"
                } else {
                    "Macro editor"
                })
                .size(style.type_scale.section_title)
                .width(Fill),
                secondary("Library", (!recording).then_some(on_message(Message::Back))),
            ]
            .spacing(style.spacing.s);
            let top: Element<'a, M> = if assigning || !selected {
                heading.into()
            } else {
                column![heading, name_controls]
                    .spacing(style.spacing.xs)
                    .into()
            };
            let mut timeline = column![].spacing(style.spacing.s).width(Fill);
            if !selected {
                timeline = timeline.push(text("Choose a macro in the library."));
            } else if editor.status() != &Status::Ready {
                timeline = timeline.push(
                    button("Read macro").on_press_maybe(idle.then_some(on_message(Message::Read))),
                );
            } else if assigning {
                timeline = timeline.push(text(selected_label.to_owned())).push(
                    text(format!(
                        "{} events",
                        editor.draft().map_or(0, |p| p.events.len())
                    ))
                    .size(12),
                );
                if !bound_keys.is_empty() {
                    timeline = timeline.push(
                        text(format!("Current triggers: {}", bound_keys.join(", "))).size(12),
                    );
                }
            } else {
                if let Some(program) = editor.draft() {
                    timeline = timeline
                        .push(text(format!("Sequence · {} events", program.events.len())))
                        .push(recording_controls);
                    if recording {
                        timeline =
                            timeline.push(text("Recording local input. Stop to edit.").size(12));
                    }
                    timeline = timeline.push(secondary(
                        "Add event",
                        editable.then_some(on_message(Message::NewEvent)),
                    ));
                    if form.composer != Composer::Closed {
                        timeline = timeline.push(
                            composer(form, descriptor, editor, editable, style).map(on_message),
                        );
                    }
                    for (index, event) in program.events.iter().enumerate() {
                        let label = action_label(&event.action, descriptor, editor.capabilities());
                        timeline = timeline.push(
                            row![
                                text(format!("{:02}", index + 1)).size(12),
                                secondary(
                                    format!("{label} · {} ms", event.delay_ms),
                                    editable.then_some(on_message(Message::Event(index)))
                                )
                                .width(Fill),
                                secondary(
                                    "↑",
                                    (editable && index > 0).then(|| {
                                        on_message(Message::Move {
                                            from: index,
                                            to: index - 1,
                                        })
                                    })
                                ),
                                secondary(
                                    "↓",
                                    (editable && index + 1 < program.events.len()).then_some(
                                        on_message(Message::Move {
                                            from: index,
                                            to: index + 1
                                        })
                                    )
                                ),
                                secondary(
                                    "×",
                                    editable.then_some(on_message(Message::Remove(index)))
                                ),
                            ]
                            .spacing(style.spacing.xs),
                        );
                    }
                    timeline = timeline.push(secondary(
                        "Clear sequence",
                        (editable && !program.events.is_empty())
                            .then_some(on_message(Message::Clear)),
                    ));
                    timeline = timeline.push(secondary(
                        format!("Repeat · {}", program.repeat_count),
                        editable.then_some(on_message(Message::ToggleRepeat)),
                    ));
                    if form.repeat_expanded {
                        let repeat_input: Element<'a, Message> = text_input("Count", &form.repeat)
                            .width(style.fields.compact)
                            .on_input_maybe(editable.then_some(Message::Repeat))
                            .into();
                        timeline = timeline.push(
                            row![text("Repeat"), repeat_input.map(on_message),]
                                .spacing(style.spacing.s),
                        );
                    }
                    timeline = timeline.push(files);
                } else if let Some(snapshot) = editor.baseline()
                    && let Content::Opaque { reason } = &snapshot.content
                {
                    timeline = timeline.push(text(format!("Read only: {reason}")));
                }
            }
            let current_target = target.map_or_else(
                || "Choose a key above".to_owned(),
                |(layer, key)| {
                    let label = descriptor
                        .keys
                        .iter()
                        .find(|candidate| candidate.id == key)
                        .map_or(key, |candidate| candidate.label.as_str());
                    format!("{layer} · {label}")
                },
            );
            let used = if bound_keys.is_empty() {
                "No saved triggers".to_owned()
            } else {
                let shown = bound_keys
                    .iter()
                    .take(2)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ");
                let extra = bound_keys.len().saturating_sub(2);
                format!(
                    "Assigned: {shown}{}",
                    if extra == 0 {
                        String::new()
                    } else {
                        format!(" +{extra}")
                    }
                )
            };
            let mut target_area = column![
                row![text("Trigger").width(Fill), text(current_target).size(12)]
                    .spacing(style.spacing.s),
                text(used).size(12),
            ]
            .spacing(style.spacing.xs);
            if !assignment_ready {
                target_area =
                    target_area.push(text("Save key edits or read the keymap first.").size(12));
            }
            if selected {
                let modes = row(editor
                    .capabilities()
                    .bindings
                    .iter()
                    .filter(|choice| choice.slot == editor.slot())
                    .map(|choice| {
                        panels::selectable_button(
                            style,
                            &choice.label,
                            binding.is_some_and(|current| current.id == choice.id),
                            editable.then(|| on_message(Message::ChooseBinding(choice.id.clone()))),
                        )
                    }))
                .spacing(style.spacing.xs)
                .wrap();
                target_area = target_area.push(modes);
            }
            let can_save = idle
                && editable
                && nonempty
                && repeat_valid
                && editor.dirty()
                && form.composer == Composer::Closed;
            let can_assign = idle
                && assignment_ready
                && editable
                && nonempty
                && repeat_valid
                && form.composer == Composer::Closed
                && target.is_some()
                && binding.is_some_and(|choice| {
                    choice.required_repeat_count.is_none_or(|count| {
                        editor
                            .draft()
                            .is_some_and(|program| program.repeat_count == count)
                    })
                });
            let new_empty = editor.baseline().is_some_and(|snapshot| {
                matches!(&snapshot.content, Content::Editable(program) if program.events.is_empty())
                    && matches!(library.occupancy(editor.slot()), Some(Occupancy::Empty))
            });
            let footer: Element<'a, M> = if assigning {
                row![
                    secondary("Cancel", (!recording).then_some(on_message(Message::Back))),
                    button("Assign").on_press_maybe(
                        can_assign
                            .then(|| on_message(Message::Assign(binding.unwrap().id.clone())))
                    ),
                ]
                .spacing(style.spacing.s)
                .into()
            } else if target.is_some() {
                row![
                    secondary(
                        if new_empty { "Discard" } else { "Revert draft" },
                        (idle && !recording && editor.dirty())
                            .then_some(on_message(Message::Revert))
                    ),
                    secondary("Save macro", can_save.then_some(on_message(Message::Save))),
                    button("Save & assign").on_press_maybe(
                        can_assign
                            .then(|| on_message(Message::Assign(binding.unwrap().id.clone())))
                    ),
                ]
                .spacing(style.spacing.s)
                .wrap()
                .into()
            } else {
                row![
                    secondary(
                        if new_empty { "Discard" } else { "Revert draft" },
                        (idle && !recording && editor.dirty())
                            .then_some(on_message(Message::Revert))
                    ),
                    button("Save macro")
                        .on_press_maybe(can_save.then_some(on_message(Message::Save))),
                ]
                .spacing(style.spacing.s)
                .into()
            };
            column![
                top,
                panels::vertical_scroll(style, timeline)
                    .id("macro-timeline")
                    .height(Fill),
                container(target_area)
                    .padding(style.spacing.s as u16)
                    .width(Fill),
                container(footer)
                    .padding(style.spacing.s as u16)
                    .width(Fill),
            ]
            .spacing(style.spacing.xs)
            .height(Fill)
            .into()
        }
    };
    container(content).width(Fill).height(Fill).into()
}
fn secondary<'a, M: Clone + 'a>(
    label: impl Into<String>,
    message: Option<M>,
) -> crate::widget::focus_button::Button<'a, M> {
    button(text(label.into()))
        .on_press_maybe(message)
        .style(iced::widget::button::secondary)
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
        let kinds = macros::kinds(caps);
        let selected_index = form
            .kind
            .as_ref()
            .and_then(|kind| kinds.iter().position(|choice| choice == kind));
        let key_options = kinds.iter().cloned().map(Message::Kind).collect();
        fields = fields.push(focus_control::select(
            pick_list(kinds, form.kind.clone(), Message::Kind).placeholder("Choose an event type"),
            key_options,
            selected_index,
            true,
        ));
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
            let selected_index = selected
                .as_ref()
                .and_then(|selected| choices.iter().position(|choice| choice == selected));
            let key_options = choices
                .iter()
                .map(|choice| Message::Value(choice.usage.to_string()))
                .collect();
            fields = fields.push(focus_control::select(
                pick_list(choices, selected, |choice: KeyChoice| {
                    Message::Value(choice.usage.to_string())
                })
                .placeholder("Key"),
                key_options,
                selected_index,
                true,
            ));
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
        fields = fields.push(focus_control::toggle(
            checkbox(form.pressed)
                .label("Down / active")
                .on_toggle_maybe(editable.then_some(Message::Pressed)),
            Message::Pressed(!form.pressed),
            editable,
        ));
    }
    let mut content = column![
        text(match form.composer {
            Composer::Replace(index) => format!("Edit event {}", index + 1),
            _ => "New event".into(),
        }),
        fields,
        row![
            text("Wait after (ms)"),
            text_input("Wait", &form.delay)
                .on_input_maybe(editable.then_some(Message::Delay))
                .width(style.fields.compact),
            button(if matches!(form.composer, Composer::Replace(_)) {
                "Update event"
            } else {
                "Add"
            })
            .on_press_maybe(editable.then_some(Message::StageEvent)),
            button("Cancel")
                .on_press_maybe(editable.then_some(Message::ToggleComposer))
                .style(iced::widget::button::secondary),
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
