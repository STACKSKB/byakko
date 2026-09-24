//! Macro widgets project capabilities and the shared draft; no firmware assumptions.
use super::{
    Desktop, Message,
    macro_editor::Message as Macro,
    macro_form::{self, Input, Kind},
    panels,
};
use byakko_core::Action as KeyAction;
use byakko_core::macros::{
    Action, Choice, Content, Edit, Event,
    editor::{Editor, Status},
};
use iced::{
    Element, Fill,
    widget::{button, checkbox, column, pick_list, row, scrollable, text, text_input},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum Composer {
    #[default]
    Collapsed,
    Expanded,
}

pub(super) fn library(app: &Desktop) -> Element<'_, Message> {
    let Some(editor) = app.session.macros() else {
        return text("Macros are unavailable").into();
    };
    let style = &app.ui;
    let configured = app.session.macro_library_slots();
    let bound = app.session.macro_bound_slots().unwrap_or_default();
    let capacity = editor.capabilities().slots.len();
    let configured_ids: std::collections::BTreeSet<_> = configured
        .as_ref()
        .into_iter()
        .flat_map(|choices| choices.iter().copied())
        .chain(bound.iter().copied())
        .map(|choice| choice.id.as_str())
        .collect();
    let slots: Vec<&Choice> = editor
        .capabilities()
        .slots
        .iter()
        .filter(|choice| {
            configured_ids.contains(choice.id.as_str())
                || app.macro_new_slot.as_deref() == Some(choice.id.as_str())
                || editor
                    .baseline()
                    .is_some_and(|snapshot| snapshot.slot == choice.id)
        })
        .collect();
    let can_select = !app.busy();
    let selected_candidate_free = app.macro_new_slot.as_deref().is_some_and(|id| {
        editor.baseline().is_some_and(|snapshot| snapshot.slot == id && matches!(&snapshot.content, Content::Editable(program) if program.events.is_empty()))
    });
    let candidate = if configured.is_some() {
        app.session.next_free_macro_slot()
    } else if app.macro_new_slot.is_none() {
        app.session
            .next_free_macro_slot()
            .or_else(|| app.session.next_macro_candidate_after(None))
    } else {
        app.session
            .next_macro_candidate_after(app.macro_new_slot.as_deref())
    };
    let can_add = can_select
        && !editor.dirty()
        && candidate.is_some()
        && !(configured.is_some() && app.macro_new_slot.is_some())
        && !selected_candidate_free;
    let mut list = column![
        button("+ New macro").on_press_maybe(can_add.then_some(Message::Macro(Macro::Add)))
    ]
    .spacing(style.spacing.s);
    for choice in slots {
        list = list.push(panels::selectable_button_fill_width(
            style,
            app.macro_files
                .slot_label(&choice.id, &choice.label)
                .to_owned(),
            choice.id == editor.slot(),
            can_select.then(|| Message::Macro(Macro::Select(choice.id.clone()))),
        ));
    }
    if let Some(error) = editor.catalog_error() {
        list = list.push(text(format!("Library scan failed: {error}")));
        list = list.push(
            button("Retry scan")
                .on_press_maybe(can_select.then_some(Message::Macro(Macro::ReadCatalog))),
        );
    } else if let Some(configured) = configured {
        list = list.push(text(format!(
            "{} of {capacity} slots used",
            configured.len()
        )));
    } else {
        list = list.push(text("Finding saved macros…"));
    }
    panels::panel(style, "Library", list.width(Fill).into())
}

