//! Macro widgets project the shared editor, library and unsubmitted form.
use crate::{
    form::macros::{Form, Kind, Message},
    widget::panels::UiStyle,
};
use byakko_core::{
    editor::{Editor, Status, macros::MacroRules},
    library::macros::{Library, Occupancy},
    model::macros::{Action, Content},
};
use iced::{
    Element, Fill,
    widget::{button, checkbox, column, pick_list, row, scrollable, text, text_input},
};
pub fn view<'a>(
    form: &'a Form,
    editor: &'a Editor<MacroRules>,
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
                text_input("Count", &form.repeat)
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
                pick_list(kinds, Some(form.kind), Message::Kind),
                text_input("Usage / button / X / action ID", &form.value).on_input(Message::Value),
                text_input("Delay (ms)", &form.delay)
                    .on_input(Message::Delay)
                    .width(style.fields.compact),
            ]
            .spacing(style.spacing.s),
        );
        if form.kind == Kind::Movement {
            detail = detail.push(text_input("Y movement", &form.second).on_input(Message::Second));
        } else {
            detail = detail.push(
                checkbox(form.pressed)
                    .label("Pressed (clear for release)")
                    .on_toggle(Message::Pressed),
            );
        }
        detail = detail.push(
            row![
                button("Append event").on_press_maybe(editable.then_some(Message::Insert)),
                button("Replace selected").on_press_maybe(
                    (editable && form.selected.is_some()).then_some(Message::Replace)
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

fn action_label(action: &Action) -> String {
    let direction = |pressed| if pressed { "press" } else { "release" };
    match action {
        Action::Key { usage, pressed } => format!("Key {usage} {}", direction(*pressed)),
        Action::Button { button, pressed } => format!("Button {button} {}", direction(*pressed)),
        Action::Move { dx, dy } => format!("Move {dx}, {dy}"),
        Action::Backend { id, pressed, .. } => format!("{id} {}", direction(*pressed)),
    }
}
