//! Binding choices are advertised actions, not UI-generated firmware codes.
use super::{Desktop, Message, Page, macro_editor::Message as Macro, panels};
use byakko_core::{
    macros::{
        Edit,
        editor::{Editor, Status as MacroStatus},
    },
    session::Status,
};
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
    let layer = descriptor.layers.iter().find(|layer| layer.id == app.layer);
    let target = match (key, layer) {
        (Some(key), Some(layer)) => format!("Bind to {} / {}", layer.label, key.label),
        _ => "Choose a target on the Keys page".into(),
    };
    let ready = !app.busy()
        && *app.session.status() == Status::Ready
        && key.is_some_and(|key| key.writable);
    let editable =
        !app.busy() && *editor.status() == MacroStatus::Ready && editor.draft().is_some();
    let modes = row(choices.into_iter().map(|choice| {
        let result = editor.binding_action(&choice.id);
        let mut mode = column![button(text(&choice.label)).on_press_maybe(
            (ready && result.is_ok()).then(|| Message::Macro(Macro::Bind(choice.id.clone())))
        ),]
        .spacing(app.ui.spacing.xs);
        if let Some(required) = choice.required_repeat_count {
            mode = mode.push(
                text(format!("Requires saved count {required}")).size(app.ui.type_scale.body),
            );
            if editor
                .draft()
                .is_some_and(|program| program.repeat_count != required)
            {
                mode = mode.push(
                    button(text(format!("Stage count {required}"))).on_press_maybe(
                        editable.then_some(Message::Macro(Macro::Edit(Edit::Repeat(required)))),
                    ),
                );
            }
        }
        mode.into()
    }))
    .spacing(app.ui.spacing.m);
    let mut content = column![
        row![
            text(target),
            button("Choose key").on_press(Message::Page(Page::Keys)),
        ]
        .spacing(app.ui.spacing.m),
        modes,
        text("Binding stages a keymap change. Review and apply it on Keys.")
            .size(app.ui.type_scale.body)
    ]
    .spacing(app.ui.spacing.s);
    if *app.session.status() != Status::Ready {
        content = content.push(
            row![
                text("Read keymaps, then read this slot before binding."),
                button("Read keymaps").on_press_maybe((!app.busy()).then_some(Message::Read)),
            ]
            .spacing(app.ui.spacing.s),
        );
    } else if editor.dirty() {
        content = content.push(
            text("Save the macro before binding. Its count affects every key using this slot.")
                .size(app.ui.type_scale.body),
        );
    }
    panels::panel(&app.ui, "Key binding", content.into())
}
