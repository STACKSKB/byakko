//! Macro widgets project capabilities and the shared draft; no firmware assumptions.
use super::{
    Desktop, Message,
    macro_editor::Message as Macro,
    macro_form::{self, Input, Kind},
    panels,
};
use byakko_core::macros::{
    Action, Choice, Content, Edit, Event,
    editor::{Editor, Status},
};
use iced::{
    Element, Fill, Size,
    widget::{
        button, checkbox, column, container, pick_list, responsive, row, scrollable, text,
        text_input,
    },
};

pub(super) fn view(app: &Desktop) -> Element<'_, Message> {
    let Some(editor) = app.session.macros() else {
        return text("This device has no macro editor").into();
    };
    if editor.catalog().is_none() {
        let pending = editor.catalog_error();
        let message = pending.map_or_else(
            || "Reading stored macros…".to_owned(),
            |error| format!("Could not read the macro library: {error}"),
        );
        let body = column![
            text(message),
            button("Retry read").on_press_maybe(
                (pending.is_some() && !app.busy()).then_some(Message::Macro(Macro::ReadCatalog))
            )
        ]
        .spacing(app.ui.spacing.s);
        return panels::panel(&app.ui, "Macros", body.into());
    }
    panels::split(&app.ui, || slots(app, editor), || detail(app, editor))
}

fn slots<'a>(app: &'a Desktop, editor: &'a Editor) -> Element<'a, Message> {
    let style = &app.ui;
    let configured = app
        .session
        .macro_library_slots()
        .expect("catalog checked by view");
    let occupied = configured.len();
    let capacity = editor.capabilities().slots.len();
    let configured_ids: std::collections::BTreeSet<_> =
        configured.iter().map(|choice| choice.id.as_str()).collect();
    let slots: Vec<&Choice> = editor
        .capabilities()
        .slots
        .iter()
        .filter(|choice| {
            configured_ids.contains(choice.id.as_str())
                || app.macro_new_slot.as_deref() == Some(choice.id.as_str())
        })
        .collect();
    let no_slots = slots.is_empty();
    let slot_id = editor.slot();
    let can_select = !app.busy();
    let can_add =
        can_select && app.session.next_free_macro_slot().is_some() && app.macro_new_slot.is_none();
    let toolbar = row![
        button("Add macro").on_press_maybe(can_add.then_some(Message::Macro(Macro::Add))),
        text(format!("{occupied} / {capacity}")),
    ]
    .spacing(style.spacing.s);
    let min_cell_width = style.choice_grid_min_cell_width;
    let gap = style.spacing.xs;
    let inset = style.scrollbar_inset;
    let scrollbar_width = style.scrollbar_width;
    let choices = responsive(move |size: Size| {
        let inner_width =
            (size.width - f32::from(inset) * 3.0 - f32::from(scrollbar_width)).max(0.0);
        let columns =
            (((inner_width + gap as f32) / (min_cell_width + gap as f32)).floor() as usize).max(1);
        let rows = slots.chunks(columns).map(|choices| {
            row(choices.iter().map(|choice| {
                panels::selectable_button_fill_width(
                    style,
                    app.macro_files
                        .slot_label(&choice.id, &choice.label)
                        .to_owned(),
                    choice.id == slot_id,
                    can_select.then(|| Message::Macro(Macro::Select(choice.id.clone()))),
                )
            }))
            .spacing(gap)
            .width(Fill)
            .into()
        });
        let grid = column(rows).spacing(gap).width(Fill);
        container(
            scrollable(grid)
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new()
                        .width(u32::from(scrollbar_width))
                        .spacing(u32::from(inset)),
                ))
                .width(Fill)
                .height(Fill),
        )
        .padding([0.0, f32::from(inset)])
        .width(Fill)
        .height(Fill)
        .into()
    });
    let list: Element<'_, Message> = if no_slots {
        text("No macros stored yet").into()
    } else {
        choices.into()
    };
    panels::panel(
        style,
        "Macros",
        column![toolbar, list]
            .spacing(style.spacing.s)
            .height(Fill)
            .into(),
    )
}

