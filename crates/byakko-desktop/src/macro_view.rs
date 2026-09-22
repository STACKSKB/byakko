//! Macro widgets project capabilities and the shared draft; no firmware assumptions.
use super::{
    Desktop, Message,
    macro_editor::Message as Macro,
    macro_form::{self, Input, Kind},
};
use byakko_core::macros::{
    Action, Content, Edit, Event,
    editor::{Editor, Status},
};
use iced::{
    Element, Fill,
    widget::{button, checkbox, column, pick_list, row, scrollable, text, text_input},
};

pub(super) fn view(app: &Desktop) -> Element<'_, Message> {
    let Some(editor) = app.session.macros() else {
        return text("This device has no macro editor").into();
    };
    let editable = !app.busy() && *editor.status() == Status::Ready && editor.draft().is_some();
    let slots = column(editor.capabilities().slots.iter().map(|choice| {
        button(text(&choice.label))
            .width(Fill)
            .style(if choice.id == editor.slot() {
                button::primary
            } else {
                button::secondary
            })
            .on_press_maybe((!app.busy()).then(|| Message::Macro(Macro::Select(choice.id.clone()))))
            .into()
    }))
    .spacing(4);
    let toolbar = row![
        button("Read slot").on_press_maybe((!app.busy()).then_some(Message::Macro(Macro::Read))),
        button("Revert draft").on_press_maybe(
            (!app.busy() && editor.dirty()).then_some(Message::Macro(Macro::Revert))
        ),
        button("Save & verify")
            .on_press_maybe((editable && editor.dirty()).then_some(Message::Macro(Macro::Apply))),
        text(if editor.dirty() {
            "Staged changes"
        } else {
            "No staged changes"
        }),
    ]
    .spacing(10);
    let mut content = column![toolbar, text(status(app, editor))].spacing(10);
    if let Some(snapshot) = editor.baseline()
        && let Content::Opaque { reason } = &snapshot.content
    {
        content = content.push(text(format!("Preserved as read-only: {reason}")));
    }
    if let Some(program) = editor.draft() {
        let repeat = text_input("Count", &app.repeat_input)
            .on_input_maybe(editable.then_some(|value| Message::Macro(Macro::RepeatInput(value))))
            .width(100);
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
            .spacing(10),
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
            .spacing(5)
            .into()
        }))
        .spacing(4);
        content = content
            .push(scrollable(events).height(Fill))
            .push(composer(app, editor, editable));
    }
    row![
        scrollable(slots).width(180).height(Fill),
        content.width(Fill).height(Fill)
    ]
    .spacing(18)
    .height(Fill)
    .into()
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
    let mut fields = row![kind].spacing(8);
    match &form.kind {
        Some(Kind::Key) => {
            fields = fields.push(
                text_input("Decimal HID usage", &form.first)
                    .on_input_maybe(
                        editable.then_some(|v| Message::Macro(Macro::Form(Input::First(v)))),
                    )
                    .width(160),
            )
        }
        Some(Kind::Move) => {
            fields = fields
                .push(
                    text_input("Horizontal", &form.first)
                        .on_input_maybe(
                            editable.then_some(|v| Message::Macro(Macro::Form(Input::First(v)))),
                        )
                        .width(100),
                )
                .push(
                    text_input("Vertical", &form.second)
                        .on_input_maybe(
                            editable.then_some(|v| Message::Macro(Macro::Form(Input::Second(v)))),
                        )
                        .width(100),
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
                .width(100),
            button("Stage event")
                .on_press_maybe(editable.then_some(Message::Macro(Macro::StageEvent))),
            button("New event").on_press_maybe(editable.then_some(Message::Macro(Macro::NewEvent))),
        ]
        .spacing(8),
        text(format!(
            "{limits} · wait {:?} ms · repeat {:?}",
            caps.delays_ms, caps.repeat_counts
        ))
        .size(13),
    ]
    .spacing(8)
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
