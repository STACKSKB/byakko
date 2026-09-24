//! Binding choices are advertised actions, not UI-generated firmware codes.
use super::{Desktop, Message, macro_editor::Message as Macro, panels};
use byakko_core::{macros::editor::Editor, session::Status};
use iced::{
    Element,
    widget::{button, column, row, text},
};

pub(super) fn view<'a>(app: &'a Desktop, editor: &'a Editor) -> Element<'a, Message> {
    let choices: Vec<_> = editor
        .capabilities()
        .bindings
        .iter()
        .filter(|binding| binding.slot == editor.slot())
        .collect();
    if choices.is_empty() {
        return text("This slot has no key-binding choices").into();
    }
    let descriptor = app.session.descriptor();
    let key = descriptor
        .keys
        .iter()
        .find(|key| Some(&key.id) == app.selected.as_ref());
    let ready = !app.busy()
        && *app.session.status() == Status::Ready
        && key.is_some_and(|key| key.writable);
    let selected = app
        .macro_binding_choice
        .as_ref()
        .filter(|(slot, _)| slot == editor.slot())
        .map(|(_, id)| id.as_str());
    let modes = row(choices.iter().map(|choice| {
        panels::selectable_button(
            &app.ui,
            &choice.label,
            selected == Some(choice.id.as_str()),
            (!app.busy()).then(|| Message::Macro(Macro::ChooseBinding(choice.id.clone()))),
        )
    }))
    .spacing(app.ui.spacing.s)
    .wrap();
    let selected_choice = choices
        .iter()
        .find(|choice| selected == Some(choice.id.as_str()));
    let restriction = selected_choice.and_then(|choice| editor.binding_action(&choice.id).err());
    let can_assign = ready
        && selected_choice.is_some()
        && restriction.is_none()
        && app.session.changes().is_empty();
    let mut content = column![
        modes,
        button("Assign to key").on_press_maybe(
            selected_choice
                .filter(|_| can_assign)
                .map(|choice| Message::Macro(Macro::Assign(choice.id.clone())))
        ),
    ]
    .spacing(app.ui.spacing.s);
    if key.is_none() {
        content = content.push(text("Select a key on the keyboard to assign this macro."));
    }
    if let Some(choice) = selected_choice
        && let Some(required) = choice.required_repeat_count
        && editor
            .draft()
            .is_some_and(|program| program.repeat_count != required)
    {
        content = content.push(text(format!(
            "Save this macro with repeat count {required} to use this mode."
        )));
    } else if let Some(reason) = restriction {
        content = content.push(text(reason));
    }
    if !app.session.changes().is_empty() {
        content = content.push(text(
            "Save or revert other key assignments before assigning this macro.",
        ));
    }
    if *app.session.status() != Status::Ready {
        content = content.push(
            row![
                text("Read keymaps before assigning."),
                button("Read keymaps").on_press_maybe((!app.busy()).then_some(Message::Read)),
            ]
            .spacing(app.ui.spacing.s),
        );
    }
    panels::panel(&app.ui, "Playback", content.into())
}
