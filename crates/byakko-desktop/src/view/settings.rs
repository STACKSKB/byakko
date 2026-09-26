//! Scalar fields permit one staged setting and preserve unfinished numeric input.
use crate::{
    form::settings::{Form, Message},
    widget::panels::UiStyle,
};
use byakko_core::{
    editor::{Editor, Feature, Status, settings::SettingsRules},
    model::settings::{Content, Edit, Kind, Value},
};
use iced::{
    Element, Fill,
    widget::{button, checkbox, column, row, scrollable, text, text_input},
};

pub fn view<'a>(
    form: &'a Form,
    editor: &'a Editor<SettingsRules>,
    editable: bool,
    idle: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    let editable = editable && editor.status() == &Status::Ready && editor.draft().is_some();
    let changes = editor.changes();
    let submitted = editor.submitted().and_then(|submitted| {
        let baseline = editor.baseline().and_then(SettingsRules::value)?;
        submitted
            .iter()
            .find(|(id, value)| baseline.get(*id) != Some(*value))
            .map(|(id, _)| id.as_str())
    });
    let pending = submitted.or_else(|| changes.first().map(|edit| edit.id.as_str()));
    let mut content = column![
        row![
            button("Read").on_press_maybe(idle.then_some(Message::Read)),
            button("Revert").on_press_maybe(
                (idle && (editor.dirty() || form.has_input())).then_some(Message::Revert)
            ),
            button("Apply now")
                .on_press_maybe((idle && editor.dirty() && editable).then_some(Message::Save)),
        ]
        .spacing(style.spacing.s),
        text("One setting at a time. Accepted values apply automatically."),
    ]
    .spacing(style.spacing.m);
    if let Some(values) = editor.draft() {
        for field in &editor.capabilities().fields {
            let can_edit = editable
                && pending.is_none_or(|id| id == field.id)
                && form.input.as_ref().is_none_or(|(id, _)| id == &field.id);
            let Some(value) = values.get(&field.id) else {
                continue;
            };
            let control: Element<'a, Message> = match (&field.kind, value) {
                (Kind::Toggle, Value::Toggle(value)) => {
                    let id = field.id.clone();
                    checkbox(*value)
                        .label(&field.label)
                        .on_toggle_maybe(can_edit.then_some(move |value| {
                            Message::Edit(Edit {
                                id: id.clone(),
                                value: Value::Toggle(value),
                            })
                        }))
                        .into()
                }
                (
                    Kind::Number {
                        min,
                        max,
                        step,
                        unit,
                        disabled_zero,
                    },
                    Value::Number(value),
                ) => {
                    let id = field.id.clone();
                    let shown = form
                        .input
                        .as_ref()
                        .filter(|(id, _)| id == &field.id)
                        .map_or_else(|| value.to_string(), |(_, input)| input.clone());
                    let input = text_input("Value", &shown)
                        .on_input_maybe(
                            can_edit.then_some(move |value| Message::Number(id.clone(), value)),
                        )
                        .on_submit_maybe(
                            (can_edit && form.input.is_some())
                                .then(|| Message::ApplyNumber(field.id.clone())),
                        )
                        .width(style.fields.compact);
                    let range = format!(
                        "{min}–{max} {unit}, step {step}{}",
                        if *disabled_zero { "; 0 = disabled" } else { "" }
                    );
                    column![
                        text(&field.label),
                        row![
                            input,
                            button("Set").on_press_maybe(
                                (can_edit && form.input.is_some())
                                    .then(|| Message::ApplyNumber(field.id.clone()))
                            ),
                            text(unit),
                        ]
                        .spacing(style.spacing.s),
                        text(range)
                    ]
                    .spacing(style.spacing.s)
                    .into()
                }
                _ => text(format!("{}: unavailable", field.label)).into(),
            };
            content = content.push(control);
        }
    } else {
        let explanation = match editor.baseline().map(|s| &s.content) {
            Some(Content::Opaque { reason }) => reason.as_str(),
            _ => "Read settings to load the available fields.",
        };
        content = content.push(text(explanation));
    }
    scrollable(content).height(Fill).into()
}