pub(super) fn editor(app: &Desktop) -> Element<'_, Message> {
    let Some(editor) = app.session.macros() else {
        return text("Macros are unavailable").into();
    };
    let selected = app
        .session
        .macro_library_slots()
        .is_some_and(|slots| slots.iter().any(|choice| choice.id == editor.slot()))
        || app.macro_new_slot.as_deref() == Some(editor.slot());
    let selected = selected
        || editor
            .baseline()
            .is_some_and(|snapshot| snapshot.slot == editor.slot());
    if !selected {
        return panels::panel(
            &app.ui,
            "Macro editor",
            text("Select a macro or add one to begin").into(),
        );
    }
    let editable = !app.busy() && *editor.status() == Status::Ready && editor.draft().is_some();
    let can_save = editable && editor.dirty() && editor.request_apply().is_ok();
    let mut content = column![
        super::macro_files::name_controls(app, editor),
        super::recording::controls(app, editable)
    ]
    .spacing(app.ui.spacing.s)
    .width(Fill);
    if let Some(reason) = &app.macro_notice {
        content = content.push(text(reason));
    }
    if *editor.status() != Status::Ready || app.busy() {
        content = content.push(text(status(app, editor)));
        if !app.busy() && *editor.status() != Status::Unloaded {
            content = content.push(button("Retry read").on_press(Message::Macro(Macro::Read)));
        }
    }
    if let Some(snapshot) = editor.baseline()
        && let Content::Opaque { reason } = &snapshot.content
    {
        content = content.push(text(format!("Preserved as read-only: {reason}")));
    }
    if let Some(program) = editor.draft() {
        if !editor
            .capabilities()
            .editable_repeat_counts
            .contains(&program.repeat_count)
        {
            content = content.push(text(format!(
                "This slot has a stored repeat count of {}. Choose a count before saving.",
                program.repeat_count
            )));
            let count = *editor.capabilities().editable_repeat_counts.start();
            content = content.push(button(text(format!("Use repeat {count}"))).on_press_maybe(
                editable.then_some(Message::Macro(Macro::Edit(Edit::Repeat(count)))),
            ));
        }
        let repeat = text_input("Count", &app.repeat_input)
            .on_input_maybe(editable.then_some(|value| Message::Macro(Macro::RepeatInput(value))))
            .width(app.ui.fields.compact);
        content = content.push(text(format!("{} events", program.events.len())));
        let events = column(program.events.iter().enumerate().map(|(index, event)| {
            row![
                button(text(format!(
                    "{:02}  {}",
                    index + 1,
                    event_label(event, app, editor)
                )))
                .width(Fill)
                .on_press_maybe(editable.then_some(Message::Macro(Macro::Inspect(index)))),
                button("↑").on_press_maybe((editable && index > 0).then_some(Message::Macro(
                    Macro::Edit(Edit::Move {
                        from: index,
                        to: index.saturating_sub(1)
                    })
                ))),
                button("↓").on_press_maybe(
                    (editable && index + 1 < program.events.len()).then_some(Message::Macro(
                        Macro::Edit(Edit::Move {
                            from: index,
                            to: index + 1
                        })
                    ))
                ),
                button("×").on_press_maybe(
                    editable.then_some(Message::Macro(Macro::Edit(Edit::Remove { at: index })))
                ),
            ]
            .spacing(app.ui.spacing.xs)
            .into()
        }))
        .spacing(app.ui.spacing.xs);
        if program.events.is_empty() {
            content = content.push(text(
                "Record input to build a macro, or add an event manually.",
            ));
        } else {
            content = content.push(scrollable(events).height(Fill));
        }
        content = content.push(
            row![
                text("Repeat"),
                repeat,
                button("Set repeat")
                    .on_press_maybe(editable.then_some(Message::Macro(Macro::StageRepeat))),
                button("Clear events").on_press_maybe(
                    (editable && !program.events.is_empty())
                        .then_some(Message::Macro(Macro::Edit(Edit::Clear)))
                ),
            ]
            .spacing(app.ui.spacing.s),
        );
        content = content.push(
            button(if app.macro_composer == Composer::Expanded {
                "Hide manual event editor"
            } else {
                "Add or edit event manually"
            })
            .on_press_maybe(editable.then_some(Message::Macro(Macro::ToggleComposer))),
        );
        if app.macro_composer == Composer::Expanded {
            content = content.push(composer(app, editor, editable));
        }
    }
    content = content.push(
        row![
            button("Save macro").on_press_maybe(can_save.then_some(Message::Macro(Macro::Apply))),
            button("Revert").on_press_maybe(
                (editable && editor.dirty()).then_some(Message::Macro(Macro::Revert))
            ),
        ]
        .spacing(app.ui.spacing.s),
    );
    content = content.push(super::macro_files::file_controls(app, editor));
    panels::panel(
        &app.ui,
        "Macro editor",
        content.width(Fill).height(Fill).into(),
    )
}

