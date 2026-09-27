//! Physical-key color selection uses advertised picture keys and the core draft.
use crate::{
    form::picture::{Form, Message},
    widget::{
        color_picker, keyboard,
        panels::{self, UiStyle},
    },
};
use byakko_core::{
    editor::{Editor, Status, picture::PictureRules},
    model::{keymap::Descriptor, picture::Content},
    projection::picture::channels,
};
use iced::{
    Element, Fill,
    widget::{button, column, row, slider, text},
};
use std::collections::BTreeMap;

pub fn view<'a>(
    form: &'a Form,
    descriptor: &'a Descriptor,
    editor: &'a Editor<PictureRules>,
    editable: bool,
    idle: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    let editable = editable && editor.status() == &Status::Ready && editor.draft().is_some();
    let advertised = &editor.capabilities().keys;
    let board = keyboard::colored_view_with_labels(
        style,
        descriptor.keys.iter().filter(|key| key.visible).collect(),
        form.selected.clone(),
        editor.draft().cloned().unwrap_or_default(),
        BTreeMap::new(),
        move |key| {
            (editable && advertised.contains(&key.id)).then(|| Message::Select(key.id.clone()))
        },
    );
    let mut content = column![
        board,
        row![
            button("Read").on_press_maybe(idle.then_some(Message::Read)),
            button("Revert").on_press_maybe((idle && editor.dirty()).then_some(Message::Revert)),
            button("Apply now")
                .on_press_maybe((idle && editable && editor.dirty()).then_some(Message::Save)),
        ]
        .spacing(style.spacing.s),
        text("Select a key to paint. Changes apply automatically after a pause."),
    ]
    .spacing(style.spacing.m);
    if let Some((key, rgb)) = form
        .selected
        .as_ref()
        .and_then(|key| form.color(editor).map(|rgb| (key, rgb)))
    {
        let label = descriptor
            .keys
            .iter()
            .find(|candidate| &candidate.id == key)
            .map_or(key.as_str(), |key| key.label.as_str());
        content = content
            .push(text(format!("Color · {label}")))
            .push(color_picker::view(
                style,
                rgb,
                format!("picture:{key}"),
                Message::Picker,
                editable.then_some(Message::Color),
            ));
        for control in channels(rgb) {
            let input: Element<'a, Message> = if editable {
                slider(0..=255, control.value, move |value| {
                    Message::Channel(control.channel, value)
                })
                .into()
            } else {
                text(control.value.to_string()).into()
            };
            content = content.push(
                column![text(format!("{}: {}", control.label, control.value)), input]
                    .spacing(style.spacing.s),
            );
        }
    } else if let Some(Content::Opaque { reason }) = editor.baseline().map(|s| &s.content) {
        content = content.push(text(reason));
    } else if editor.draft().is_none() {
        content = content.push(text("Read per-key colors to load the current picture."));
    }
    panels::vertical_scroll(style, content).height(Fill).into()
}
