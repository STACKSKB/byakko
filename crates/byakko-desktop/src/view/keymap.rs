//! Keymap widgets project the shared editor and unsubmitted form.
use crate::{
    form::keymap::{Form, Message},
    widget::{keyboard as physical_board, panels::UiStyle},
};
use byakko_core::{
    editor::{Editor, keymap::KeymapRules},
    model::keymap::Descriptor,
};
use iced::{
    Element, Fill,
    widget::{button, column, container, row, scrollable, text, text_input},
};
pub fn workspace<'a>(
    form: &'a Form,
    descriptor: &'a Descriptor,
    editor: &'a Editor<KeymapRules>,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    let layers = row(descriptor.layers.iter().map(|layer| {
        crate::widget::panels::selectable_button(
            style,
            &layer.label,
            layer.id == form.layer,
            Some(Message::Layer(layer.id.clone())),
        )
    }))
    .spacing(style.spacing.s);
    let board = physical_board::view_with_labels(
        style,
        descriptor.keys.iter().filter(|key| key.visible).collect(),
        form.selected.clone(),
        physical_board::labels_for_layer(descriptor, editor.draft(), &form.layer),
        |key| Some(Message::Key(key.id.clone())),
    );
    column![layers, board].spacing(style.spacing.m).into()
}

pub fn view<'a>(
    form: &'a Form,
    descriptor: &'a Descriptor,
    editor: &'a Editor<KeymapRules>,
    editable: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    let can_assign = editable
        && form.selected.as_ref().is_some_and(|id| {
            descriptor
                .keys
                .iter()
                .any(|key| &key.id == id && key.writable)
        });
    let query = form.search.to_lowercase();
    let choices = column(
        descriptor
            .actions
            .iter()
            .enumerate()
            .filter(|(_, choice)| choice.label.to_lowercase().contains(&query))
            .map(|(index, choice)| {
                button(text(&choice.label))
                    .on_press_maybe(can_assign.then_some(Message::Assign(index)))
                    .into()
            }),
    )
    .spacing(style.spacing.s);
    let assignments = column![
        text_input("Search assignments", &form.search).on_input(Message::Search),
        scrollable(choices).height(Fill)
    ]
    .spacing(style.spacing.s);
    column![
        workspace(form, descriptor, editor, style),
        container(assignments).width(Fill).height(Fill)
    ]
    .spacing(style.spacing.l)
    .into()
}
