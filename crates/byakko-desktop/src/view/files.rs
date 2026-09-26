//! Portable macro documents and local names, separate from device save controls.
use crate::{
    form::files::{Form, Message, Operation},
    widget::panels::UiStyle,
};
use byakko_core::editor::{Editor, Status, macros::MacroRules};
use iced::{
    Element,
    widget::{button, column, row, text, text_input},
};

pub fn macros<'a>(
    form: &'a Form,
    editor: &'a Editor<MacroRules>,
    idle: bool,
    names_available: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    let editable = idle && editor.draft().is_some();
    let mut names = row![
        text("Name"),
        text_input("Macro name", form.name(editor.slot()))
            .on_input_maybe(editable.then_some(Message::Name))
            .width(style.fields.regular)
    ]
    .spacing(style.spacing.s);
    if names_available {
        names = names
            .push(button("Save name").on_press_maybe(
                (idle && form.labels_dirty()).then_some(Message::Begin(Operation::SaveLabels)),
            ))
            .push(
                button("Reload names")
                    .on_press_maybe(idle.then_some(Message::Begin(Operation::LoadLabels))),
            );
    }
    column![
        names,
        row![
            text_input("Path to macro JSON", &form.macro_path)
                .on_input_maybe(idle.then_some(Message::MacroPath)),
            button("Import draft").on_press_maybe(
                (editable && editor.status() == &Status::Ready)
                    .then_some(Message::Begin(Operation::ImportMacro))
            ),
            button("Export new file")
                .on_press_maybe(editable.then_some(Message::Begin(Operation::ExportMacro))),
        ]
        .spacing(style.spacing.s),
        text("Import replaces the selected draft. Export creates a new file. Names are local."),
    ]
    .spacing(style.spacing.s)
    .into()
}
