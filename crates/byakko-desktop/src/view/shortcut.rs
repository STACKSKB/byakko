//! Device-described shortcut choices; composing them has no device effects.
use crate::{
    form::shortcut::{Form, Message},
    widget::panels::{self, UiStyle},
};
use byakko_core::model::keymap::ShortcutCapabilities;
use iced::{
    Element, Fill,
    widget::{button, column, row, text, text_input},
};

pub fn view<'a>(
    form: &'a Form,
    caps: &'a ShortcutCapabilities,
    editable: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    let modifiers = row(caps.modifiers.iter().map(|choice| {
        panels::selectable_button(
            style,
            &choice.label,
            form.modifiers.contains(&choice.usage),
            editable.then_some(Message::ToggleModifier(choice.usage)),
        )
    }))
    .spacing(style.spacing.s)
    .wrap();
    let query = form.query.trim().to_lowercase();
    let matches = row(caps
        .keys
        .iter()
        .filter(|choice| !query.is_empty() && choice.label.to_lowercase().contains(&query))
        .take(12)
        .map(|choice| {
            panels::selectable_button(
                style,
                &choice.label,
                form.key == Some(choice.usage),
                editable.then_some(Message::SelectKey(choice.usage)),
            )
        }))
    .spacing(style.spacing.xs)
    .wrap();
    let selected = caps
        .keys
        .iter()
        .find(|choice| Some(choice.usage) == form.key)
        .map_or("Choose a key", |choice| choice.label.as_str());
    column![
        text("Shortcut").size(style.type_scale.section_title),
        modifiers,
        text(form.error.as_deref().unwrap_or(" ")).style(text::danger),
        text_input("Find shortcut key…", &form.query)
            .on_input_maybe(editable.then_some(Message::Search)),
        matches,
        text(selected),
        button("Stage shortcut")
            .on_press_maybe((editable && form.action(caps).is_ok()).then_some(Message::Stage)),
    ]
    .spacing(style.spacing.s)
    .width(Fill)
    .into()
}
