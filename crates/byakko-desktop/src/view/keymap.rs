//! Selected key, edit mode and pending assignment review in the inspector.
use crate::widget::focus_button::button;
use crate::{
    form::{
        application::Message,
        files, keymap,
        keymap::{Form, Pane, PresetChoice},
    },
    widget::{
        keyboard,
        panels::{self, UiStyle},
    },
};
use byakko_core::session::Session;
use iced::{
    Element,
    widget::{column, row, text},
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
    let selected = form
        .selected
        .as_ref()
        .and_then(|id| descriptor.keys.iter().find(|key| key.id == *id))
        .map_or("Choose a key", |key| key.label.as_str());
    let tabs = row![
        panels::selectable_button(
            style,
            "Remap",
            form.pane == Pane::Remap,
            Some(Message::Keys(keymap::Message::Pane(Pane::Remap))),
        ),
        panels::selectable_button(
            style,
            "Shortcut",
            form.pane == Pane::Shortcut,
            descriptor
                .shortcuts
                .as_ref()
                .map(|_| Message::Keys(keymap::Message::Pane(Pane::Shortcut))),
        ),
        panels::selectable_button(
            style,
            "Typing preset",
            form.pane == Pane::TypingPreset,
            (descriptor.backend_id == "nia87")
                .then_some(Message::Keys(keymap::Message::Pane(Pane::TypingPreset))),
        ),
    ]
    .spacing(style.spacing.xs);
    let mut toolbar = row![
        text(selected).size(style.type_scale.section_title),
        text(format!(" · {}", form.layer)).size(style.type_scale.body),
    ]
    .spacing(style.spacing.xs);
    if editor.dirty() {
        toolbar = toolbar
            .push(
                button("Save assignments")
                    .on_press_maybe((editable && idle).then_some(Message::Save)),
            )
            .push(button("Revert").on_press_maybe(idle.then_some(Message::Revert)));
    }
    let mut detail = column![toolbar, tabs].spacing(style.spacing.m);
    if form.pane == Pane::Shortcut
        && let Some(caps) = &descriptor.shortcuts
    {
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
    if form.pane == Pane::TypingPreset {
        detail = detail.push(
            column![
                text("Typing layout"),
                row![
                    panels::selectable_button(
                        style,
                        "Colemak",
                        form.preset == PresetChoice::Colemak,
                        Some(Message::Keys(keymap::Message::ChoosePreset(
                            PresetChoice::Colemak
                        ))),
                    ),
                    panels::selectable_button(
                        style,
                        "Dvorak",
                        form.preset == PresetChoice::Dvorak,
                        Some(Message::Keys(keymap::Message::ChoosePreset(
                            PresetChoice::Dvorak
                        ))),
                    ),
                ]
                .spacing(style.spacing.s),
                button("Stage typing preset").on_press_maybe(
                    editable.then_some(Message::Keys(keymap::Message::StagePreset)),
                ),
            ]
            .spacing(style.spacing.s),
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