fn composer<'a>(app: &'a Desktop, editor: &Editor, editable: bool) -> Element<'a, Message> {
    let form = &app.macro_form;
    let caps = editor.capabilities();
    let choices = macro_form::kinds(caps);
    let kind: Element<'_, Message> = if editable {
        pick_list(choices, form.kind.clone(), |kind| {
            Message::Macro(Macro::Form(Input::Kind(kind)))
        })
        .placeholder("Event type")
        .into()
    } else {
        text("Read an editable slot to stage events").into()
    };
    let mut fields = row![kind].spacing(app.ui.spacing.s);
    match &form.kind {
        Some(Kind::Key) => {
            let mut keys: Vec<KeyChoice> = app
                .session
                .descriptor()
                .actions
                .iter()
                .filter_map(|choice| match choice.action {
                    KeyAction::Key(usage)
                        if caps
                            .keys
                            .as_ref()
                            .is_some_and(|range| range.contains(&usage)) =>
                    {
                        Some(KeyChoice {
                            usage,
                            label: choice.label.clone(),
                        })
                    }
                    _ => None,
                })
                .collect();
            let selected = form.first.parse::<u16>().ok();
            if let Some(usage) = selected
                && !keys.iter().any(|choice| choice.usage == usage)
            {
                keys.push(KeyChoice {
                    usage,
                    label: format!("Key {usage}"),
                });
            }
            keys.sort_by(|a, b| a.label.cmp(&b.label));
            let current =
                selected.and_then(|usage| keys.iter().find(|key| key.usage == usage).cloned());
            fields = fields.push(
                pick_list(keys, current, |key: KeyChoice| {
                    Message::Macro(Macro::Form(Input::First(key.usage.to_string())))
                })
                .placeholder("Choose a key")
                .width(app.ui.fields.regular),
            );
        }
        Some(Kind::Move) => {
            fields = fields
                .push(
                    text_input("Horizontal", &form.first)
                        .on_input_maybe(
                            editable.then_some(|v| Message::Macro(Macro::Form(Input::First(v)))),
                        )
                        .width(app.ui.fields.compact),
                )
                .push(
                    text_input("Vertical", &form.second)
                        .on_input_maybe(
                            editable.then_some(|v| Message::Macro(Macro::Form(Input::Second(v)))),
                        )
                        .width(app.ui.fields.compact),
                );
        }
        _ => {}
    }
    if form.kind.is_some() && form.kind != Some(Kind::Move) {
        fields = fields.push(
            checkbox(form.pressed)
                .label("Pressed / flag set")
                .on_toggle_maybe(
                    editable.then_some(|v| Message::Macro(Macro::Form(Input::Pressed(v)))),
                ),
        );
    }
    let limits = match &form.kind {
        Some(Kind::Key) => String::new(),
        Some(Kind::Move) => format!("Each axis {:?}", caps.movement),
        _ => String::new(),
    };
    column![
        text(form.target.map_or_else(
            || "Append event".into(),
            |index| format!("Replace event {}", index + 1)
        )),
        fields,
        row![
            text("Wait after (ms)"),
            text_input("Wait", &form.wait)
                .on_input_maybe(editable.then_some(|v| Message::Macro(Macro::Form(Input::Wait(v)))))
                .width(app.ui.fields.compact),
            button("Stage event")
                .on_press_maybe(editable.then_some(Message::Macro(Macro::StageEvent))),
            button("New event").on_press_maybe(editable.then_some(Message::Macro(Macro::NewEvent))),
        ]
        .spacing(app.ui.spacing.s),
        text(format!(
            "{limits} · wait {:?} ms · repeat {:?}",
            caps.delays_ms, caps.editable_repeat_counts
        ))
        .size(app.ui.type_scale.body),
    ]
    .spacing(app.ui.spacing.s)
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

fn event_label(event: &Event, app: &Desktop, editor: &Editor) -> String {
    let (label, edge) = match &event.action {
        Action::Key { usage, pressed } => (
            app.session
                .descriptor()
                .actions
                .iter()
                .find(|choice| choice.action == KeyAction::Key(*usage))
                .map_or_else(|| format!("Key {usage}"), |choice| choice.label.clone()),
            Some(*pressed),
        ),
        Action::Button { button, pressed } => (
            editor
                .capabilities()
                .buttons
                .iter()
                .find(|c| c.button == *button)
                .map_or_else(|| format!("Button {button}"), |c| c.label.clone()),
            Some(*pressed),
        ),
        Action::Move { dx, dy } => (format!("Move {dx}, {dy}"), None),
        Action::Backend { id, pressed, .. } => (
            editor
                .capabilities()
                .backend_actions
                .iter()
                .find(|c| c.id == *id)
                .map_or_else(|| id.clone(), |c| c.label.clone()),
            Some(*pressed),
        ),
    };
    let edge = match edge {
        Some(true) => " down",
        Some(false) => " up",
        None => "",
    };
    format!("{label}{edge} · wait {} ms", event.delay_ms)
}

fn status(app: &Desktop, editor: &Editor) -> String {
    if app.busy() {
        return super::view::status(app);
    }
    match editor.status() {
        Status::Ready => "Slot readback verified · edits remain staged until saved".into(),
        Status::Unloaded => "Select a slot to read it".into(),
        Status::Unverified { problem } => super::view::problem_label(problem),
        Status::Conflict { .. } => "Slot changed on the device. Draft retained; revert it, then read again to use device values.".into(),
    }
}