fn detail<'a>(app: &'a Desktop, editor: &'a Editor) -> Element<'a, Message> {
    let selected = app
        .session
        .macro_library_slots()
        .is_some_and(|slots| slots.iter().any(|choice| choice.id == editor.slot()))
        || app.macro_new_slot.as_deref() == Some(editor.slot());
    if !selected {
        return panels::panel(
            &app.ui,
            "Macro editor",
            text("Select a macro or add one to begin").into(),
        );
    }
    let editable = !app.busy() && *editor.status() == Status::Ready && editor.draft().is_some();
    let can_save = editable && editor.dirty() && editor.request_apply().is_ok();
    let toolbar = row![
        button("Read slot").on_press_maybe((!app.busy()).then_some(Message::Macro(Macro::Read))),
        button("Revert draft").on_press_maybe(
            (!app.busy() && editor.dirty()).then_some(Message::Macro(Macro::Revert))
        ),
        button("Save & verify").on_press_maybe(can_save.then_some(Message::Macro(Macro::Apply))),
        text(if editor.dirty() {
            "Staged changes"
        } else {
            "No staged changes"
        }),
    ]
    .spacing(app.ui.spacing.m);
    let mut content = column![toolbar, text(status(app, editor))].spacing(app.ui.spacing.m);
    content = content.push(super::recording::controls(app, editable));
    content = content.push(super::macro_files::view(app, editor));
    content = content.push(super::macro_binding_view::view(app, editor));
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
                "Stored count {} is preserved; stage a count in {:?} before saving or binding.",
                program.repeat_count,
                editor.capabilities().editable_repeat_counts
            )));
        }
        let repeat = text_input("Count", &app.repeat_input)
            .on_input_maybe(editable.then_some(|value| Message::Macro(Macro::RepeatInput(value))))
            .width(app.ui.fields.compact);
        content = content.push(
            row![
                text(format!("Stored repeat count: {}", program.repeat_count)),
                repeat,
                button("Stage count")
                    .on_press_maybe(editable.then_some(Message::Macro(Macro::StageRepeat))),
                button("Clear events").on_press_maybe(
                    (editable && !program.events.is_empty())
                        .then_some(Message::Macro(Macro::Edit(Edit::Clear)))
                ),
            ]
            .spacing(app.ui.spacing.m),
        );
        let events = column(program.events.iter().enumerate().map(|(index, event)| {
            row![
                button(text(format!(
                    "{:02}  {}",
                    index + 1,
                    event_label(event, editor)
                )))
                .width(Fill)
                .on_press_maybe(editable.then_some(Message::Macro(Macro::Inspect(index)))),
                button("Up").on_press_maybe((editable && index > 0).then_some(Message::Macro(
                    Macro::Edit(Edit::Move {
                        from: index,
                        to: index.saturating_sub(1)
                    })
                ))),
                button("Down").on_press_maybe(
                    (editable && index + 1 < program.events.len()).then_some(Message::Macro(
                        Macro::Edit(Edit::Move {
                            from: index,
                            to: index + 1
                        })
                    ))
                ),
                button("Remove").on_press_maybe(
                    editable.then_some(Message::Macro(Macro::Edit(Edit::Remove { at: index })))
                ),
            ]
            .spacing(app.ui.spacing.xs)
            .into()
        }))
        .spacing(app.ui.spacing.xs);
        content = content
            .push(scrollable(events).height(Fill))
            .push(composer(app, editor, editable));
    }
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
            fields = fields.push(
                text_input("Decimal HID usage", &form.first)
                    .on_input_maybe(
                        editable.then_some(|v| Message::Macro(Macro::Form(Input::First(v)))),
                    )
                    .width(app.ui.fields.regular),
            )
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
        Some(Kind::Key) => format!("HID usages {:?}", caps.keys),
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

fn event_label(event: &Event, editor: &Editor) -> String {
    let (label, edge) = match &event.action {
        Action::Key { usage, pressed } => (format!("Key {usage}"), Some(*pressed)),
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
        Some(true) => " · pressed/flag set",
        Some(false) => " · released/flag clear",
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
