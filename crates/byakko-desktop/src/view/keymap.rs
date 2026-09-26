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
    widget::{column, container, row},
};
pub fn workspace<'a>(
    form: &'a Form,
    descriptor: &'a Descriptor,
    editor: &'a Editor<KeymapRules>,
    interactive: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    let layers = row(descriptor.layers.iter().map(|layer| {
        crate::widget::panels::selectable_button(
            style,
            &layer.label,
            layer.id == form.layer,
            interactive.then(|| Message::Layer(layer.id.clone())),
        )
    }))
    .spacing(style.spacing.s);
    let board = physical_board::view_with_labels(
        style,
        descriptor.keys.iter().filter(|key| key.visible).collect(),
        form.selected.clone(),
        physical_board::labels_for_layer(descriptor, editor.draft(), &form.layer),
        move |key| interactive.then(|| Message::Key(key.id.clone())),
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
    let can_assign = editable && form.can_assign(editor);
    let mut assignments = row![super::catalog::view(
        &form.catalog,
        &descriptor.actions,
        form.selected_action(editor),
        can_assign,
        style
    )]
    .spacing(style.spacing.l);
    if let Some(caps) = &descriptor.shortcuts {
        assignments = assignments.push(
            container(
                super::shortcut::view(&form.shortcut, caps, can_assign, style)
                    .map(Message::Shortcut),
            )
            .width(style.fields.regular),
        );
    }
    column![
        workspace(form, descriptor, editor, true, style),
        container(assignments).width(Fill).height(Fill)
    ]
    .spacing(style.spacing.l)
    .into()
}
