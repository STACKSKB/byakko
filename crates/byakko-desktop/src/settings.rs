//! Capability-driven scalar settings; the view knows no firmware fields.
use super::{Desktop, Message as AppMessage};
use crate::{
    control_widgets::{self, Choice},
    panels,
};
use byakko_core::settings::{
    Content, Edit, Field, Kind, Value,
    editor::{Editor, Status},
};
use iced::{
    Element, Fill,
    widget::{button, column, pick_list, row, scrollable, text},
};
use std::fmt;

#[derive(Clone, Debug)]
pub(super) enum Message {
    Select(String),
    Read,
    Apply,
    Revert,
    Edit(Edit),
}

impl Desktop {
    pub(super) fn update_settings(&mut self, message: Message) {
        if self.busy() {
            return;
        }
        match message {
            Message::Select(id) => {
                if self.session.settings().is_some_and(|editor| {
                    editor
                        .capabilities()
                        .fields
                        .iter()
                        .any(|field| field.id == id)
                }) {
                    self.settings_selected = Some(id);
                }
            }
            Message::Read => {
                let request = self.session.request_settings_read();
                self.submit(request);
            }
            Message::Apply => {
                let request = self.session.request_setting_apply();
                self.submit(request);
            }
            Message::Revert => self.notice = self.session.revert_settings().err(),
            Message::Edit(edit) => self.notice = self.session.edit_setting(edit).err(),
        }
    }
}

pub(super) fn view(app: &Desktop) -> Element<'_, AppMessage> {
    let Some(editor) = app.session.settings() else {
        return text("Settings are unavailable on this device").into();
    };
    let editable = !app.busy() && *editor.status() == Status::Ready && editor.draft().is_some();
    let mut content = column![toolbar(app, editor, editable), text(status(app, editor))]
        .spacing(app.ui.spacing.m);
    let Some(draft) = editor.draft() else {
        if let Some(snapshot) = editor.baseline()
            && let Content::Opaque { reason } = &snapshot.content
        {
            content = content.push(text(reason));
        }
        return content.into();
    };
    let fields = &editor.capabilities().fields;
    let selected = app
        .settings_selected
        .as_ref()
        .filter(|id| draft.contains_key(*id))
        .or_else(|| fields.first().map(|field| &field.id));
    let Some(selected) = selected else {
        return content.push(text("No editable settings")).into();
    };
    let field = fields
        .iter()
        .find(|field| &field.id == selected)
        .expect("catalog field");
    let value = &draft[selected];
    let editable_field = editable
        && editor
            .changes()
            .first()
            .is_none_or(|change| change.id == field.id);
    let entries: Vec<_> = fields
        .iter()
        .filter_map(|field| {
            let value = draft.get(&field.id)?;
            Some(Choice {
                label: format!("{} · {}", field.label, display_value(field, value)),
                selected: &field.id == selected,
                message: (!app.busy())
                    .then(|| AppMessage::Settings(Message::Select(field.id.clone()))),
            })
        })
        .collect();
    let style = &app.ui;
    let workbench = panels::split(
        style,
        move || {
            panels::panel(
                style,
                "Available settings",
                scrollable(control_widgets::choices(style, "Controls", entries.clone()))
                    .height(Fill)
                    .into(),
            )
        },
        move || {
            panels::panel(
                style,
                field.label.clone(),
                control(style, field, value, editable_field),
            )
        },
    );
    content.push(workbench).height(Fill).into()
}

fn toolbar<'a>(app: &Desktop, editor: &Editor, editable: bool) -> Element<'a, AppMessage> {
    row![
        button("Read settings")
            .on_press_maybe((!app.busy()).then_some(AppMessage::Settings(Message::Read))),
        button("Revert draft").on_press_maybe(
            (!app.busy() && editor.dirty()).then_some(AppMessage::Settings(Message::Revert))
        ),
        button("Apply & verify").on_press_maybe(
            (editable && editor.dirty()).then_some(AppMessage::Settings(Message::Apply))
        ),
        text(if editor.dirty() {
            "One setting staged"
        } else {
            "No staged changes"
        }),
    ]
    .spacing(app.ui.spacing.m)
    .align_y(iced::Center)
    .into()
}

fn status(app: &Desktop, editor: &Editor) -> String {
    if app.busy() {
        return super::view::status(app);
    }
    match editor.status() {
        Status::Unloaded => "Read settings to begin".into(),
        Status::Ready => "Readback verified · one setting can be staged at a time".into(),
        Status::Conflict { .. } => "Settings changed since the draft began. Draft retained; revert it, then read again to use device values.".into(),
        Status::Unverified { problem } => super::view::problem_label(problem),
    }
}

fn control(
    style: &panels::UiStyle,
    field: &Field,
    value: &Value,
    editable: bool,
) -> Element<'static, AppMessage> {
    match (&field.kind, value) {
        (Kind::Toggle, Value::Toggle(current)) => control_widgets::choices(
            style,
            "Value",
            [("Enabled", true), ("Disabled", false)]
                .into_iter()
                .map(|(label, selected)| Choice {
                    label: label.into(),
                    selected: *current == selected,
                    message: editable.then(|| {
                        AppMessage::Settings(Message::Edit(Edit {
                            id: field.id.clone(),
                            value: Value::Toggle(selected),
                        }))
                    }),
                }),
        ),
        (Kind::Number { .. }, Value::Number(current)) => {
            let options = number_options(&field.kind);
            let selected = options
                .iter()
                .find(|option| option.value == *current)
                .cloned();
            let id = field.id.clone();
            let picker = pick_list(options, selected, move |option: NumberOption| {
                AppMessage::Settings(Message::Edit(Edit {
                    id: id.clone(),
                    value: Value::Number(option.value),
                }))
            })
            .width(style.fields.regular);
            if editable {
                column![text("Value"), picker]
                    .spacing(style.spacing.s)
                    .into()
            } else {
                text(display_value(field, value)).into()
            }
        }
        _ => text("Setting value does not match its capability").into(),
    }
}

fn display_value(field: &Field, value: &Value) -> String {
    match (&field.kind, value) {
        (Kind::Toggle, Value::Toggle(true)) => "Enabled".into(),
        (Kind::Toggle, Value::Toggle(false)) => "Disabled".into(),
        (
            Kind::Number {
                disabled_zero: true,
                ..
            },
            Value::Number(0),
        ) => "Disabled".into(),
        (Kind::Number { unit, .. }, Value::Number(value)) => format!("{value} {unit}"),
        _ => "Unknown value".into(),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NumberOption {
    value: u16,
    label: String,
}

impl fmt::Display for NumberOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

fn number_options(kind: &Kind) -> Vec<NumberOption> {
    let Kind::Number {
        min,
        max,
        step,
        unit,
        disabled_zero,
    } = kind
    else {
        return Vec::new();
    };
    let mut options = Vec::new();
    if *disabled_zero {
        options.push(NumberOption {
            value: 0,
            label: "Disabled".into(),
        });
    }
    options.extend(
        (*min..=*max)
            .step_by(*step as usize)
            .map(|value| NumberOption {
                value,
                label: format!("{value} {unit}"),
            }),
    );
    options
}
