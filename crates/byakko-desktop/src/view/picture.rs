//! The lighting workspace owns the board; per-key mode owns this compact brush picker.
use crate::{
    form::picture::{Form, Message},
    widget::{color_picker, keyboard, panels::UiStyle},
};
use byakko_core::{
    editor::{Editor, Status, picture::PictureRules},
    model::{keymap::Descriptor, picture::Content},
};
use iced::{
    Element,
    widget::{button, column, text},
};
use std::collections::BTreeMap;

pub fn board<'a>(
    form: &'a Form,
    descriptor: &'a Descriptor,
    editor: &'a Editor<PictureRules>,
    editable: bool,
    style: &'a UiStyle,
    labels: BTreeMap<String, keyboard::BoardLabel>,
) -> Element<'a, Message> {
    let editable = editable && editor.status() == &Status::Ready && editor.draft().is_some();
    let advertised = &editor.capabilities().keys;
    keyboard::colored_view_with_labels(
        style,
        descriptor.keys.iter().filter(|key| key.visible).collect(),
        form.selected.clone(),
        editor.draft().cloned().unwrap_or_default(),
        labels,
        move |key| {
            (editable && advertised.contains(&key.id)).then(|| Message::Select(key.id.clone()))
        },
    )
}

pub fn view<'a>(
    form: &'a Form,
    _descriptor: &'a Descriptor,
    editor: &'a Editor<PictureRules>,
    editable: bool,
    idle: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    if !matches!(editor.status(), Status::Ready) {
        let mut content = column![
            text("Stored colors need to be read before editing."),
            button("Retry color read").on_press_maybe(idle.then_some(Message::Read))
        ]
        .spacing(style.spacing.s);
        if editor.dirty() {
            content = content
                .push(button("Revert edits").on_press_maybe(idle.then_some(Message::Revert)));
        }
        return content.into();
    }
    let editable = editable && editor.status() == &Status::Ready && editor.draft().is_some();
    if let Some(rgb) = form.color(editor) {
        color_picker::view(
            style,
            rgb,
            "picture-brush".into(),
            Message::Picker,
            editable.then_some(Message::Color),
        )
    } else if let Some(Content::Opaque { reason }) = editor.baseline().map(|s| &s.content) {
        text(reason).into()
    } else if editor.draft().is_none() {
        text("Loading stored colors…").into()
    } else {
        text("Select a key to paint.").into()
    }
}
