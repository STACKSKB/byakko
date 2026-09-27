//! Assignment controls sit below the persistent keyboard, with the catalog beside it.
use crate::{
    form::{application::Message, files, keymap, keymap::Form},
    widget::{
        keyboard,
        panels::{self, UiStyle},
    },
};
use byakko_core::session::Session;
use iced::{
    Element,
    widget::{button, column, row, text},
};

pub fn view<'a>(
    form: &'a Form,
    session: &'a Session,
    names: &'a files::Form,
    editable: bool,
    idle: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    let descriptor = session.descriptor();
    let editor = session.keymap();
    let mut toolbar = layers(form, session, editable, style);
    if editor.dirty() {
        toolbar = toolbar
            .push(
                button("Save assignments")
                    .on_press_maybe((editable && idle).then_some(Message::Save)),
            )
            .push(button("Revert").on_press_maybe(idle.then_some(Message::Revert)));
    }
    let mut detail = column![toolbar].spacing(style.spacing.m);
    if let Some(caps) = &descriptor.shortcuts {
        detail = detail.push(
            super::shortcut::view(
                &form.shortcut,
                caps,
                editable && form.can_assign(editor),
                style,
            )
            .map(|message| Message::Keys(keymap::Message::Shortcut(message))),
        );
    }
    let changes = editor.changes();
    if !changes.is_empty() {
        detail = detail.push(text("Changes to save"));
        for change in changes {
            let labels = |draft| {
                keyboard::labels_for_layer(descriptor, draft, &change.layer, |action| {
                    session
                        .macros()
                        .and_then(|editor| names.assignment_name(action, editor.capabilities()))
                        .map(str::to_owned)
                })
            };
            let before = labels(editor.baseline().map(|state| &state.bindings));
            let after = labels(editor.draft());
            let key = descriptor
                .keys
                .iter()
                .find(|key| key.id == change.key)
                .map_or(change.key.as_str(), |key| key.label.as_str());
            let layer = descriptor
                .layers
                .iter()
                .find(|layer| layer.id == change.layer)
                .map_or(change.layer.as_str(), |layer| layer.label.as_str());
            detail = detail.push(text(format!(
                "{layer} / {key}: {} → {}",
                before
                    .get(&change.key)
                    .map_or("Unknown", |label| label.full.as_str()),
                after
                    .get(&change.key)
                    .map_or("Unknown", |label| label.full.as_str())
            )));
        }
    }
    detail.into()
}

/// Keys and macro assignments share the same selected keymap layer.
pub fn layers<'a>(
    form: &Form,
    session: &'a Session,
    idle: bool,
    style: &'a UiStyle,
) -> iced::widget::Row<'a, Message> {
    row(session.descriptor().layers.iter().map(|layer| {
        panels::selectable_button(
            style,
            &layer.label,
            layer.id == form.layer,
            idle.then(|| Message::Keys(keymap::Message::Layer(layer.id.clone()))),
        )
    }))
    .spacing(style.spacing.s)
}
